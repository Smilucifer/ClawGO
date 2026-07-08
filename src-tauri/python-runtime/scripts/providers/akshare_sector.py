"""
AkShare 板块资金流 provider.
JSON-RPC 方法: "akshare_sector.sector_fund_flow"。

产出行业板块（东财口径 → 同花顺兜底）当日资金流：主力净流入 / 涨跌幅 /
换手率 / 主力净占比。单位统一到 **亿元 / %**。异常返回空列表 + stderr
warning，绝不抛。上层拿到空列表就当"当日无数据"处理。

数据源优先级：
1. `stock_sector_fund_flow_rank` (东财, 字段最全: 主力净流入 + 净占比 + 换手率)
2. `stock_fund_flow_industry(symbol='即时')` (同花顺, 兜底: 只有净额 + 涨跌幅, 无换手率/净占比)

单位约定：
- 东财原始接口返回主力净流入单位是 **元**, 已除 1e8 转 **亿元**
- 同花顺 "净额" 原始单位是 **亿元**, 直接透传
- 涨跌幅 / 换手率 / 主力净占比 单位是 **%**
"""

import sys


def _warn(msg: str) -> None:
    print(f"[akshare_sector] {msg}", file=sys.stderr, flush=True)


def _try_em() -> list[dict] | None:
    """东财 stock_sector_fund_flow_rank —— 字段最全 (含换手率 + 主力净占比)。
    endpoint 有时抽风 (push2.eastmoney.com 502)，失败返回 None 让上层走兜底。
    """
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

    # 东财实际列名（akshare 已中文化）：
    #   名称 / 今日涨跌幅 / 今日主力净流入-净额 / 今日主力净流入-净占比 /
    #   今日超大单净流入-净额 / ... / 今日主力净流入最大股 / 今日涨跌幅[主力]
    # 换手率 EM 该接口其实不返回, 后面补 None
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
    col_pct = find("今日", "主力", "净占比") or find("主力", "净占比")

    if not col_name or not col_net:
        _warn(f"EM rank missing key cols; got={cols}")
        return None

    out: list[dict] = []
    for _, row in df.iterrows():
        try:
            name = str(row[col_name]).strip()
            if not name:
                continue
            net_yuan = row[col_net]
            # EM 返回 "元", 转 "亿元"
            try:
                net_yi = float(net_yuan) / 1e8
            except (TypeError, ValueError):
                continue
            chg = None
            if col_chg is not None:
                try:
                    chg = float(row[col_chg])
                except (TypeError, ValueError):
                    chg = None
            pct = None
            if col_pct is not None:
                try:
                    pct = float(row[col_pct])
                except (TypeError, ValueError):
                    pct = None

            out.append({
                "name": name,
                "net_inflow": round(net_yi, 4),        # 亿元
                "change_pct": chg,                     # %
                "turnover_rate": None,                 # EM 该接口不给, THS 也不给, 保留字段
                "main_inflow_pct": pct,                # %
                "source": "eastmoney",
            })
        except Exception as e:
            _warn(f"EM row parse err: {e!r}")
            continue

    if not out:
        return None
    out.sort(key=lambda r: r["net_inflow"], reverse=True)
    return out


def _try_ths() -> list[dict] | None:
    """同花顺 stock_fund_flow_industry(symbol='即时') —— 稳定兜底, 但缺换手率/净占比。"""
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
        _warn("THS returned empty")
        return None

    # 列: 序号 / 行业 / 行业指数 / 行业-涨跌幅 / 流入资金 / 流出资金 / 净额 /
    #      公司家数 / 领涨股 / 领涨股-涨跌幅 / 当前价
    # 单位: 流入/流出/净额 = 亿元 (THS 惯例)
    out: list[dict] = []
    for _, row in df.iterrows():
        try:
            name = str(row.get("行业", "")).strip()
            if not name:
                continue
            try:
                net_yi = float(row.get("净额", 0.0))
            except (TypeError, ValueError):
                continue
            try:
                chg = float(row.get("行业-涨跌幅"))
            except (TypeError, ValueError):
                chg = None

            out.append({
                "name": name,
                "net_inflow": round(net_yi, 4),        # 亿元 (THS 原始就是亿元)
                "change_pct": chg,                     # %
                "turnover_rate": None,                 # THS 该接口不提供
                "main_inflow_pct": None,               # THS 该接口不提供
                "source": "ths",
            })
        except Exception as e:
            _warn(f"THS row parse err: {e!r}")
            continue

    if not out:
        return None
    out.sort(key=lambda r: r["net_inflow"], reverse=True)
    return out


def sector_fund_flow() -> list[dict]:
    """行业板块当日主力资金流排名 (按主力净流入降序)。

    Returns: list of dict, 每行:
        {
          "name":            str,           # 板块名
          "net_inflow":      float,         # 主力净流入 (亿元)
          "change_pct":      float | None,  # 板块涨跌幅 (%)
          "turnover_rate":   float | None,  # 换手率 (%) —— 当前上游均不提供, 保留兼容位
          "main_inflow_pct": float | None,  # 主力净占比 (%) —— 仅东财可用
          "source":          "eastmoney" | "ths",
        }

    数据不可用时返回 `[]`, 不抛。
    """
    em = _try_em()
    if em:
        return em
    ths = _try_ths()
    if ths:
        return ths
    _warn("both EM and THS unavailable, returning []")
    return []
