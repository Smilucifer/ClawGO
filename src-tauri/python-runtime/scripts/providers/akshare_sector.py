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


# ---------------------------------------------------------------------------
# 东财板块成分 + 强度端点 (B5a)
# ---------------------------------------------------------------------------

def _normalize_code(v) -> str | None:
    """将各种格式的股票代码统一为6位纯数字字符串。"""
    if v is None:
        return None
    s = str(v).strip()
    if not s:
        return None
    # 去掉点号 (如 600519.SH)
    s = s.replace(".", "").replace(" ", "")
    # 只保留数字
    s = "".join(c for c in s if c.isdigit())
    if not s:
        return None
    # 取后6位并补零
    return s[-6:].zfill(6)


def _detect_code_col(columns: list[str]) -> str | None:
    """从 DataFrame 列名中检测股票代码列。"""
    candidates = ["代码", "成分券代码", "股票代码", "证券代码", "code"]
    for c in columns:
        cs = str(c).strip()
        for kw in candidates:
            if cs == kw:
                return c
    # 模糊匹配
    for c in columns:
        cs = str(c).strip().lower()
        if "代码" in cs or "code" in cs:
            return c
    return None


def _fetch_board_cons(ak, board_name: str, board_type: str) -> list[dict]:
    """获取单个东财板块的成分股列表。"""
    try:
        if board_type == "industry":
            df = ak.stock_board_industry_cons_em(symbol=board_name)
        else:
            df = ak.stock_board_concept_cons_em(symbol=board_name)
    except Exception as e:
        _warn(f"board_cons_em fetch '{board_name}' ({board_type}) failed: {e!r}")
        return []

    if df is None or df.empty:
        return []

    code_col = _detect_code_col(list(df.columns))
    if not code_col:
        _warn(f"board_cons_em cannot detect code col for '{board_name}'; cols={list(df.columns)}")
        return []

    out: list[dict] = []
    for _, row in df.iterrows():
        code = _normalize_code(row.get(code_col))
        if not code:
            continue
        out.append({
            "ts_code": code,
            "board_name": board_name,
            "board_type": board_type,
        })
    return out


def board_cons_em(board_type: str | None = None) -> list[dict]:
    """东财板块成分股列表。

    Args:
        board_type: None=行业+概念, "industry"=仅行业, "concept"=仅概念。

    Returns:
        [{"ts_code": "600519", "board_name": "白酒", "board_type": "industry"}, ...]
    """
    try:
        import akshare as ak
    except ImportError:
        _warn("akshare not installed, skipping board_cons_em")
        return []

    do_industry = board_type in (None, "industry")
    do_concept = board_type in (None, "concept")

    results: list[dict] = []

    if do_industry:
        try:
            name_df = ak.stock_board_industry_name_em()
        except Exception as e:
            _warn(f"board_cons_em industry name list failed: {e!r}")
            name_df = None

        if name_df is not None and not name_df.empty:
            # 板块名在 "板块名称" 列
            board_names_col = None
            for c in name_df.columns:
                cs = str(c).strip()
                if "板块名称" in cs or cs == "名称":
                    board_names_col = c
                    break
            if board_names_col is None:
                # fallback: first column that looks like a name
                for c in name_df.columns:
                    if "名称" in str(c):
                        board_names_col = c
                        break

            if board_names_col is not None:
                for _, r in name_df.iterrows():
                    bname = str(r.get(board_names_col, "")).strip()
                    if not bname:
                        continue
                    results.extend(_fetch_board_cons(ak, bname, "industry"))
            else:
                _warn(f"board_cons_em industry: cannot find board name col; got={list(name_df.columns)}")

    if do_concept:
        try:
            name_df = ak.stock_board_concept_name_em()
        except Exception as e:
            _warn(f"board_cons_em concept name list failed: {e!r}")
            name_df = None

        if name_df is not None and not name_df.empty:
            board_names_col = None
            for c in name_df.columns:
                cs = str(c).strip()
                if "板块名称" in cs or cs == "名称":
                    board_names_col = c
                    break
            if board_names_col is None:
                for c in name_df.columns:
                    if "名称" in str(c):
                        board_names_col = c
                        break

            if board_names_col is not None:
                for _, r in name_df.iterrows():
                    bname = str(r.get(board_names_col, "")).strip()
                    if not bname:
                        continue
                    results.extend(_fetch_board_cons(ak, bname, "concept"))
            else:
                _warn(f"board_cons_em concept: cannot find board name col; got={list(name_df.columns)}")

    return results


def _detect_col(columns: list[str], candidates: list[str]) -> str | None:
    """按候选列表检测 DataFrame 列名，精确匹配优先，模糊匹配兜底。"""
    for kw in candidates:
        for c in columns:
            if str(c).strip() == kw:
                return c
    for kw in candidates:
        for c in columns:
            if kw in str(c):
                return c
    return None


def sector_strength_em() -> list[dict]:
    """东财板块强度（涨跌幅 + 主力净流入）。

    Returns:
        [{"board_name": "...", "board_type": "industry"|"concept",
          "change_pct": float|None, "net_amount": float|None (亿元)}, ...]
    """
    try:
        import akshare as ak
    except ImportError:
        _warn("akshare not installed, skipping sector_strength_em")
        return []

    results: list[dict] = []

    for stype, board_type in [("行业资金流", "industry"), ("概念资金流", "concept")]:
        try:
            df = ak.stock_sector_fund_flow_rank(indicator="今日", sector_type=stype)
        except Exception as e:
            _warn(f"sector_strength_em {stype} failed: {e!r}")
            continue

        if df is None or df.empty:
            _warn(f"sector_strength_em {stype} returned empty")
            continue

        cols = list(df.columns)
        name_col = _detect_col(cols, ["名称", "板块名称", "行业名称", "概念名称"])
        pct_col = _detect_col(cols, ["今日涨跌幅", "涨跌幅"])
        net_col = _detect_col(cols, ["今日主力净流入-净额", "主力净流入-净额", "主力净流入净额"])

        if not name_col:
            _warn(f"sector_strength_em {stype}: cannot find name col; got={cols}")
            continue

        for _, row in df.iterrows():
            name = str(row.get(name_col, "")).strip()
            if not name:
                continue
            pct = _to_float(row.get(pct_col)) if pct_col else None
            net_yuan = _to_float(row.get(net_col)) if net_col else None
            net_yi = round(net_yuan / 1e8, 4) if net_yuan is not None else None

            results.append({
                "board_name": name,
                "board_type": board_type,
                "change_pct": pct,
                "net_amount": net_yi,
            })

    return results
