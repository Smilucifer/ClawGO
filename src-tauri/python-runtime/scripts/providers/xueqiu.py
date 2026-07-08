"""雪球舆情 provider（RPC: xueqiu.hot）。

scrapling StealthyFetcher 过阿里云 WAF + 注入登录 cookie。
只抓市场级热帖榜（不逐标的）。

失败降级契约：
- scrapling 未安装 → 返回空列表
- cookie 未提供 / 已失效 → 返回空列表
- 引擎抓取异常（超时 / WAF 拦截 / 网络）→ 返回空列表
- JSON 解析失败 → 返回空列表

调用方（Rust `fetch_xueqiu_market`）看到空列表即判定降级，不抛错。
"""

import sys
import re
import json
from datetime import datetime


def _parse_cookie(cookie_json: str):
    """cookie_json: [{"name":..,"value":..}] 或 "k=v; k2=v2" 字符串。"""
    if not cookie_json:
        return []
    try:
        data = json.loads(cookie_json)
        if isinstance(data, list):
            out = []
            for c in data:
                if "name" in c and "value" in c:
                    out.append({
                        "name": c["name"],
                        "value": c["value"],
                        "domain": ".xueqiu.com",
                        "path": "/",
                    })
            return out
    except Exception:
        pass
    # 退化：k=v; 串
    out = []
    for part in cookie_json.split(";"):
        if "=" in part:
            k, v = part.strip().split("=", 1)
            out.append({
                "name": k.strip(),
                "value": v.strip(),
                "domain": ".xueqiu.com",
                "path": "/",
            })
    return out


def _strip_html(s):
    return re.sub(r"<[^>]+>", "", s or "").strip()


def _ts_iso(ms):
    try:
        return datetime.fromtimestamp(int(ms) // 1000).isoformat(timespec="seconds")
    except Exception:
        return datetime.now().isoformat(timespec="seconds")


def hot(cookie_json="", size=15) -> list:
    """雪球市场级热帖榜。cookie 失效 / 引擎缺失 / 抓取失败均返回空列表（不抛）。"""
    try:
        from scrapling.fetchers import StealthyFetcher
    except ImportError:
        print("[xueqiu] scrapling not installed", file=sys.stderr, flush=True)
        return []

    cookies = _parse_cookie(cookie_json)
    size = max(1, min(int(size or 15), 30))
    api = (
        "https://xueqiu.com/statuses/hot/listV2.json"
        f"?since_id=-1&max_id=-1&size={size}"
    )
    captured = {}

    def action(page):
        try:
            if cookies:
                page.context.add_cookies(cookies)
            page.goto("https://xueqiu.com/", wait_until="domcontentloaded")
            captured["text"] = page.evaluate(
                """async (url) => {
                    const r = await fetch(url, {credentials:'include'});
                    return await r.text();
                }""",
                api,
            )
        except Exception as e:
            print(f"[xueqiu] page_action error: {e}", file=sys.stderr, flush=True)
        return page

    try:
        StealthyFetcher.fetch(
            "https://xueqiu.com/",
            headless=True,
            network_idle=True,
            timeout=60000,
            page_action=action,
        )
    except Exception as e:
        print(f"[xueqiu] fetch failed: {e}", file=sys.stderr, flush=True)
        return []

    text = captured.get("text", "")
    if not text:
        print("[xueqiu] empty response (WAF/cookie may have failed)",
              file=sys.stderr, flush=True)
        return []

    try:
        data = json.loads(text)
    except Exception as e:
        preview = text[:100].replace("\n", " ")
        print(f"[xueqiu] parse failed: {e} raw={preview!r}",
              file=sys.stderr, flush=True)
        return []

    items = []
    for it in data.get("items", []):
        od = it.get("original_status") or it
        title = _strip_html(od.get("title") or od.get("text") or "")[:120]
        if not title:
            continue
        items.append({
            "provider": "xueqiu",
            "symbol": None,
            "title": title,
            "summary": "",
            "url": "https://xueqiu.com" + (od.get("target") or ""),
            "published_at": _ts_iso(od.get("created_at") or 0),
            "read_count": od.get("view_count"),
            "comment_count": od.get("reply_count"),
            "source_type": "post",
            "sentiment_hint": 0.0,
        })
    print(f"[xueqiu.hot] {len(items)} items", file=sys.stderr, flush=True)
    return items
