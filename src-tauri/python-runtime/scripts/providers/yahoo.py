"""
Yahoo Finance provider using direct HTTP requests to Yahoo Chart API.
Methods are called via JSON-RPC: "yahoo.quote", "yahoo.history", "yahoo.news"

Uses the public chart endpoint directly instead of yfinance to avoid 429 rate limits
that the yfinance library triggers via its multi-request info fetching pattern.
"""

import requests
from datetime import datetime, timedelta

_HEADERS = {
    "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 "
                  "(KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
}

_CHART_URL = "https://query1.finance.yahoo.com/v8/finance/chart/{symbol}"


def _fetch_chart(symbol: str, params: dict = None) -> dict:
    """Fetch Yahoo Finance chart data for a symbol via direct HTTP."""
    url = _CHART_URL.format(symbol=symbol)
    resp = requests.get(url, headers=_HEADERS, params=params or {}, timeout=15)
    resp.raise_for_status()
    data = resp.json()
    result = data.get("chart", {}).get("result")
    if not result:
        raise ValueError(f"Yahoo chart: no data for {symbol}")
    return result[0]


def quote(symbol: str) -> dict:
    """Fetch real-time quote for a symbol.

    Returns dict matching Rust YahooQuote struct:
    {symbol, name, price, change, change_pct, previous_close, timestamp}
    """
    data = _fetch_chart(symbol)
    meta = data.get("meta", {})

    price = meta.get("regularMarketPrice", 0.0)
    previous_close = meta.get("chartPreviousClose", 0.0)
    change = price - previous_close if previous_close else 0.0
    change_pct = (change / previous_close * 100) if previous_close else 0.0

    return {
        "symbol": symbol,
        "name": meta.get("shortName") or meta.get("symbol") or symbol,
        "price": round(price, 3),
        "change": round(change, 3),
        "change_pct": round(change_pct, 1),
        "previous_close": round(previous_close, 3),
        "timestamp": int(meta.get("regularMarketTime", datetime.now().timestamp())),
    }


def history(symbol: str, days: int = 30) -> list:
    """Fetch historical daily bars for a symbol.

    Returns list of dicts matching Rust YahooBar struct:
    [{symbol, date, open, high, low, close, volume}]
    """
    from datetime import datetime, timedelta

    end_ts = int(datetime.now().timestamp())
    start_ts = int((datetime.now() - timedelta(days=days + 10)).timestamp())

    data = _fetch_chart(symbol, params={
        "period1": start_ts,
        "period2": end_ts,
        "interval": "1d",
    })

    timestamps = data.get("timestamp", [])
    indicators = data.get("indicators", {}).get("quote", [{}])[0]

    opens = indicators.get("open", [])
    highs = indicators.get("high", [])
    lows = indicators.get("low", [])
    closes = indicators.get("close", [])
    volumes = indicators.get("volume", [])

    bars = []
    for i, ts in enumerate(timestamps):
        o = opens[i] if i < len(opens) else None
        h = highs[i] if i < len(highs) else None
        l = lows[i] if i < len(lows) else None
        c = closes[i] if i < len(closes) else None
        v = volumes[i] if i < len(volumes) else None
        if o is None or h is None or l is None or c is None:
            continue
        bars.append({
            "symbol": symbol,
            "date": datetime.utcfromtimestamp(ts).strftime("%Y-%m-%d"),
            "open": round(float(o), 4),
            "high": round(float(h), 4),
            "low": round(float(l), 4),
            "close": round(float(c), 4),
            "volume": int(v) if v else 0,
        })

    # Sort chronologically (oldest first)
    bars.sort(key=lambda b: b["date"])
    return bars


def news(query: str, count: int = 10) -> list:
    """Search Yahoo Finance news.

    Returns list of dicts matching Rust NewsItem struct:
    [{uuid, title, publisher, link, provider_publish_time, related_tickers}]
    """
    # yfinance doesn't have a direct news search API,
    # use the search endpoint via requests
    import requests

    url = "https://query1.finance.yahoo.com/v1/finance/search"
    params = {
        "q": query,
        "quotesCount": 0,
        "newsCount": count,
        "enableFuzzyQuery": False,
        "quotesQueryId": "tss_match_phrase_query",
    }
    headers = {
        "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
    }

    resp = requests.get(url, params=params, headers=headers, timeout=15)
    resp.raise_for_status()
    data = resp.json()

    items = []
    for item in data.get("news", []):
        items.append({
            "uuid": item.get("uuid", ""),
            "title": item.get("title", ""),
            "publisher": item.get("publisher", ""),
            "link": item.get("link", ""),
            "provider_publish_time": item.get("providerPublishTime", ""),
            "related_tickers": item.get("relatedTickers", []),
        })

    return items
