"""
Yahoo Finance provider using yfinance.
Methods are called via JSON-RPC: "yahoo.quote", "yahoo.history", "yahoo.news"
"""

import yfinance as yf
from datetime import datetime, timedelta


def quote(symbol: str) -> dict:
    """Fetch real-time quote for a symbol.

    Returns dict matching Rust YahooQuote struct:
    {symbol, name, price, change, change_pct, previous_close, timestamp}
    """
    ticker = yf.Ticker(symbol)
    info = ticker.info

    price = info.get("regularMarketPrice") or info.get("currentPrice") or 0.0
    previous_close = info.get("regularMarketPreviousClose") or info.get("previousClose") or 0.0
    change = price - previous_close if previous_close else 0.0
    change_pct = (change / previous_close * 100) if previous_close else 0.0

    return {
        "symbol": symbol,
        "name": info.get("shortName") or info.get("longName") or symbol,
        "price": round(price, 3),
        "change": round(change, 3),
        "change_pct": round(change_pct, 1),
        "previous_close": round(previous_close, 3),
        "timestamp": int(info.get("regularMarketTime", datetime.now().timestamp())),
    }


def history(symbol: str, days: int = 30) -> list:
    """Fetch historical daily bars for a symbol.

    Returns list of dicts matching Rust YahooBar struct:
    [{symbol, date, open, high, low, close, volume}]
    """
    ticker = yf.Ticker(symbol)

    # Map days to yfinance period
    if days <= 5:
        period = "5d"
    elif days <= 30:
        period = "1mo"
    elif days <= 90:
        period = "3mo"
    elif days <= 180:
        period = "6mo"
    else:
        period = "1y"

    df = ticker.history(period=period, interval="1d")

    bars = []
    for date, row in df.iterrows():
        bars.append({
            "symbol": symbol,
            "date": date.strftime("%Y-%m-%d"),
            "open": round(float(row["Open"]), 4),
            "high": round(float(row["High"]), 4),
            "low": round(float(row["Low"]), 4),
            "close": round(float(row["Close"]), 4),
            "volume": int(row["Volume"]),
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
