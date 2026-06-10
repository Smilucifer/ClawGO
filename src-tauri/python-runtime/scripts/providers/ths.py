"""
同花顺 (THS/10jqka) news provider.
Methods are called via JSON-RPC: "ths.news"
Uses the THS public news API — no auth required, no extra dependencies.
"""

import requests
import time
from datetime import datetime


_session = requests.Session()
_session.headers.update({
    "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
    "Referer": "https://news.10jqka.com.cn/",
})


def news(query: str = "", count: int = 15) -> list:
    """Fetch stock news from THS (同花顺).

    Returns list of dicts matching NewsItem schema:
    [{uuid, title, publisher, link, provider_publish_time, related_tickers}]

    Uses THS's public news API (no auth required).
    """
    url = "https://news.10jqka.com.cn/tapp/news/push/stock/"
    params = {
        "page": 1,
        "tag": "",
        "track": "website",
        "pagesize": min(count, 50),
    }

    try:
        resp = _session.get(url, params=params, timeout=15)
        resp.raise_for_status()
        data = resp.json()
    except Exception as e:
        return []

    items = []
    for item in data.get("data", {}).get("list", []):
        title = item.get("title", "").strip()
        if not title:
            continue

        # Keyword filter if query provided
        if query and not _matches_query(title, query):
            continue

        # Parse time (ctime is Unix timestamp in seconds)
        ctime = item.get("ctime", 0)
        try:
            pub_time = int(ctime) if ctime else int(datetime.now().timestamp())
        except (ValueError, TypeError):
            pub_time = int(datetime.now().timestamp())

        # Build link
        link = item.get("url", "") or item.get("appUrl", "")

        items.append({
            "uuid": f"ths_{item.get('id', '')}",
            "title": title,
            "publisher": item.get("source", "") or "同花顺",
            "link": link,
            "provider_publish_time": pub_time,
            "related_tickers": [],
        })

    return items[:count]


def _matches_query(title: str, query: str) -> bool:
    """Check if title matches any keyword in the query string."""
    keywords = [kw.strip() for kw in query.replace(",", " ").split() if kw.strip()]
    if not keywords:
        return True
    title_lower = title.lower()
    return any(kw.lower() in title_lower for kw in keywords)
