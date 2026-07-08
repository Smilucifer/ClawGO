"""
AkShare 板块资金流 provider.
JSON-RPC 方法: "akshare_sector.sector_fund_flow"。

主数据源：`stock_board_industry_summary_ths()` —— 同花顺板块概览，一次返回全 90
个行业板块，字段最全（总成交额 + 上涨/下跌家数 + 领涨股 + 领涨股涨跌幅），
覆盖盘前 02 段（板块资金流入榜）+ 拥挤度雷达三指标合成所需的全部输入。

回退链：
1. `stock_board_industry_summary_ths()` —— 全字段主源。
2. `stock_sector_fund_flow_rank(indicator="今日", sector_type="行业资金流")`
   —— 东财，只有主力净流入 + 涨跌幅（附加字段留空），断连时兜底。
3. `stock_fund_flow_industry(symbol="即时")` —— 同花顺兜底老接口。

单位约定（对齐 Rust `SectorFlow`）：
- `net_inflow`          亿元
- `change_pct`          %
- `total_turnover`      亿元（板块当日总成交额）
- `total_volume`        万手（板块当日总成交量）
- `advance_count`       家（板块内上涨家数）
- `decline_count`       家（板块内下跌家数）
- `lead_stock`          str（领涨股名）
- `lead_change_pct`     %（领涨股当日涨跌幅）

任意上游异常都会被吞掉并返回 `[]`，上层拿到空列表就当"当日无数据"处理。
"""

import sys


def _warn(msg: str) -> None:
    print(f"[akshare_sector] {msg}", file=sys.stderr, flush=True)


def _to_float(v) -> float | None:
    try:
        f = float(v)
        # NaN / inf 视为空
        if f != f or f in (float("inf"), float("-inf")):
            return None
        return f
    except (TypeError, ValueError):
        return None


def _to_int(v) -> int | None:
    try:
        return int(v)
    except (TypeError, ValueError):
        return None


def _try_ths_summary() -> list[dict] | None:
    """同花顺 stock_board_industry_summary_ths —— 主源，字段最全。

    返回列（akshare 中文化后）：
        序号 / 板块 / 涨跌幅 / 总成交量(万手) / 总成交额(亿元) / 净流入(亿元) /
        上涨家数 / 下跌家数 / 均价 / 领涨股 / 领涨股-最新价 / 领涨股-涨跌幅
    """
    try:
        import akshare as ak
    except ImportError:
        return None

    try:
        df = ak.stock_board_industry_summary_ths()
    except Exception as e:
        _warn(f"THS summary failed: {e!r}")
        return None

    if df is None or df.empty:
        _warn("THS summary returned empty")
        return None

    cols = set(df.columns)
    required = {"板块", "净流入"}
    if not required.issubset(cols):
        _warn(f"THS summary missing key cols; got={list(cols)}")
        return None

    out: list[dict] = []
    for _, row in df.iterrows():
        try:
            name = str(row.get("板块", "")).strip()
            if not name:
                continue
            net = _to_float(row.get("净流入"))
            if net is None:
                continue
            out.append({
                "name":            name,
                "net_inflow":      round(net, 4),                                      # 亿元
                "change_pct":      _to_float(row.get("涨跌幅")),                        # %
                "total_turnover":  _round_opt(_to_float(row.get("总成交额")), 4),        # 亿元
                "total_volume":    _round_opt(_to_float(row.get("总成交量")), 4),        # 万手
                "advance_count":   _to_int(row.get("上涨家数")),                        # 家
                "decline_count":   _to_int(row.get("下跌家数")),                        # 家
                "lead_stock":      _str_or_none(row.get("领涨股")),                     # 股票名
                "lead_change_pct": _to_float(row.get("领涨股-涨跌幅")),                  # %
                "source":          "ths_summary",
            })
        except Exception as e:
            _warn(f"THS summary row parse err: {e!r}")
            continue

    if not out:
        return None
    out.sort(key=lambda r: (r["net_inflow"] is None, -(r["net_inflow"] or 0)))
    return out


def _round_opt(v: float | None, digits: int) -> float | None:
    return None if v is None else round(v, digits)


def _str_or_none(v) -> str | None:
    if v is None:
        return None
    s = str(v).strip()
    return s or None


def _empty_extras() -> dict:
    """兜底源不提供的字段填 None，保持列契约一致。"""
    return {
        "total_turnover":  None,
        "total_volume":    None,
        "advance_count":   None,
        "decline_count":   None,
        "lead_stock":      None,
        "lead_change_pct": None,
    }


def _try_em() -> list[dict] | None:
    """东财 stock_sector_fund_flow_rank —— 兜底 A：有主力净流入 + 净占比。"""
    try:
        import akshare as ak
    except ImportError:
        return None

    try:
        df = ak.stock_sector_fund_flow_rank(indicator="今日", sector_type="行业资金流")
    except Exception as e:
        _warn(f"EM stock_sector_fund_flow_rank failed: {e!r}")
        return None

    if df is None or df.empty:
        _warn("EM rank returned empty")
        return None

    cols = list(df.columns)

    def find(*keys) -> str | None:
        for c in cols:
            cs = str(c)
            if all(k in cs for k in keys):
                return c
        return None

    col_name = find("名称") or find("行业")
    col_chg = find("今日", "涨跌幅") or find("涨跌幅")
    col_net = find("今日", "主力", "净额") or find("主力", "净额")

    if not col_name or not col_net:
        _warn(f"EM rank missing key cols; got={cols}")
        return None

    out: list[dict] = []
    for _, row in df.iterrows():
        try:
            name = str(row[col_name]).strip()
            if not name:
                continue
            net_yuan = _to_float(row[col_net])
            if net_yuan is None:
                continue
            net_yi = net_yuan / 1e8
            chg = _to_float(row[col_chg]) if col_chg else None

            out.append({
                "name":            name,
                "net_inflow":      round(net_yi, 4),
                "change_pct":      chg,
                **_empty_extras(),
                "source":          "eastmoney",
            })
        except Exception as e:
            _warn(f"EM row parse err: {e!r}")
            continue

    if not out:
        return None
    out.sort(key=lambda r: r["net_inflow"], reverse=True)
    return out


def _try_ths_flow() -> list[dict] | None:
    """同花顺 stock_fund_flow_industry(symbol='即时') —— 兜底 B (老接口)。"""
    try:
        import akshare as ak
    except ImportError:
        return None

    try:
        df = ak.stock_fund_flow_industry(symbol="即时")
    except Exception as e:
        _warn(f"THS stock_fund_flow_industry failed: {e!r}")
        return None

    if df is None or df.empty:
        _warn("THS flow returned empty")
        return None

    out: list[dict] = []
    for _, row in df.iterrows():
        try:
            name = str(row.get("行业", "")).strip()
            if not name:
                continue
            net_yi = _to_float(row.get("净额"))
            if net_yi is None:
                continue
            chg = _to_float(row.get("行业-涨跌幅"))
            out.append({
                "name":            name,
                "net_inflow":      round(net_yi, 4),
                "change_pct":      chg,
                **_empty_extras(),
                "source":          "ths",
            })
        except Exception as e:
            _warn(f"THS flow row parse err: {e!r}")
            continue

    if not out:
        return None
    out.sort(key=lambda r: r["net_inflow"], reverse=True)
    return out


def sector_fund_flow() -> list[dict]:
    """行业板块当日主力资金流排名（按主力净流入降序）。

    字段契约见模块 docstring。返回按 net_inflow 降序；数据不可用返回 `[]`，不抛。
    """
    ths_summary = _try_ths_summary()
    if ths_summary:
        return ths_summary
    em = _try_em()
    if em:
        return em
    ths_flow = _try_ths_flow()
    if ths_flow:
        return ths_flow
    _warn("all sources unavailable, returning []")
    return []
