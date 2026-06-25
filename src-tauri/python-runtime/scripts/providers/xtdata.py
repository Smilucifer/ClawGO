# -*- coding: utf-8 -*-
"""miniQMT (xtquant.xtdata) 行情 provider。

软依赖：xtquant 未安装或 QMT 客户端未运行时，health() 返回 available=false，
其余函数抛异常由 RPC 层捕获，上层编排器据此降级到 tushare。
仅行情类（第一期）：kline / realtime_quote / health。
"""

_xtdata = None
_import_error = None


def _get_xtdata():
    """Lazy import xtquant.xtdata；失败记录原因。"""
    global _xtdata, _import_error
    if _xtdata is not None:
        return _xtdata
    if _import_error is not None:
        raise RuntimeError(_import_error)
    try:
        from xtquant import xtdata as xt
        _xtdata = xt
        return xt
    except Exception as e:  # noqa: BLE001
        _import_error = f"xtquant import failed: {e}"
        raise RuntimeError(_import_error)


def health() -> dict:
    """探测 miniQMT 是否可用。不抛异常。"""
    try:
        xt = _get_xtdata()
        # get_market_data_ex 对空列表应快速返回而不报错，作为客户端连通性探针。
        xt.get_sector_list()
        return {"available": True, "reason": ""}
    except Exception as e:  # noqa: BLE001
        return {"available": False, "reason": str(e)}


def kline(symbol: str = "", period: str = "1d", count: int = 25) -> dict:
    """获取历史 K线，字段名归一化为 tushare 风格。"""
    xt = _get_xtdata()
    fields = ["time", "open", "high", "low", "close", "volume"]
    # 触发本地下载后读取缓存。
    xt.download_history_data(symbol, period, "", "")
    data = xt.get_market_data_ex(fields, [symbol], period=period, count=count)
    df = data.get(symbol)
    items = []
    if df is not None:
        for idx in range(len(df)):
            row = df.iloc[idx]
            # time 为毫秒时间戳或 yyyymmdd，统一转 yyyymmdd 字符串。
            t = row["time"]
            trade_date = _to_yyyymmdd(t)
            items.append({
                "trade_date": trade_date,
                "open": float(row["open"]),
                "high": float(row["high"]),
                "low": float(row["low"]),
                "close": float(row["close"]),
                "vol": float(row["volume"]),
            })
    return {"items": items, "source": "miniqmt"}


def realtime_quote(symbols=None) -> dict:
    """获取实时快照。symbols 为代码列表。"""
    xt = _get_xtdata()
    if symbols is None:
        symbols = []
    if isinstance(symbols, str):
        symbols = [symbols]
    ticks = xt.get_full_tick(symbols)
    out = {}
    for code, t in (ticks or {}).items():
        out[code] = {
            "last": float(t.get("lastPrice", 0.0)),
            "volume": float(t.get("volume", 0.0)),
            "amount": float(t.get("amount", 0.0)),
        }
    return out


def _to_yyyymmdd(t) -> str:
    """xtdata time 字段（ms 时间戳 / yyyymmdd / yyyymmddHHMMSS）→ yyyymmdd。"""
    s = str(int(t)) if not isinstance(t, str) else t
    if len(s) >= 8 and s[:8].isdigit() and s[:4].startswith(("19", "20")):
        return s[:8]
    # 毫秒时间戳
    try:
        import datetime
        dt = datetime.datetime.fromtimestamp(int(t) / 1000)
        return dt.strftime("%Y%m%d")
    except Exception:  # noqa: BLE001
        return s[:8]
