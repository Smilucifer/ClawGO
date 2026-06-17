"""
AkShare market statistics provider.
Methods are called via JSON-RPC: "akshare_market.bond_yield_10y",
"akshare_market.market_stats".

Uses AkShare library for:
- 中国10年期国债收益率 (bond_zh_us_rate)
- A股涨停/跌停家数 (stock_zt_pool_em / stock_zt_pool_dtgc_em)
"""

from datetime import datetime, timedelta

from .utils import clean_dataframe


def bond_yield_10y() -> dict:
    """Fetch latest China 10Y government bond yield.

    Returns: {"yield_10y": float, "date": "YYYY-MM-DD"} or {} on failure.
    """
    try:
        import akshare as ak
    except ImportError:
        return {}

    # Only fetch recent data (90 days) to avoid downloading all history
    start = (datetime.now() - timedelta(days=90)).strftime("%Y%m%d")
    try:
        df = ak.bond_zh_us_rate(start_date=start)
    except Exception:
        return {}

    if df is None or df.empty:
        return {}

    # Find the 10Y column — column name contains "10" and "中国" (robust match)
    col_10y = None
    for col in df.columns:
        col_str = str(col)
        if "10" in col_str and "中国" in col_str and "2" not in col_str.split("10")[-1][:1]:
            col_10y = col
            break
    # Fallback: try index 3 (historically "中国国债收益率10年")
    if col_10y is None and len(df.columns) > 3:
        col_10y = df.columns[3]

    if col_10y is None:
        return {}

    # Get the latest non-null row
    df_clean = df.dropna(subset=[col_10y])
    if df_clean.empty:
        return {}

    latest = df_clean.iloc[-1]
    date_val = str(latest.iloc[0])[:10]
    yield_val = float(latest[col_10y])

    return {"yield_10y": yield_val, "date": date_val}


def market_stats(date: str = "") -> dict:
    """Fetch limit-up and limit-down stock counts for a given date.

    Args:
        date: Trading date in "YYYYMMDD" format. Empty string = today.

    Returns: {"limit_up_count": int, "limit_down_count": int, "date": str}
    or {} if both API calls fail (e.g. non-trading day).
    """
    try:
        import akshare as ak
    except ImportError:
        return {}

    if not date:
        date = datetime.now().strftime("%Y%m%d")

    result = {"limit_up_count": 0, "limit_down_count": 0, "date": date}
    any_success = False

    # 涨停家数
    try:
        df_zt = ak.stock_zt_pool_em(date=date)
        result["limit_up_count"] = len(df_zt) if df_zt is not None else 0
        any_success = True
    except Exception:
        pass  # Non-trading day or API error

    # 跌停家数
    try:
        df_dt = ak.stock_zt_pool_dtgc_em(date=date)
        result["limit_down_count"] = len(df_dt) if df_dt is not None else 0
        any_success = True
    except Exception:
        pass

    return result if any_success else {}
