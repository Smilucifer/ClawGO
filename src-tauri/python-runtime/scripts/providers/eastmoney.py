"""
EastMoney (东方财富) news provider.
Methods are called via JSON-RPC: "eastmoney.news", "eastmoney.quote"
Uses direct REST API calls — no extra dependencies beyond 'requests'.
"""

import time
from datetime import datetime

from .utils import LazySession, matches_query, parse_timestamp

_session = LazySession("eastmoney", referer="https://finance.eastmoney.com/")


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

    Returns list of dicts matching NewsItem schema:
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

    session = _session.get()
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

    session = _session.get()
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
    """Fetch real-time quote for an A-share symbol from EastMoney trends2 API.

    Uses /api/qt/stock/trends2/get which is NOT blocked by eastmoney anti-crawling WAF.
    Returns dict with quote fields:
    {symbol, name, price, change, change_pct, previous_close, timestamp}
    """
    # Determine market code (1=SH, 0=SZ)
    secid = f"1.{symbol}" if symbol.startswith("6") else f"0.{symbol}"

    url = "https://push2.eastmoney.com/api/qt/stock/trends2/get"
    # fields1 controls top-level metadata (name, decimal, preClose)
    # fields2 controls trend point fields (datetime, price, avg)
    params = {"secid": secid, "fields1": "f1,f2,f3,f4,f5,f6,f7,f8,f9,f10", "fields2": "f51,f52,f53"}

    session = _session.get()
    if session is None:
        return _empty_quote(symbol)

    try:
        resp = session.get(url, params=params, timeout=10)
        resp.raise_for_status()
        data = resp.json().get("data", {})
    except Exception:
        return _empty_quote(symbol)

    if not data or not data.get("trends"):
        return _empty_quote(symbol)

    # Latest price from last trend point: "datetime,price,avg_price"
    try:
        latest_str = data["trends"][-1]
        price = float(latest_str.split(",")[1])
    except (IndexError, ValueError):
        return _empty_quote(symbol)

    prev_close = data.get("preClose", 0)
    change = price - prev_close if prev_close else 0.0
    change_pct = (change / prev_close * 100) if prev_close else 0.0

    return {
        "symbol": symbol,
        "name": data.get("name", symbol),
        "price": round(price, 3),
        "change": round(change, 3),
        "change_pct": round(change_pct, 1),
        "previous_close": round(prev_close, 3),
        "timestamp": int(datetime.now().timestamp()),
    }


def overseas_indicator(secid: str) -> dict:
    """Fetch an overseas indicator (DXY / US10Y) from EastMoney trends2 API.

    secid examples: "100.UDI" (美元指数), "171.US10Y" (美国10年期国债收益率).
    Uses /api/qt/stock/trends2/get which is NOT blocked by eastmoney anti-crawling WAF
    (unlike /api/qt/stock/get which triggers Connection reset).

    Returns {"value": float, "name": str, "change_pct": float} or {} on failure.
    """
    import logging
    url = "https://push2.eastmoney.com/api/qt/stock/trends2/get"
    # fields1 controls top-level metadata (name, decimal, preClose, preSettlement)
    # fields2 controls trend point fields (datetime, price, avg)
    params = {"secid": secid, "fields1": "f1,f2,f3,f4,f5,f6,f7,f8,f9,f10", "fields2": "f51,f52,f53"}
    session = _session.get()
    if session is None:
        logging.warning("eastmoney.overseas_indicator(%s): session unavailable", secid)
        return {}
    try:
        resp = session.get(url, params=params, timeout=10)
        resp.raise_for_status()
        data = resp.json().get("data", {})
    except Exception as e:
        logging.warning("eastmoney.overseas_indicator(%s): request failed: %s", secid, e)
        return {}
    if not data or not data.get("trends"):
        logging.warning("eastmoney.overseas_indicator(%s): empty data or no trends", secid)
        return {}
    # Latest price from last trend point: "datetime,price,avg_price"
    try:
        latest_str = data["trends"][-1]
        price = float(latest_str.split(",")[1])
    except (IndexError, ValueError) as e:
        logging.warning("eastmoney.overseas_indicator(%s): trend parse failed: %s", secid, e)
        return {}
    decimal = data.get("decimal", 2)
    name = data.get("name", secid)
    # Previous close
    prev_close = data.get("preClose") or data.get("preSettlement")
    change_pct = round((price / prev_close - 1) * 100, 2) if prev_close and prev_close > 0 else 0.0
    return {
        "value": round(price, decimal),
        "name": name,
        "change_pct": change_pct,
    }
