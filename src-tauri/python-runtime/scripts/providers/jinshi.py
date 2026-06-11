"""
金十数据 (Jin10) news provider.
Methods are called via JSON-RPC: "jinshi.news"
Uses the Jin10 flash news API — no auth required, no extra dependencies.
"""

import re

from .utils import LazySession, matches_query, parse_timestamp

_session = LazySession("jinshi", referer="https://www.jin10.com/", origin="https://www.jin10.com")

# Channel IDs from Jin10 API
# [1]=速报, [2]=A股, [3]=商品, [4]=债券, [5]=国际
CHANNEL_ALL = None  # No filter
CHANNEL_A_SHARE = 2  # A股频道


def _clean_html(html: str) -> str:
    """Clean HTML content to plain text."""
    if not html:
        return ''
    text = re.sub(r'<br\s*/?>', '\n', html)
    text = re.sub(r'<[^>]+>', '', text)
    text = text.replace('&nbsp;', ' ').replace('&amp;', '&')
    text = text.replace('&lt;', '<').replace('&gt;', '>').replace('&quot;', '"')
    return text.strip()


def _should_skip(item: dict) -> str | None:
    """Filter rules for Jin10 flash news. Returns skip reason or None to keep."""
    data = item.get("data", {})

    # 1. Ads
    extras = item.get("extras") or {}
    if extras.get("ad") is True:
        return "ad"

    # 2. HTML list/collection (section-news)
    raw_content = data.get("content", "") if isinstance(data, dict) else ""
    if "section-news" in raw_content:
        return "html-list"

    # 3. Empty or too short
    content = _clean_html(raw_content)
    if not content or len(content) < 5:
        return "empty"

    # 4. Click bait ("点击查看…")
    if re.match(r'^.{0,30}点击查看[…\.]{1,3}$', content):
        return "click-bait"
    if len(content) < 30 and "点击查看" in content:
        return "click-bait"

    # 5. Summary digest (>1000 chars with numbered list)
    if len(content) > 1000 and re.match(r'^[①②③④⑤\d]+[.、)）]', content):
        return "summary-digest"
    if len(content) > 1000 and re.search(r'\n[①②③]', content):
        return "summary-digest"

    return None  # Keep


def news(query: str = "", count: int = 15, channel: int | None = None) -> list:
    """Fetch flash news from Jin10 (金十数据).

    Args:
        query: Optional keyword filter
        count: Max items to return
        channel: Channel filter (None=all, 2=A-share, 3=commodity, 4=bond, 5=international)

    Returns list of dicts matching NewsItem schema:
    [{uuid, title, publisher, link, provider_publish_time, related_tickers, channels}]
    """
    url = "https://flash-api.jin10.com/get_flash_list"
    params = {
        "channel": "-8200",  # All channels (we filter client-side)
        "max_time": "",
        "vip": 1,
    }
    headers = {
        "x-app-id": "bVBF4FyRTn5NJF5n",
        "x-version": "1.0.0",
    }

    session = _session.get()
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
        # Apply filter rules
        skip_reason = _should_skip(item)
        if skip_reason:
            continue

        raw_content = item.get("data", {})
        if isinstance(raw_content, dict):
            title = _clean_html(raw_content.get("content", ""))
        elif isinstance(raw_content, str):
            title = _clean_html(raw_content)
        else:
            continue

        if not title:
            continue

        # Channel filter (client-side)
        item_channels = item.get("channel", [])
        if channel is not None and channel not in item_channels:
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
            "channels": item_channels,  # Include channel info
        })

    return items[:count]


def news_a_share(query: str = "", count: int = 15) -> list:
    """Fetch A-share related flash news from Jin10 (金十数据).

    Convenience wrapper that filters for A-share channel (channel=2).
    """
    return news(query=query, count=count, channel=CHANNEL_A_SHARE)
