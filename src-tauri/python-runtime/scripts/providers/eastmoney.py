"""
EastMoney (东方财富) news provider.
Methods are called via JSON-RPC: "eastmoney.news", "eastmoney.quote"
Uses direct REST API calls — no extra dependencies beyond 'requests'.
"""

import time
from datetime import datetime

from .utils import create_session, matches_query, parse_timestamp

# Lazy-init session (requests may not be installed).
_session = None


def _get_session():
    global _session
    if _session is None:
        _session = create_session(referer="https://finance.eastmoney.com/")
    return _session


def _empty_quote(symbol: str) -> dict:
    """Return a zero-value quote dict for fallback when data is unavailable."""
    return {
        "symbol": symbol,
        "name": symbol,
        "price": 0.0,
        "change": 0.0,
        "change_pct": 0.0,
        "previous_close": 0.0,
        "timestamp": int(datetime.now().timestamp()),
    }


def news(query: str = "", count: int = 15) -> list:
    """Search EastMoney financial news.

    Returns list of dicts matching YahooNewsItem schema:
    [{uuid, title, publisher, link, provider_publish_time, related_tickers}]
    """
    url = "https://np-listapi.eastmoney.com/comm/web/getNewsByColumns"
    params = {
        "column": "467",  # A股资讯
        "pageSize": min(count, 50),
        "pageIndex": 0,
        "client": "web",
        "biz": "web_news_col",
        "sortEnd": "",
        "req_trace": str(int(time.time() * 1000)),
    }

    session = _get_session()
    if session is None:
        return _fetch_7x24_news(count)

    try:
        resp = session.get(url, params=params, timeout=15)
        resp.raise_for_status()
        data = resp.json()
    except Exception:
        # Fallback: try 7x24 fast news API
        return _fetch_7x24_news(count)

    items = []
    for item in (data.get("data") or {}).get("list", []):
        title = item.get("title", "").strip()
        if not title:
            continue

        if query and not matches_query(title, query):
            continue

        items.append({
            "uuid": item.get("code", item.get("uniqueUrl", str(hash(title)))),
            "title": title,
            "publisher": item.get("mediaName", "") or "东方财富",
            "link": item.get("url", ""),
            "provider_publish_time": parse_timestamp(item.get("showTime", "")),
            "related_tickers": [],
        })

    return items[:count]


def _fetch_7x24_news(count: int) -> list:
    """Fallback: fetch from EastMoney 7x24 live news feed."""
    url = "https://np-anotice-stock.eastmoney.com/api/security/ann"
    params = {
        "page_size": min(count, 50),
        "page_index": 1,
        "ann_type": "A",
        "client_source": "web",
        "f_node": "0",
        "s_node": "0",
    }

    session = _get_session()
    if session is None:
        return []

    try:
        resp = session.get(url, params=params, timeout=15)
        resp.raise_for_status()
        data = resp.json()
    except Exception:
        return []

    items = []
    for item in data.get("data", {}).get("list", []):
        title = item.get("title", "").strip()
        if not title:
            continue
        items.append({
            "uuid": item.get("art_code", str(hash(title))),
            "title": title,
            "publisher": "东方财富公告",
            "link": item.get("url", ""),
            "provider_publish_time": parse_timestamp(item.get("notice_date", "")),
            "related_tickers": [],
        })

    return items[:count]


def quote(symbol: str) -> dict:
    """Fetch real-time quote for an A-share symbol from EastMoney.

    Returns dict matching YahooQuote schema:
    {symbol, name, price, change, change_pct, previous_close, timestamp}
    """
    # Determine market code (1=SH, 0=SZ)
    secid = f"1.{symbol}" if symbol.startswith("6") else f"0.{symbol}"

    url = "https://push2.eastmoney.com/api/qt/stock/get"
    params = {
        "secid": secid,
        "fields": "f43,f44,f45,f46,f47,f48,f58,f60,f170",
        "ut": "fa5fd1943c7b386f172d6893dbbd1d0c",
    }

    session = _get_session()
    if session is None:
        return _empty_quote(symbol)

    try:
        resp = session.get(url, params=params, timeout=10)
        resp.raise_for_status()
        data = resp.json().get("data", {})
    except Exception:
        return _empty_quote(symbol)

    # Prices are in cents (分) for A-shares
    price = data.get("f43", 0) / 100.0
    prev_close = data.get("f60", 0) / 100.0
    change = price - prev_close if prev_close else 0.0
    change_pct = (change / prev_close * 100) if prev_close else 0.0

    return {
        "symbol": symbol,
        "name": data.get("f58", symbol),
        "price": round(price, 3),
        "change": round(change, 3),
        "change_pct": round(change_pct, 1),
        "previous_close": round(prev_close, 3),
        "timestamp": int(datetime.now().timestamp()),
    }
