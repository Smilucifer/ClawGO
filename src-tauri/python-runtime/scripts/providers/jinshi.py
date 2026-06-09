"""
金十数据 (Jin10) news provider.
Methods are called via JSON-RPC: "jinshi.news"
Uses the Jin10 flash news API — no auth required, no extra dependencies.
"""

import re

from .utils import create_session, matches_query, parse_timestamp

# Lazy-init session (requests may not be installed).
_session = None


def _get_session():
    global _session
    if _session is None:
        _session = create_session(
            referer="https://www.jin10.com/",
            origin="https://www.jin10.com",
        )
    return _session


def news(query: str = "", count: int = 15) -> list:
    """Fetch flash news from Jin10 (金十数据).

    Returns list of dicts matching YahooNewsItem schema:
    [{uuid, title, publisher, link, provider_publish_time, related_tickers}]
    """
    url = "https://flash-api.jin10.com/get_flash_list"
    params = {
        "channel": "-8200",  # 全部频道
        "max_time": "",
        "vip": 1,
    }
    headers = {
        "x-app-id": "bVBF4FyRTn5NJF5n",
        "x-version": "1.0.0",
    }

    session = _get_session()
    if session is None:
        return []

    try:
        resp = session.get(url, params=params, headers=headers, timeout=15)
        resp.raise_for_status()
        data = resp.json()
    except Exception:
        return []

    items = []
    for item in data.get("data", []):
        raw_content = item.get("data", {})
        if isinstance(raw_content, dict):
            title = re.sub(r'<[^>]+>', '', raw_content.get("content", "")).strip()
        elif isinstance(raw_content, str):
            title = re.sub(r'<[^>]+>', '', raw_content).strip()
        else:
            continue

        if not title:
            continue

        if query and not matches_query(title, query):
            continue

        flash_id = item.get("id", "")
        items.append({
            "uuid": f"jin10_{flash_id}" if flash_id else str(hash(title)),
            "title": title,
            "publisher": "金十数据",
            "link": f"https://www.jin10.com/flash_newest.html#id={flash_id}" if flash_id else "",
            "provider_publish_time": parse_timestamp(item.get("time", "")),
            "related_tickers": [],
        })

    return items[:count]
