# -*- coding: utf-8 -*-
"""测试 miniQMT (xtdata) 取数模块是否能正确运行。

使用方法：
  cd src-tauri/python-runtime
  python/python.exe scripts/test_xtdata.py
"""

import sys
import json
import traceback

# 添加 scripts 目录到 path，以便导入 providers
sys.path.insert(0, "scripts")

def test_import():
    """测试 xtquant 是否能正常导入。"""
    print("=" * 60)
    print("[测试 1] xtquant 模块导入")
    print("=" * 60)
    try:
        from xtquant import xtdata
        print("  [OK] xtquant.xtdata 导入成功")
        print(f"  [INFO] 模块路径: {xtdata.__file__}")
        return True
    except ImportError as e:
        print(f"  [FAIL] 导入失败: {e}")
        print("  [TIP] 提示: 需要安装 xtquant 包或确保 QMT 客户端已安装")
        return False
    except Exception as e:
        print(f"  [FAIL] 导入异常: {e}")
        traceback.print_exc()
        return False


def test_health():
    """测试 miniQMT 客户端健康检查。"""
    print("\n" + "=" * 60)
    print("[测试 2] miniQMT 客户端健康检查 (health)")
    print("=" * 60)
    try:
        from providers import xtdata
        result = xtdata.health()
        available = result.get("available", False)
        reason = result.get("reason", "")

        if available:
            print("  [OK] miniQMT 客户端在线")
            print(f"  [INFO] 状态: {json.dumps(result, ensure_ascii=False, indent=2)}")
            return True
        else:
            print("  [WARN] miniQMT 客户端不可用")
            print(f"  [INFO] 状态: {json.dumps(result, ensure_ascii=False, indent=2)}")
            if reason:
                print(f"  [TIP] 原因: {reason}")
            return False
    except Exception as e:
        print(f"  [FAIL] 健康检查失败: {e}")
        traceback.print_exc()
        return False


def test_kline(symbol: str = "000001.SZ", period: str = "1d", count: int = 5):
    """测试获取历史 K线数据。"""
    print("\n" + "=" * 60)
    print(f"[测试 3] 获取历史 K线 (kline)")
    print(f"  股票代码: {symbol}")
    print(f"  周期: {period}")
    print(f"  数量: {count}")
    print("=" * 60)
    try:
        from providers import xtdata
        result = xtdata.kline(symbol=symbol, period=period, count=count)
        items = result.get("items", [])
        source = result.get("source", "unknown")

        if items:
            print(f"  [OK] 成功获取 {len(items)} 根 K线 (数据源: {source})")
            print("  [DATA] 最近数据:")
            for i, bar in enumerate(items[-3:]):  # 只显示最后 3 根
                print(f"     [{i+1}] {bar['trade_date']} "
                      f"开={bar['open']:.2f} 高={bar['high']:.2f} "
                      f"低={bar['low']:.2f} 收={bar['close']:.2f} "
                      f"量={bar['vol']:.0f}")
            return True
        else:
            print("  [WARN] 返回数据为空")
            print(f"  [INFO] 原始响应: {json.dumps(result, ensure_ascii=False, indent=2)}")
            return False
    except Exception as e:
        print(f"  [FAIL] K线获取失败: {e}")
        traceback.print_exc()
        return False


def test_realtime_quote(symbols=None):
    """测试获取实时行情快照。"""
    if symbols is None:
        symbols = ["000001.SZ", "600519.SH"]

    print("\n" + "=" * 60)
    print(f"[测试 4] 获取实时行情 (realtime_quote)")
    print(f"  股票代码: {', '.join(symbols)}")
    print("=" * 60)
    try:
        from providers import xtdata
        result = xtdata.realtime_quote(symbols=symbols)

        if result:
            print(f"  [OK] 成功获取 {len(result)} 只股票行情")
            print("  [DATA] 实时行情:")
            for code, quote in result.items():
                print(f"     {code}: 最新价={quote['last']:.2f} "
                      f"成交量={quote['volume']:.0f} "
                      f"成交额={quote['amount']:.0f}")
            return True
        else:
            print("  [WARN] 返回数据为空")
            print(f"  [INFO] 原始响应: {json.dumps(result, ensure_ascii=False, indent=2)}")
            return False
    except Exception as e:
        print(f"  [FAIL] 实时行情获取失败: {e}")
        traceback.print_exc()
        return False


def test_rpc_server():
    """测试通过 JSON-RPC 调用 xtdata。"""
    print("\n" + "=" * 60)
    print("[测试 5] JSON-RPC 调用测试")
    print("=" * 60)

    import subprocess
    import os

    server_path = os.path.join(os.path.dirname(__file__), "server.py")
    python_exe = os.path.join(os.path.dirname(__file__), "..", "python", "python.exe")

    if not os.path.exists(python_exe):
        # 尝试从 scripts 目录查找
        python_exe = os.path.join(os.path.dirname(__file__), "..", "python-runtime", "python", "python.exe")

    if not os.path.exists(python_exe):
        print("  [WARN] 找不到 Python 解释器，跳过 RPC 测试")
        return None

    print(f"  [PATH] Python: {python_exe}")
    print(f"  [PATH] Server: {server_path}")

    try:
        # 启动 server 进程
        proc = subprocess.Popen(
            [python_exe, server_path],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            cwd=os.path.dirname(os.path.dirname(__file__))  # python-runtime 目录
        )

        # 发送 ping 测试
        ping_req = json.dumps({"jsonrpc": "2.0", "method": "ping", "id": 1})
        proc.stdin.write(ping_req + "\n")
        proc.stdin.flush()

        # 读取响应 (可能有多行非 JSON 输出)
        response = ""
        for _ in range(10):  # 最多读 10 行找到 JSON 响应
            line = proc.stdout.readline().strip()
            if not line:
                continue
            try:
                resp = json.loads(line)
                response = line
                break
            except json.JSONDecodeError:
                continue  # 跳过非 JSON 行

        if response:
            resp = json.loads(response)
            if resp.get("result") == "pong":
                print("  [OK] JSON-RPC 服务正常 (ping -> pong)")
            else:
                print(f"  [WARN] 异常响应: {response}")
        else:
            print("  [FAIL] 无响应")

        # 测试 xtdata.health
        health_req = json.dumps({"jsonrpc": "2.0", "method": "xtdata.health", "id": 2})
        proc.stdin.write(health_req + "\n")
        proc.stdin.flush()

        response = None
        for _ in range(10):
            line = proc.stdout.readline().strip()
            if not line:
                continue
            try:
                resp = json.loads(line)
                response = line
                break
            except json.JSONDecodeError:
                continue

        if response:
            resp = json.loads(response)
            result = resp.get("result", {})
            available = result.get("available", False)
            if available:
                print("  [OK] RPC xtdata.health: 客户端在线")
            else:
                reason = result.get("reason", "unknown")
                print(f"  [WARN] RPC xtdata.health: 客户端不可用 ({reason})")
        else:
            print("  [FAIL] xtdata.health 无响应")

        # 测试 xtdata.kline
        kline_req = json.dumps({
            "jsonrpc": "2.0",
            "method": "xtdata.kline",
            "params": {"symbol": "000001.SZ", "period": "1d", "count": 3},
            "id": 3
        })
        proc.stdin.write(kline_req + "\n")
        proc.stdin.flush()

        response = None
        for _ in range(10):
            line = proc.stdout.readline().strip()
            if not line:
                continue
            try:
                resp = json.loads(line)
                response = line
                break
            except json.JSONDecodeError:
                continue

        if response:
            resp = json.loads(response)
            if "result" in resp:
                items = resp["result"].get("items", [])
                if items:
                    print(f"  [OK] RPC xtdata.kline: 获取 {len(items)} 根 K线")
                else:
                    print("  [WARN] RPC xtdata.kline: 返回空数据")
            elif "error" in resp:
                print(f"  [WARN] RPC xtdata.kline 错误: {resp['error'].get('message', 'unknown')}")
        else:
            print("  [FAIL] xtdata.kline 无响应")

        # 关闭进程
        proc.stdin.close()
        proc.wait(timeout=5)

        return True

    except Exception as e:
        print(f"  [FAIL] RPC 测试失败: {e}")
        traceback.print_exc()
        try:
            proc.kill()
        except:
            pass
        return False


def main():
    """运行所有测试。"""
    print("\n" + "=" * 60)
    print("  miniQMT (xtdata) 取数模块测试")
    print("=" * 60 + "\n")

    results = {}

    # 测试 1: 导入
    results["import"] = test_import()

    # 测试 2: 健康检查 (需要先导入成功)
    if results["import"]:
        results["health"] = test_health()
    else:
        results["health"] = None
        print("\n[SKIP] 跳过健康检查 (导入失败)")

    # 测试 3: K线数据 (需要客户端在线)
    if results.get("health"):
        results["kline"] = test_kline()
    else:
        results["kline"] = None
        print("\n[SKIP] 跳过 K线测试 (客户端不可用)")

    # 测试 4: 实时行情 (需要客户端在线)
    if results.get("health"):
        results["realtime"] = test_realtime_quote()
    else:
        results["realtime"] = None
        print("\n[SKIP] 跳过实时行情测试 (客户端不可用)")

    # 测试 5: RPC 调用
    results["rpc"] = test_rpc_server()

    # 汇总结果
    print("\n" + "=" * 60)
    print("[SUMMARY] 测试结果汇总")
    print("=" * 60)

    status_map = {
        True: "[PASS] 通过",
        False: "[FAIL] 失败",
        None: "[SKIP] 跳过"
    }

    for test_name, result in results.items():
        status = status_map.get(result, "[?] 未知")
        print(f"  {test_name:12} : {status}")

    # 判断整体结果
    passed = sum(1 for v in results.values() if v is True)
    failed = sum(1 for v in results.values() if v is False)
    skipped = sum(1 for v in results.values() if v is None)

    print(f"\n  总计: {passed} 通过, {failed} 失败, {skipped} 跳过")

    if failed == 0:
        print("\n[DONE] 所有测试通过！miniQMT 取数模块工作正常。")
        return 0
    else:
        print("\n[WARN] 部分测试失败，请检查 miniQMT 客户端是否已启动。")
        return 1


if __name__ == "__main__":
    sys.exit(main())
