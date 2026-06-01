"""scripts/event_watch_debug —— Event Watch 扫描调试脚本

独立运行，复现 ClawGO Rust event_scanner 的完整流程，逐步打印诊断日志。
参考 OpenInvest 的多源新闻架构：RSS + DuckDuckGo + Yahoo Finance + Tushare。

用法：
    python scripts/event_watch_debug.py              # 完整扫描（fetch + filter + LLM + save）
    python scripts/event_watch_debug.py --dry-run     # 只打印不入库
    python scripts/event_watch_debug.py --no-llm      # 跳过 LLM，只看关键词过滤结果
    python scripts/event_watch_debug.py --sources     # 列出所有数据源

配置读取：
    ~/.claw-go/settings.json       → tushare_token, tushare_proxy_url
    ~/.claw-go/invest/llm_config.json → LLM provider (api_key, base_url, model)
"""
from __future__ import annotations

import argparse
import json
import logging
import os
import re
import sqlite3
import sys
import uuid
import xml.etree.ElementTree as ET
from concurrent.futures import ThreadPoolExecutor, as_completed
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

import requests

log = logging.getLogger("event_watch_debug")

# ── Paths ──

def claw_go_dir() -> Path:
    return Path.home() / ".claw-go"

def settings_path() -> Path:
    return claw_go_dir() / "settings.json"

def llm_config_path() -> Path:
    return claw_go_dir() / "invest" / "llm_config.json"

def invest_db_path() -> Path:
    return claw_go_dir() / "invest" / "invest.db"

# ── Config loading ──

def load_tushare_config() -> Tuple[str, str]:
    """返回 (token, base_url)"""
    p = settings_path()
    if not p.exists():
        raise FileNotFoundError(f"settings.json not found: {p}")
    data = json.loads(p.read_text("utf-8"))
    user = data.get("user", data)
    token = user.get("tushare_token", "")
    if not token:
        raise ValueError("tushare_token not configured in settings.json")
    proxy_url = user.get("tushare_proxy_url") or ""
    base_url = proxy_url.rstrip("/") if proxy_url else "http://api.tushare.pro"
    return token, base_url

def load_llm_config() -> Dict[str, Any]:
    """返回 {api_key, base_url, model, timeout_secs}
    支持两种配置格式：
    - providers 数组格式（Rust 端写入）
    - 扁平 {provider_id: {api_key, ...}} 格式（前端写入）
    """
    p = llm_config_path()
    if not p.exists():
        raise FileNotFoundError(f"llm_config.json not found: {p}")
    data = json.loads(p.read_text("utf-8"))
    timeout = data.get("timeout_secs", 120)
    selected = data.get("selected_provider", "deepseek")

    # 格式 1: providers 数组
    providers = data.get("providers", [])
    if providers:
        provider = None
        for p_cfg in providers:
            if p_cfg.get("provider_id") == selected and p_cfg.get("api_key"):
                provider = p_cfg
                break
        if not provider:
            for p_cfg in providers:
                if p_cfg.get("api_key"):
                    provider = p_cfg
                    break
        if provider:
            return {
                "api_key": provider["api_key"],
                "base_url": provider["base_url"],
                "model": provider["default_model"],
                "timeout_secs": timeout,
            }

    # 格式 2: 扁平 {provider_id: {api_key, base_url, default_model}}
    for pid in [selected, "deepseek", "mimo_plan", "mimo_api"]:
        cfg = data.get(pid)
        if cfg and isinstance(cfg, dict) and cfg.get("api_key"):
            return {
                "api_key": cfg["api_key"],
                "base_url": cfg["base_url"],
                "model": cfg.get("default_model", "deepseek-v4-pro"),
                "timeout_secs": timeout,
            }

    raise ValueError("No LLM provider with api_key configured in llm_config.json")

# ── Tushare client (带 429 重试 + 指数退避，与 Rust 端对齐) ──

import time as _time

class TushareClient:
    MAX_RETRIES = 3
    REQUEST_INTERVAL = 0.35  # 请求间隔，避免触发限流

    def __init__(self, token: str, base_url: str):
        self.token = token
        self.base_url = base_url
        self._last_request_time = 0.0

    def call_api(self, api_name: str, params: dict, fields: str = "") -> dict:
        """调用 Tushare API，429/5xx 自动重试 3 次，指数退避 1s/2s/4s。"""
        payload = {
            "api_name": api_name,
            "token": self.token,
            "params": params,
            "fields": fields,
        }
        last_err = None
        for attempt in range(self.MAX_RETRIES):
            # 请求间隔限流
            elapsed = _time.monotonic() - self._last_request_time
            if elapsed < self.REQUEST_INTERVAL:
                _time.sleep(self.REQUEST_INTERVAL - elapsed)

            resp = requests.post(self.base_url, json=payload, timeout=30)
            self._last_request_time = _time.monotonic()

            # 429 / 5xx → 重试
            if resp.status_code == 429 or resp.status_code >= 500:
                last_err = f"HTTP {resp.status_code} (attempt {attempt+1}/{self.MAX_RETRIES})"
                if attempt + 1 < self.MAX_RETRIES:
                    backoff = 1.0 * (2 ** attempt)  # 1s, 2s, 4s
                    log.debug(f"  Tushare {api_name} {last_err}, retry in {backoff}s")
                    _time.sleep(backoff)
                continue

            resp.raise_for_status()
            data = resp.json()
            if data.get("code") != 0:
                raise RuntimeError(f"Tushare {api_name} error: {data.get('msg', 'unknown')}")
            return data

        raise RuntimeError(f"Tushare {api_name} max retries exceeded: {last_err}")

    def major_news(self, src: str, start_date: str, end_date: str) -> List[Dict]:
        resp = self.call_api("major_news", {
            "src": src,
            "start_date": start_date,
            "end_date": end_date,
        })
        fields = resp["data"]["fields"]
        return [dict(zip(fields, row)) for row in resp["data"]["items"]]

    def anns_d(self, symbol: str, start_date: str, end_date: str) -> List[Dict]:
        resp = self.call_api("anns_d", {
            "ts_code": symbol,
            "start_date": start_date,
            "end_date": end_date,
        })
        fields = resp["data"]["fields"]
        return [dict(zip(fields, row)) for row in resp["data"]["items"]]

    def get_latest_price(self, ts_code: str) -> Optional[float]:
        prefix = ts_code.split(".")[0]
        is_etf = prefix[:3] in (
            "159", "510", "515", "588", "150", "500", "501",
            "160", "161", "162", "163", "164",
        )
        api = "fund_daily" if is_etf else "daily"
        resp = self.call_api(api, {"ts_code": ts_code}, "ts_code,trade_date,close")
        fields = resp["data"]["fields"]
        items = resp["data"]["items"]
        if not items:
            return None
        close_idx = fields.index("close") if "close" in fields else -1
        if close_idx < 0:
            return None
        try:
            return float(items[0][close_idx])
        except (ValueError, TypeError, IndexError):
            return None

# ── RSS feeds (参考 OpenInvest rss_feeds.yml) ──

RSS_FEEDS = [
    {"name": "bbc_business", "url": "https://feeds.bbci.co.uk/news/business/rss.xml"},
    {"name": "ft_markets", "url": "https://www.ft.com/markets?format=rss"},
    {"name": "yahoo_finance", "url": "https://finance.yahoo.com/news/rssindex"},
    {"name": "seeking_alpha", "url": "https://seekingalpha.com/market_currents.xml"},
]

def fetch_rss_feed(name: str, url: str, max_items: int = 20) -> List[Dict]:
    """解析单个 RSS feed，返回 RawEvent 列表"""
    items = []
    try:
        resp = requests.get(url, timeout=15, headers={
            "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36"
        })
        resp.raise_for_status()
        root = ET.fromstring(resp.content)

        # 支持 RSS 2.0 和 Atom
        ns = {"atom": "http://www.w3.org/2005/Atom"}
        entries = root.findall(".//item") or root.findall(".//atom:entry", ns)

        for entry in entries[:max_items]:
            title = ""
            link = ""
            summary = ""
            published = ""

            # RSS 2.0
            if entry.find("title") is not None:
                title = (entry.find("title").text or "").strip()
            if entry.find("link") is not None:
                link = (entry.find("link").text or "").strip()
            if entry.find("description") is not None:
                summary = _strip_html((entry.find("description").text or "").strip())
            if entry.find("pubDate") is not None:
                published = (entry.find("pubDate").text or "").strip()

            # Atom
            if not title and entry.find("atom:title", ns) is not None:
                title = (entry.find("atom:title", ns).text or "").strip()
            if not link:
                link_el = entry.find("atom:link", ns)
                if link_el is not None:
                    link = link_el.get("href", "")
            if not summary and entry.find("atom:summary", ns) is not None:
                summary = _strip_html((entry.find("atom:summary", ns).text or "").strip())

            if not title:
                continue

            items.append({
                "source": f"rss:{name}",
                "event_type": "news",
                "title": title,
                "body": summary[:500] if summary else title,
                "url": link,
                "created_at": published or datetime.now().isoformat(),
            })
    except Exception as e:
        log.warning(f"RSS {name} failed: {e}")
    return items

def _strip_html(s: str) -> str:
    return re.sub(r"<[^>]+>", " ", s).strip()

# ── Yahoo Finance search (与 Rust 端 providers/yahoo.py 相同) ──

def fetch_yahoo_news(query: str, count: int = 10) -> List[Dict]:
    """Yahoo Finance search API"""
    items = []
    try:
        url = "https://query1.finance.yahoo.com/v1/finance/search"
        params = {
            "q": query,
            "quotesCount": 0,
            "newsCount": count,
            "enableFuzzyQuery": False,
            "quotesQueryId": "tss_match_phrase_query",
        }
        headers = {"User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36"}
        resp = requests.get(url, params=params, headers=headers, timeout=15)
        resp.raise_for_status()
        data = resp.json()

        for item in data.get("news", []):
            title = item.get("title", "")
            if not title:
                continue
            ts = item.get("providerPublishTime", 0)
            created_at = (
                datetime.fromtimestamp(ts, tz=timezone.utc).strftime("%Y-%m-%dT%H:%M:%S")
                if ts > 0 else datetime.now().strftime("%Y-%m-%dT%H:%M:%S")
            )
            items.append({
                "source": "yahoo_finance",
                "event_type": "news",
                "title": title,
                "body": f"Publisher: {item.get('publisher', '')}",
                "url": item.get("link", ""),
                "created_at": created_at,
            })
    except Exception as e:
        log.warning(f"Yahoo Finance query '{query}' failed: {e}")
    return items

# ── DuckDuckGo news (参考 OpenInvest ddgs_news.py) ──

def fetch_ddgs_news(query: str, max_results: int = 15) -> List[Dict]:
    """DuckDuckGo 新闻搜索"""
    try:
        from ddgs import DDGS
    except ImportError:
        # 尝试旧版包名
        try:
            from duckduckgo_search import DDGS
        except ImportError:
            log.warning("ddgs/duckduckgo_search 未安装，跳过 DDGS")
            return []

    items = []
    try:
        with DDGS() as ddgs:
            results = ddgs.news(query, region="wt-wt", safesearch="off", max_results=max_results) or []
        for r in results:
            url = (r.get("url") or "").strip()
            title = (r.get("title") or "").strip()
            if not url or not title:
                continue
            body = (r.get("body") or r.get("snippet") or "")[:300]
            published = (r.get("date") or "").strip()
            items.append({
                "source": f"ddgs:{query[:20]}",
                "event_type": "news",
                "title": title,
                "body": body,
                "url": url,
                "created_at": published or datetime.now().isoformat(),
            })
    except Exception as e:
        log.warning(f"DDGS query '{query}' failed: {e}")
    return items

# ── Holdings ──

def load_active_holdings() -> List[str]:
    db_path = invest_db_path()
    if not db_path.exists():
        log.warning(f"invest.db not found: {db_path}")
        return []
    conn = sqlite3.connect(str(db_path))
    try:
        rows = conn.execute(
            "SELECT symbol FROM holdings WHERE kind IN ('hold', 'watch')"
        ).fetchall()
        return [r[0] for r in rows]
    except Exception as e:
        log.warning(f"读取持仓失败: {e}")
        return []
    finally:
        conn.close()

# ── Keyword filtering (扩展版：中英双语，覆盖全球宏观) ──

HIGH_KEYWORDS = [
    # 中文 — 央行/宏观政策/市场剧烈波动
    "央行", "降准", "降息", "加息", "MLF", "LPR", "逆回购",
    "暴跌", "熔断", "ST", "退市", "暂停上市", "重大违法",
    "关税", "制裁", "禁令", "反垄断", "行业整顿",
    # 英文 — 央行/宏观政策/市场剧烈波动（短语匹配，避免误判）
    "rate cut", "rate hike", "interest rate", "inflation",
    "tariff", "sanction", "antitrust",
    "circuit breaker", "bankruptcy", "default",
    "trade war", "geopolitical",
    "fed reserve", "federal reserve", "fed decision",
    "recession risk", "market crash",
]

MEDIUM_KEYWORDS = [
    # 中文 — 业绩/资本运作
    "财报", "业绩预告", "净利润", "营收",
    "增持", "减持", "回购", "定增", "分红",
    "产能", "订单", "并购", "重组",
    # 英文 — 业绩/资本运作（短语匹配）
    "earnings", "dividend", "buyback",
    "merger", "acquisition",
    "layoff", "ipo",
    "gdp growth", "jobs report", "pmi data", "cpi data",
]

def classify_severity(title: str, body: str) -> Optional[str]:
    """返回 'high' / 'medium' / None（LOW 被过滤）
    中文关键词用子串匹配，英文关键词用单词边界匹配避免误判。"""
    text = title + " " + body
    text_lower = text.lower()
    for kw in HIGH_KEYWORDS:
        if _kw_match(kw, text, text_lower):
            return "high"
    for kw in MEDIUM_KEYWORDS:
        if _kw_match(kw, text, text_lower):
            return "medium"
    return None

def _kw_match(kw: str, text: str, text_lower: str) -> bool:
    """中文关键词用子串匹配，英文关键词用单词边界匹配。"""
    if any('一' <= c <= '鿿' for c in kw):
        return kw in text
    # 英文：用正则单词边界
    return bool(re.search(r'\b' + re.escape(kw) + r'\b', text_lower))

# ── LLM normalization ──

NORMALIZER_PROMPT = """你是一个A股财经新闻分析师。对以下新闻/公告进行结构化提取。

对每条新闻输出一个JSON数组，每个元素包含：
- title_cn: 中文标题（如果原标题已是中文则保持不变，英文则翻译为简洁中文）
- one_line_claim: 一句话中文摘要（≤30字）
- stance: bullish / bearish / neutral
- severity: high / medium / low
- affected_symbols: 涉及的A股代码数组（6位数字格式，如 "600519"）

只输出JSON数组，不要其他文字。"""

def normalize_events(
    raw_events: List[Dict],
    llm_config: Dict[str, Any],
    system_prompt: Optional[str] = None,
) -> List[Dict]:
    if not raw_events:
        return []

    items_text = ""
    for i, ev in enumerate(raw_events):
        body = ev.get("body") or ev.get("title", "")
        items_text += f"\n[{i+1}] source={ev.get('source','')} type={ev.get('event_type','')} title={ev.get('title','')}\n{body}\n"

    system = system_prompt or NORMALIZER_PROMPT

    try:
        from openai import OpenAI
        client = OpenAI(
            api_key=llm_config["api_key"],
            base_url=llm_config["base_url"],
            timeout=llm_config.get("timeout_secs", 120),
        )
        resp = client.chat.completions.create(
            model=llm_config["model"],
            messages=[
                {"role": "system", "content": system},
                {"role": "user", "content": items_text},
            ],
            temperature=0.7,
            max_tokens=4096,
        )
        content = resp.choices[0].message.content or ""
    except ImportError:
        log.error("openai 库未安装，无法调用 LLM")
        return [_fallback_normalize(ev) for ev in raw_events]
    except Exception as e:
        log.warning(f"LLM 调用失败: {e}，使用规则回退")
        return [_fallback_normalize(ev) for ev in raw_events]

    return _parse_normalized_response(content, raw_events)

def _parse_normalized_response(content: str, raw_events: List[Dict]) -> List[Dict]:
    text = content.strip()
    if text.startswith("```"):
        lines = text.split("\n")
        lines = [l for l in lines[1:] if l.strip() != "```"]
        text = "\n".join(lines).strip()

    try:
        results = json.loads(text)
    except json.JSONDecodeError as e:
        log.warning(f"LLM JSON 解析失败: {e}; raw[:200]={content[:200]}")
        return [_fallback_normalize(ev) for ev in raw_events]

    if isinstance(results, dict):
        results = results.get("events", results.get("data", []))
    if not isinstance(results, list):
        log.warning(f"LLM 返回非 list: {type(results)}")
        return [_fallback_normalize(ev) for ev in raw_events]

    results = results[:len(raw_events)]
    while len(results) < len(raw_events):
        idx = len(results)
        results.append(_fallback_normalize(raw_events[idx]))
    return results

def _fallback_normalize(ev: Dict) -> Dict:
    severity = classify_severity(ev.get("title", ""), ev.get("body", "")) or "low"
    title = ev.get("title", "")
    return {
        "title_cn": title,  # 回退时保持原标题
        "one_line_claim": title[:30],
        "stance": "neutral",
        "severity": severity,
        "affected_symbols": [],
    }

# ── Event storage ──

def ensure_events_table(conn: sqlite3.Connection):
    conn.execute("""
        CREATE TABLE IF NOT EXISTS events (
            id TEXT PRIMARY KEY,
            source TEXT NOT NULL,
            event_type TEXT NOT NULL,
            title TEXT NOT NULL,
            body TEXT,
            symbols TEXT,
            severity TEXT NOT NULL,
            stance TEXT,
            triggered INTEGER NOT NULL DEFAULT 0,
            trigger_verdict_id TEXT,
            created_at TEXT NOT NULL
        )
    """)
    conn.execute("""
        CREATE UNIQUE INDEX IF NOT EXISTS idx_events_source_title
        ON events(source, title)
    """)
    conn.commit()

def save_event(conn: sqlite3.Connection, event: Dict) -> Tuple[bool, str]:
    try:
        conn.execute(
            """INSERT OR IGNORE INTO events
               (id, source, event_type, title, body, symbols, severity, stance, triggered, trigger_verdict_id, created_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)""",
            (
                event["id"], event["source"], event["event_type"],
                event["title"], event.get("body"), event.get("symbols"),
                event["severity"], event.get("stance"),
                0, None, event["created_at"],
            ),
        )
        conn.commit()
        if conn.execute("SELECT changes()").fetchone()[0] > 0:
            return True, "inserted"
        else:
            return False, "duplicate (source+title)"
    except Exception as e:
        return False, str(e)

# ── Main scan flow ──

def run_scan(
    *,
    dry_run: bool = False,
    no_llm: bool = False,
) -> Dict[str, Any]:
    """执行完整扫描流程，逐步打印诊断日志。"""

    # ── Step 0: Load config ──
    log.info("=" * 60)
    log.info("Step 0: 加载配置")

    tushare = None
    try:
        tushare_token, tushare_base = load_tushare_config()
        tushare = TushareClient(tushare_token, tushare_base)
        log.info(f"  Tushare base_url: {tushare_base}")
    except Exception as e:
        log.warning(f"  Tushare 配置加载失败: {e}")

    llm_cfg = None
    if not no_llm:
        try:
            llm_cfg = load_llm_config()
            log.info(f"  LLM provider: {llm_cfg['base_url']}, model: {llm_cfg['model']}")
        except Exception as e:
            log.warning(f"  LLM 配置加载失败: {e}，将使用规则回退")

    # ── Step 1: Fetch from multiple sources (并发) ──
    log.info("=" * 60)
    log.info("Step 1: 多源新闻抓取")

    now = datetime.now()
    today = now.strftime("%Y%m%d")
    one_day_ago = (now - timedelta(days=1)).strftime("%Y%m%d")

    raw_events: List[Dict] = []
    errors: List[str] = []
    holdings = load_active_holdings()
    log.info(f"  活跃持仓: {holdings}")

    # 构建并发任务列表
    tasks = []

    # 1a. Tushare major_news (sina + cls)
    if tushare:
        for src in ["sina", "cls"]:
            tasks.append(("tushare_major_news:" + src, lambda s=src: _fetch_tushare_news(tushare, s, one_day_ago, today)))

        # 1b. Tushare announcements for holdings
        for symbol in holdings:
            tasks.append(("tushare_anns:" + symbol, lambda sym=symbol: _fetch_tushare_anns(tushare, sym, one_day_ago, today)))

    # 1c. RSS feeds (免费，无需 API key)
    for feed in RSS_FEEDS:
        tasks.append((f"rss:{feed['name']}", lambda f=feed: fetch_rss_feed(f["name"], f["url"], 20)))

    # 1d. Yahoo Finance search (全球宏观查询，不限于 A 股)
    yahoo_queries = [
        "China stock market",
        "Fed interest rate",
        "US China trade",
        "global economy recession",
        "A股 央行",
    ]
    for q in yahoo_queries:
        tasks.append((f"yahoo:{q[:20]}", lambda query=q: fetch_yahoo_news(query, 10)))

    # 1e. DuckDuckGo news (如果安装了 ddgs)
    ddgs_queries = [
        "China economy policy",
        "Federal Reserve rate decision",
        "global market crash",
    ]
    for q in ddgs_queries:
        tasks.append((f"ddgs:{q[:20]}", lambda query=q: fetch_ddgs_news(query, 10)))

    # 并发执行所有任务
    log.info(f"  启动 {len(tasks)} 个数据源任务...")
    with ThreadPoolExecutor(max_workers=min(8, len(tasks))) as pool:
        futures = {pool.submit(fn): label for label, fn in tasks}
        try:
            for fut in as_completed(futures, timeout=45):
                label = futures[fut]
                try:
                    items = fut.result()
                    raw_events.extend(items)
                    log.info(f"  ✓ {label}: {len(items)} 条")
                except Exception as e:
                    msg = f"{label}: {e}"
                    log.warning(f"  ✗ {msg}")
                    errors.append(msg)
        except Exception:
            unfinished = [futures[f] for f in futures if not f.done()]
            log.warning(f"  超时，未完成: {unfinished}")

    # 去重（同 url）
    seen_urls = set()
    deduped = []
    for ev in raw_events:
        url = ev.get("url")
        if url and url in seen_urls:
            continue
        if url:
            seen_urls.add(url)
        deduped.append(ev)
    raw_events = deduped

    fetched = len(raw_events)
    log.info(f"  总计原始事件: {fetched}（去重后）")

    # ── Step 2: Keyword filtering ──
    log.info("=" * 60)
    log.info("Step 2: 关键词过滤（中英双语，覆盖全球宏观）")

    filtered_events = []
    for ev in raw_events:
        severity = classify_severity(ev["title"], ev.get("body", ""))
        if severity:
            ev["keyword_severity"] = severity
            filtered_events.append(ev)
            log.info(f"  ✓ [{severity.upper():6s}] [{ev['source'][:15]}] {ev['title'][:55]}")
        else:
            log.debug(f"  ✗ [DROP ] [{ev['source'][:15]}] {ev['title'][:55]}")

    filtered = len(filtered_events)
    log.info(f"  过滤结果: {filtered} 保留, {fetched - filtered} 丢弃")

    if not filtered_events:
        log.info("  无事件通过关键词过滤，结束")
        return {"fetched": fetched, "filtered": 0, "normalized": 0, "saved": 0, "errors": errors}

    # ── Step 2.5: DB 去重（在 LLM 之前，节省 token）──
    log.info("=" * 60)
    log.info("Step 2.5: DB 去重（跳过已存在的事件）")

    db_path = invest_db_path()
    existing_titles: set = set()
    if db_path.exists():
        try:
            conn = sqlite3.connect(str(db_path))
            rows = conn.execute("SELECT source, title FROM events").fetchall()
            existing_titles = {(r[0], r[1]) for r in rows}
            conn.close()
            log.info(f"  DB 中已有 {len(existing_titles)} 条事件")
        except Exception as e:
            log.warning(f"  读取 DB 失败: {e}")

    new_events = []
    for ev in filtered_events:
        key = (ev["source"], ev["title"])
        if key in existing_titles:
            log.info(f"  [seen] 已存在，跳过 — {ev['title'][:50]}")
        else:
            new_events.append(ev)

    skipped_by_dedup = len(filtered_events) - len(new_events)
    log.info(f"  新事件: {len(new_events)}, 已存在: {skipped_by_dedup}")

    if not new_events:
        log.info("  无新事件，结束")
        return {"fetched": fetched, "filtered": filtered, "normalized": 0, "saved": 0, "errors": errors}

    filtered_events = new_events

    # ── Step 3: LLM normalization + 翻译 ──
    log.info("=" * 60)
    log.info("Step 3: LLM 归一化")

    if no_llm or llm_cfg is None:
        log.info("  跳过 LLM（--no-llm 或配置缺失），使用规则回退")
        normalized = [_fallback_normalize(ev) for ev in filtered_events]
    else:
        log.info(f"  调用 LLM: {llm_cfg['model']}")
        normalized = normalize_events(filtered_events, llm_cfg)

    for ev, norm in zip(filtered_events, normalized):
        sev = norm.get("severity", "?")
        stance = norm.get("stance", "?")
        claim = norm.get("one_line_claim", "")
        title_cn = norm.get("title_cn", "")
        symbols = norm.get("affected_symbols", [])
        skip_mark = " ⏩ SKIP" if sev == "low" else ""
        log.info(f"  [{sev.upper():6s}] stance={stance} symbols={symbols}{skip_mark}")
        log.info(f"           中文: {title_cn[:60]}")
        log.info(f"           摘要: {claim[:60]}")
        if title_cn != ev["title"]:
            log.info(f"           原文: {ev['title'][:60]}")

    # ── Step 4: Save to DB ──
    log.info("=" * 60)
    log.info("Step 4: 保存到数据库")

    if dry_run:
        log.info("  --dry-run 模式，跳过数据库写入")
        saved = 0
        for ev, norm in zip(filtered_events, normalized):
            title_cn = norm.get("title_cn", ev["title"])
            if norm.get("severity") == "low":
                log.info(f"  [skip] LOW — {title_cn[:50]}")
            else:
                saved += 1
                log.info(f"  [would save] {title_cn[:50]}")
        return {"fetched": fetched, "filtered": filtered, "normalized": len(normalized), "saved": saved, "errors": errors}

    db_path = invest_db_path()
    if not db_path.exists():
        log.error(f"  invest.db 不存在: {db_path}")
        return {"fetched": fetched, "filtered": filtered, "normalized": len(normalized), "saved": 0, "errors": errors + ["invest.db not found"]}

    conn = sqlite3.connect(str(db_path))
    ensure_events_table(conn)

    saved = 0
    for ev, norm in zip(filtered_events, normalized):
        severity = norm.get("severity", "low")
        title_cn = norm.get("title_cn", ev["title"])
        if severity == "low":
            log.info(f"  [skip] LLM classified as LOW — {title_cn[:50]}")
            continue

        symbols = norm.get("affected_symbols", [])
        symbols_str = ",".join(symbols) if symbols else None
        claim = norm.get("one_line_claim", "")
        body = claim if claim else title_cn

        event_row = {
            "id": str(uuid.uuid4()),
            "source": ev["source"],
            "event_type": ev["event_type"],
            "title": title_cn,  # 使用中文标题
            "body": body,
            "symbols": symbols_str,
            "severity": severity,
            "stance": norm.get("stance", "neutral"),
            "created_at": ev["created_at"],
        }

        ok, reason = save_event(conn, event_row)
        if ok:
            saved += 1
            log.info(f"  [saved] {title_cn[:50]}")
        elif "duplicate" in reason:
            log.info(f"  [dedup] {title_cn[:50]} — already exists")
        else:
            log.warning(f"  [error] {ev['title'][:50]} — {reason}")

    conn.close()

    log.info("=" * 60)
    log.info(f"扫描完成: {fetched} fetched, {filtered} filtered, {len(normalized)} normalized, {saved} saved")
    if errors:
        log.info(f"  errors: {errors}")

    return {
        "fetched": fetched,
        "filtered": filtered,
        "normalized": len(normalized),
        "saved": saved,
        "errors": errors,
    }

# ── Tushare fetch helpers ──

def _fetch_tushare_news(tushare: TushareClient, src: str, start: str, end: str) -> List[Dict]:
    items = tushare.major_news(src, start, end)
    result = []
    for item in items:
        result.append({
            "source": f"tushare_major_news:{src}",
            "event_type": "news",
            "title": item.get("title", ""),
            "body": item.get("content", ""),
            "url": None,
            "created_at": item.get("datetime", "") or datetime.now().isoformat(),
        })
    return result

def _fetch_tushare_anns(tushare: TushareClient, symbol: str, start: str, end: str) -> List[Dict]:
    items = tushare.anns_d(symbol, start, end)
    result = []
    for item in items:
        result.append({
            "source": f"tushare_anns:{symbol}",
            "event_type": "announcement",
            "title": item.get("title", ""),
            "body": item.get("content", item.get("title", "")),
            "url": None,
            "created_at": item.get("ann_date", "") or datetime.now().isoformat(),
        })
    return result

# ── CLI ──

def main():
    if sys.platform == "win32":
        try:
            sys.stdout.reconfigure(encoding="utf-8")
            sys.stderr.reconfigure(encoding="utf-8")
        except Exception:
            pass

    parser = argparse.ArgumentParser(description="Event Watch 扫描调试脚本")
    parser.add_argument("--dry-run", action="store_true", help="只打印不入库")
    parser.add_argument("--no-llm", action="store_true", help="跳过 LLM，只看关键词过滤")
    parser.add_argument("--debug", action="store_true", help="显示 DEBUG 级别日志（含被过滤的事件）")
    parser.add_argument("--sources", action="store_true", help="列出所有数据源")
    args = parser.parse_args()

    if args.sources:
        print("数据源列表:")
        print("  Tushare major_news (sina, cls) — 需要 2000+ 积分")
        print("  Tushare announcements (per-holding)")
        print(f"  RSS feeds ({len(RSS_FEEDS)} 个):")
        for f in RSS_FEEDS:
            print(f"    - {f['name']}: {f['url'][:60]}")
        print("  Yahoo Finance search (全球宏观)")
        print("  DuckDuckGo news (需安装 ddgs)")
        return

    level = logging.DEBUG if args.debug else logging.INFO
    logging.basicConfig(
        level=level,
        format="%(asctime)s [%(levelname)s] %(message)s",
        datefmt="%H:%M:%S",
    )

    result = run_scan(dry_run=args.dry_run, no_llm=args.no_llm)
    print(json.dumps(result, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
