"""五源舆情抓取 provider（RPC: sentiment.fetch）。

前四源（同花顺 / 新浪 / 财联社 / 东方财富）走纯 requests；雪球留 Plan B。

- ths        同花顺  — push API，公开无签名
- sina       新浪    — 全球直播流，需 trust_env=False 关系统代理
- cailianshe 财联社  — m 站 API，无签名
- eastmoney  东财    — 股吧列表页内嵌 JSON（个股舆情）

统一契约（每条 dict）：
    provider, symbol, title, summary, url, published_at,
    read_count, comment_count, source_type, sentiment_hint
"""

import sys
import re
import json
from datetime import datetime

from .utils import LazySession, parse_timestamp

# ---------------------------------------------------------------------------
# 粗情绪词典（快速可复现；精细语境判断留给 AI 点评）
# ---------------------------------------------------------------------------

_POS_WORDS = [
    "利好", "涨停", "突破", "增持", "回购", "中标", "订单", "扩产", "涨价",
    "超预期", "创新高", "放量", "主力流入", "北向流入", "看多", "重组", "并购",
]
_NEG_WORDS = [
    "利空", "跌停", "破位", "减持", "亏损", "退市", "问询", "处罚", "爆雷",
    "不及预期", "创新低", "商誉", "解禁", "看空", "违规", "立案", "下调",
]


def sentiment_hint(text: str) -> float:
    """基于关键词词典的粗情绪分 -1.0 ~ 1.0。命中越多越极端，无命中为 0。"""
    if not text:
        return 0.0
    pos = sum(1 for w in _POS_WORDS if w in text)
    neg = sum(1 for w in _NEG_WORDS if w in text)
    if pos == 0 and neg == 0:
        return 0.0
    return round((pos - neg) / (pos + neg), 2)


def _item(provider, symbol, title, summary, url, published_at,
          read_count=None, comment_count=None, source_type="news"):
    """归一化到统一 SentimentItem 契约。"""
    text = f"{title} {summary or ''}"
    return {
        "provider": provider,
        "symbol": symbol,
        "title": (title or "").strip(),
        "summary": (summary or "").strip(),
        "url": url or "",
        "published_at": published_at,
        "read_count": read_count,
        "comment_count": comment_count,
        "source_type": source_type,
        "sentiment_hint": sentiment_hint(text),
    }


def _ts_iso(ts_sec) -> str:
    try:
        return datetime.fromtimestamp(int(ts_sec)).isoformat(timespec="seconds")
    except Exception:
        return datetime.now().isoformat(timespec="seconds")


def _strip_html(s: str) -> str:
    return re.sub(r"<[^>]+>", "", s or "").strip()


# ---------------------------------------------------------------------------
# 同花顺（THS）— push API，公开无签名
# ---------------------------------------------------------------------------

_ths_session = LazySession("ths_premarket", referer="https://news.10jqka.com.cn/")


def fetch_ths(symbol=None, limit=20) -> list:
    """同花顺快讯流。symbol 暂不区分（push 接口为全市场快讯）。"""
    session = _ths_session.get()
    if session is None:
        return []
    url = "https://news.10jqka.com.cn/tapp/news/push/stock/"
    params = {"page": 1, "tag": "", "track": "website", "pagesize": min(limit, 50)}
    try:
        resp = session.get(url, params=params, timeout=15)
        resp.raise_for_status()
        data = resp.json()
    except Exception as e:
        print(f"[ths] failed: {e}", file=sys.stderr, flush=True)
        return []
    items = []
    lst = (data.get("data") or {}).get("list", [])
    for it in lst:
        title = (it.get("title") or "").strip()
        if not title:
            continue
        items.append(_item(
            "ths", symbol, title, _strip_html(it.get("digest", "")),
            it.get("url", ""), _ts_iso(it.get("ctime", "")),
            source_type="news",
        ))
    return items[:limit]


# ---------------------------------------------------------------------------
# 新浪（Sina）— 需 trust_env=False 关系统代理
# ---------------------------------------------------------------------------

def fetch_sina(symbol=None, limit=20) -> list:
    """新浪财经全球直播流。"""
    try:
        import requests
    except ImportError:
        return []
    session = requests.Session()
    session.trust_env = False  # 关键：关系统代理，否则 ProxyError
    session.headers.update({
        "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
        "Referer": "https://finance.sina.com.cn/",
    })
    url = "https://zhibo.sina.com.cn/api/zhibo/feed"
    params = {
        "page": 1, "page_size": min(limit, 50), "zhibo_id": 152,
        "tag_id": 0, "dire": "f", "dpc": 1,
    }
    try:
        resp = session.get(url, params=params, timeout=15)
        resp.raise_for_status()
        data = resp.json()
    except Exception as e:
        print(f"[sina] failed: {e}", file=sys.stderr, flush=True)
        return []
    items = []
    feed = (((data.get("result") or {}).get("data") or {}).get("feed") or {})
    for it in feed.get("list", []):
        rich = it.get("rich_text") or it.get("content") or ""
        title = _strip_html(rich)[:120]
        if not title:
            continue
        items.append(_item(
            "sina", symbol, title, "",
            it.get("docurl", ""), _ts_iso(parse_timestamp(it.get("create_time", ""))),
            source_type="news",
        ))
    return items[:limit]


# ---------------------------------------------------------------------------
# 财联社（Cailianshe）— m 站 API，无签名
# ---------------------------------------------------------------------------

_cls_session = LazySession("cls_premarket", referer="https://m.cls.cn/")


def fetch_cailianshe(symbol=None, limit=20) -> list:
    """财联社电报流（m 站）。"""
    session = _cls_session.get()
    if session is None:
        return []
    url = "https://m.cls.cn/nodeapi/telegraphs"
    params = {"app": "CailianpressWap", "os": "web", "sv": "1.0", "rn": min(limit, 50)}
    try:
        resp = session.get(url, params=params, timeout=15)
        resp.raise_for_status()
        data = resp.json()
    except Exception as e:
        print(f"[cailianshe] failed: {e}", file=sys.stderr, flush=True)
        return []
    items = []
    roll = (data.get("data") or {}).get("roll_data", []) or data.get("data", [])
    if isinstance(roll, dict):
        roll = roll.get("roll_data", [])
    for it in roll:
        title = (it.get("title") or it.get("content") or "").strip()
        title = _strip_html(title)[:120]
        if not title:
            continue
        items.append(_item(
            "cailianshe", symbol, title, "",
            it.get("shareurl", ""), _ts_iso(it.get("ctime", "")),
            read_count=it.get("reading_num"),
            comment_count=it.get("comment_num"),
            source_type="news",
        ))
    return items[:limit]


# ---------------------------------------------------------------------------
# 东方财富（EastMoney）— 股吧列表页内嵌 var article_list JSON（个股舆情）
# ---------------------------------------------------------------------------

_em_session = LazySession("em_premarket", referer="https://guba.eastmoney.com/")


def fetch_eastmoney(symbol=None, limit=20) -> list:
    """东财股吧帖子（个股舆情）。无 symbol 时取热门吧（示例用东财吧）。"""
    session = _em_session.get()
    if session is None:
        return []
    code = "600519"
    if symbol:
        code = re.sub(r"[^0-9]", "", symbol) or "600519"
    url = f"https://guba.eastmoney.com/list,{code}_1.html"
    try:
        resp = session.get(url, timeout=15)
        resp.raise_for_status()
        html = resp.text
    except Exception as e:
        print(f"[eastmoney] failed: {e}", file=sys.stderr, flush=True)
        return []
    # 页面内嵌 var article_list = {...};
    m = re.search(r"var\s+article_list\s*=\s*(\{.*?\});", html, re.DOTALL)
    if not m:
        print("[eastmoney] article_list not found in page", file=sys.stderr, flush=True)
        return []
    try:
        data = json.loads(m.group(1))
    except Exception as e:
        print(f"[eastmoney] json parse failed: {e}", file=sys.stderr, flush=True)
        return []
    items = []
    for it in (data.get("re") or []):
        title = (it.get("post_title") or "").strip()
        if not title:
            continue
        items.append(_item(
            "eastmoney", symbol, title, "",
            "https://guba.eastmoney.com" + (it.get("post_url") or ""),
            it.get("post_publish_time", datetime.now().isoformat()),
            read_count=it.get("post_click_count"),
            comment_count=it.get("post_comment_count"),
            source_type="post",
        ))
    return items[:limit]


# ---------------------------------------------------------------------------
# 统一入口
# ---------------------------------------------------------------------------

_PROVIDERS = {
    "ths": fetch_ths,
    "sina": fetch_sina,
    "cailianshe": fetch_cailianshe,
    "eastmoney": fetch_eastmoney,
}


def fetch(provider="all", symbol=None, limit=20) -> list:
    out = []
    targets = _PROVIDERS.keys() if provider == "all" else [provider]
    for name in targets:
        fn = _PROVIDERS.get(name)
        if fn is None:
            continue
        try:
            got = fn(symbol, limit)
            print(f"[sentiment.{name}] {len(got)} items", file=sys.stderr, flush=True)
            out += got
        except Exception as e:
            print(f"[sentiment.{name}] error: {e}", file=sys.stderr, flush=True)
    return out
