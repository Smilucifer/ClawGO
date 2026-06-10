"""
AkShare per-stock news provider.
Methods are called via JSON-RPC: "akshare.stock_news"
Uses AkShare library (stock_news_em) for EastMoney per-stock news.
"""

import re
from datetime import datetime

from .utils import clean_dataframe, parse_timestamp


def stock_news(symbol: str = "", count: int = 10) -> list:
    """Fetch per-stock news from EastMoney via AkShare.

    Returns list of dicts matching NewsItem schema:
    [{uuid, title, publisher, link, provider_publish_time, related_tickers}]

    Uses AkShare stock_news_em which wraps EastMoney's search API.
    """
    if not symbol:
        return []

    try:
        import akshare as ak
        df = ak.stock_news_em(symbol=symbol)
    except Exception:
        return []

    # Clean NaN and literal "-" (EastMoney empty-cell sentinel).
    df = clean_dataframe(df)
    items = []
    for row in df.to_dict("records"):
        title = str(row.get("新闻标题", "")).strip()
        if not title:
            continue

        link = str(row.get("新闻链接", "")).strip()
        items.append({
            "uuid": link if link else str(hash(title)),
            "title": title,
            "publisher": str(row.get("文章来源", "")).strip() or "东方财富",
            "link": link,
            "provider_publish_time": parse_timestamp(str(row.get("发布时间", "")).strip()),
            "related_tickers": [symbol],
        })

    return items[:count]
