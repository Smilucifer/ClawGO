# -*- coding: utf-8 -*-
"""诊断 Tushare moneyflow_dc 接口数据问题。

测试东山精密(002245.SZ)的主力资金流入/流出数据为何为 0。

使用方法：
  cd src-tauri/python-runtime
  python/python.exe scripts/test_moneyflow_dc.py [可选: tushare_token]
"""

import sys
import os
import json
import traceback
from datetime import datetime, timedelta

# 添加 scripts 目录到 path
sys.path.insert(0, "scripts")


def load_tushare_token(override_token: str = None) -> str:
    """从 settings.json 读取 tushare_token，或使用命令行传入的值。"""
    if override_token:
        return override_token

    # 尝试从 ~/.claw-go/settings.json 读取
    home = os.path.expanduser("~")
    settings_path = os.path.join(home, ".claw-go", "settings.json")

    if not os.path.exists(settings_path):
        print(f"  [WARN] settings.json 不存在: {settings_path}")
        return ""

    try:
        with open(settings_path, "r", encoding="utf-8") as f:
            settings = json.load(f)
        token = settings.get("user", {}).get("tushare_token", "")
        if not token:
            # 尝试顶层
            token = settings.get("tushare_token", "")
        return token or ""
    except Exception as e:
        print(f"  [WARN] 读取 settings.json 失败: {e}")
        return ""


def load_proxy_url() -> str:
    """从 settings.json 读取 tushare_proxy_url。"""
    home = os.path.expanduser("~")
    settings_path = os.path.join(home, ".claw-go", "settings.json")

    if not os.path.exists(settings_path):
        return "https://api.tushare.pro"

    try:
        with open(settings_path, "r", encoding="utf-8") as f:
            settings = json.load(f)
        proxy = settings.get("user", {}).get("tushare_proxy_url", "")
        if not proxy:
            proxy = settings.get("tushare_proxy_url", "")
        return proxy if proxy else "https://api.tushare.pro"
    except Exception:
        return "https://api.tushare.pro"


def call_tushare_api(token: str, api_name: str, params: dict, fields: str = "", base_url: str = "") -> dict:
    """调用 Tushare Pro API。"""
    import requests

    body = {
        "api_name": api_name,
        "token": token,
        "params": params,
        "fields": fields,
    }

    print(f"\n  [API] POST {base_url}")
    print(f"  [API] api_name={api_name}, params={json.dumps(params, ensure_ascii=False)}")

    resp = requests.post(base_url, json=body, timeout=30)
    resp.raise_for_status()
    data = resp.json()

    if data.get("code") != 0:
        print(f"  [ERROR] API 返回错误: code={data.get('code')}, msg={data.get('msg')}")
        return data

    return data


def test_moneyflow_dc(token: str, base_url: str, ts_code: str = "002245.SZ"):
    """测试 moneyflow_dc 接口。"""
    print("\n" + "=" * 70)
    print(f"[测试] Tushare moneyflow_dc - {ts_code} (东山精密)")
    print("=" * 70)

    # 计算日期范围（最近 5 个交易日）
    now = datetime.now()
    end_date = now.strftime("%Y%m%d")
    start_date = (now - timedelta(days=10)).strftime("%Y%m%d")

    print(f"\n  [INFO] 日期范围: {start_date} ~ {end_date}")
    print(f"  [INFO] 股票代码: {ts_code}")
    print(f"  [INFO] API 地址: {base_url}")

    try:
        data = call_tushare_api(token, "moneyflow_dc", {
            "ts_code": ts_code,
            "start_date": start_date,
            "end_date": end_date,
        }, base_url=base_url)

        if data.get("code") != 0:
            return

        fields = data.get("data", {}).get("fields", [])
        items = data.get("data", {}).get("items", [])

        print(f"\n  [RESULT] 返回字段: {fields}")
        print(f"  [RESULT] 返回行数: {len(items)}")

        if not items:
            print("\n  [DIAGNOSIS] ⚠️  API 返回空数据！")
            print("  [DIAGNOSIS] 可能原因:")
            print("    1. Tushare 积分不足，moneyflow_dc 接口需要 2000+ 积分")
            print("    2. 该股票不在 moneyflow_dc 覆盖范围")
            print("    3. 日期范围内无交易日数据")
            print(f"\n  [TEST] 尝试其他股票验证 API 是否可用...")
            test_other_stock(token, base_url)
            return

        # 打印原始数据
        print("\n  [RAW DATA] 原始数据:")
        print(f"  {'trade_date':>12} | {'buy_sm_amount':>14} | {'buy_md_amount':>14} | {'buy_lg_amount':>14} | {'buy_elg_amount':>14} | {'net_amount':>14}")
        print("  " + "-" * 100)

        for row in items:
            # 按字段位置提取
            row_dict = {}
            for i, f in enumerate(fields):
                if i < len(row):
                    row_dict[f] = row[i]

            trade_date = row_dict.get("trade_date", "N/A")
            buy_sm = row_dict.get("buy_sm_amount", "N/A")
            buy_md = row_dict.get("buy_md_amount", "N/A")
            buy_lg = row_dict.get("buy_lg_amount", "N/A")
            buy_elg = row_dict.get("buy_elg_amount", "N/A")
            net = row_dict.get("net_amount", "N/A")

            print(f"  {str(trade_date):>12} | {str(buy_sm):>14} | {str(buy_md):>14} | {str(buy_lg):>14} | {str(buy_elg):>14} | {str(net):>14}")

        # 计算聚合值
        print("\n  [AGGREGATE] 聚合计算 (模拟 Rust aggregate_moneyflow):")
        main_net = 0.0  # 主力 = 超大单 + 大单
        retail_net = 0.0  # 散户 = 中单 + 小单

        for row in items:
            row_dict = {}
            for i, f in enumerate(fields):
                if i < len(row):
                    row_dict[f] = row[i]

            buy_sm = _to_float(row_dict.get("buy_sm_amount"))
            buy_md = _to_float(row_dict.get("buy_md_amount"))
            buy_lg = _to_float(row_dict.get("buy_lg_amount"))
            buy_elg = _to_float(row_dict.get("buy_elg_amount"))

            main_net += (buy_elg or 0.0) + (buy_lg or 0.0)
            retail_net += (buy_md or 0.0) + (buy_sm or 0.0)

        main_yi = main_net / 10000.0  # 万元 → 亿元
        retail_yi = retail_net / 10000.0

        main_label = "主力净流入" if main_net >= 0 else "主力净流出"
        retail_label = "散户净流入" if retail_net >= 0 else "散户净流出"

        print(f"  [AGGREGATE] {main_label} {abs(main_yi):.2f}亿元")
        print(f"  [AGGREGATE] {retail_label} {abs(retail_yi):.2f}亿元")

        # 仅最新一天
        if items:
            latest = max(items, key=lambda r: r[fields.index("trade_date")] if "trade_date" in fields else "")
            latest_dict = {}
            for i, f in enumerate(fields):
                if i < len(latest):
                    latest_dict[f] = latest[i]

            latest_main = (_to_float(latest_dict.get("buy_elg_amount")) or 0.0) + (_to_float(latest_dict.get("buy_lg_amount")) or 0.0)
            latest_retail = (_to_float(latest_dict.get("buy_md_amount")) or 0.0) + (_to_float(latest_dict.get("buy_sm_amount")) or 0.0)
            latest_main_yi = latest_main / 10000.0
            latest_retail_yi = latest_retail / 10000.0
            latest_main_label = "主力净流入" if latest_main >= 0 else "主力净流出"
            latest_retail_label = "散户净流入" if latest_retail >= 0 else "散户净流出"

            latest_date = latest_dict.get("trade_date", "N/A")
            print(f"\n  [LATEST] 仅最新一天 ({latest_date}):")
            print(f"  [LATEST] {latest_main_label} {abs(latest_main_yi):.2f}亿元")
            print(f"  [LATEST] {latest_retail_label} {abs(latest_retail_yi):.2f}亿元")

        # 诊断
        if main_net == 0.0 and retail_net == 0.0:
            print("\n  [DIAGNOSIS] ⚠️  主力和散户资金流均为 0！")
            print("  [DIAGNOSIS] 检查各字段是否全为 None/0:")
            all_none = True
            for row in items:
                row_dict = {}
                for i, f in enumerate(fields):
                    if i < len(row):
                        row_dict[f] = row[i]
                for key in ["buy_sm_amount", "buy_md_amount", "buy_lg_amount", "buy_elg_amount"]:
                    val = row_dict.get(key)
                    if val is not None and val != 0:
                        all_none = False
                        break
                if not all_none:
                    break

            if all_none:
                print("  [DIAGNOSIS] 确认：所有 buy_*_amount 字段均为 None 或 0")
                print("  [DIAGNOSIS] 这是 Tushare API 返回的原始数据问题，不是代码 bug")
            else:
                print("  [DIAGNOSIS] 部分字段非零，可能是聚合逻辑问题")

    except Exception as e:
        print(f"\n  [FAIL] 测试失败: {e}")
        traceback.print_exc()


def test_other_stock(token: str, base_url: str):
    """用其他常见股票验证 API 是否可用。"""
    test_codes = ["000001.SZ", "600519.SH", "000858.SZ"]

    for code in test_codes:
        print(f"\n  [TEST] 测试 {code}...")
        try:
            now = datetime.now()
            data = call_tushare_api(token, "moneyflow_dc", {
                "ts_code": code,
                "start_date": (now - timedelta(days=5)).strftime("%Y%m%d"),
                "end_date": now.strftime("%Y%m%d"),
            }, base_url=base_url)

            if data.get("code") != 0:
                continue

            items = data.get("data", {}).get("items", [])
            fields = data.get("data", {}).get("fields", [])

            if items:
                print(f"  [TEST] {code}: 返回 {len(items)} 条数据 ✓")
                # 打印最新一条
                latest = items[-1]
                row_dict = {}
                for i, f in enumerate(fields):
                    if i < len(latest):
                        row_dict[f] = latest[i]
                print(f"  [TEST] 最新数据: {json.dumps(row_dict, ensure_ascii=False)}")
            else:
                print(f"  [TEST] {code}: 返回空数据 ✗")

        except Exception as e:
            print(f"  [TEST] {code}: 失败 - {e}")


def _to_float(val) -> float | None:
    """安全转换为 float，None/空值返回 None。"""
    if val is None:
        return None
    try:
        return float(val)
    except (ValueError, TypeError):
        return None


def main():
    """主函数。"""
    print("\n" + "=" * 70)
    print("  Tushare moneyflow_dc 诊断工具")
    print("  诊断东山精密(002245.SZ)主力资金流入/流出为 0 的原因")
    print("=" * 70)

    # 获取 token
    override_token = sys.argv[1] if len(sys.argv) > 1 else None
    token = load_tushare_token(override_token)

    if not token:
        print("\n  [ERROR] 未找到 Tushare token！")
        print("  [TIP] 使用方法: python test_moneyflow_dc.py <your_tushare_token>")
        print("  [TIP] 或在 Claw GO 设置中配置 tushare_token")
        return 1

    base_url = load_proxy_url()
    print(f"\n  [CONFIG] Tushare token: {token[:8]}...{token[-4:]}")
    print(f"  [CONFIG] API 地址: {base_url}")

    # 测试 1: 东山精密
    test_moneyflow_dc(token, base_url, "002245.SZ")

    # 测试 2: 检查 stock_basic 确认代码正确
    print("\n" + "=" * 70)
    print("[验证] stock_basic 确认东山精密代码")
    print("=" * 70)
    try:
        data = call_tushare_api(token, "stock_basic", {
            "name": "东山精密",
        }, fields="ts_code,symbol,name,market", base_url=base_url)

        if data.get("code") == 0:
            items = data.get("data", {}).get("items", [])
            fields = data.get("data", {}).get("fields", [])
            print(f"\n  [RESULT] 东山精密匹配结果: {len(items)} 条")
            for row in items:
                row_dict = {}
                for i, f in enumerate(fields):
                    if i < len(row):
                        row_dict[f] = row[i]
                print(f"  [RESULT] {json.dumps(row_dict, ensure_ascii=False)}")
    except Exception as e:
        print(f"  [FAIL] stock_basic 查询失败: {e}")

    print("\n" + "=" * 70)
    print("[DONE] 诊断完成")
    print("=" * 70)

    return 0


if __name__ == "__main__":
    sys.exit(main())
