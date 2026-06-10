# Discarded Solutions — Python RPC UnicodeEncodeError Fix

## Bug

Jin10 和 AkShare 数据源均报 `Python RPC error: Python RPC IO error: Python process exited`。

## Root Cause

Python server (`server.py`) 在 Windows 上的 stdout 编码默认为系统 ANSI code page（中文系统为 GBK）。当 Jin10/AkShare 返回的新闻数据包含 GBK 无法编码的 Unicode 字符（如 U+200B 零宽空格）时，`print()` 抛出 `UnicodeEncodeError`，导致 server 进程崩溃退出。

关键链路：
1. `json.dumps(resp, ensure_ascii=False)` 产生含特殊 Unicode 的 JSON 字符串
2. `_safe_print()` 调用 `print(text, flush=True)`，stdout 编码为 GBK
3. GBK 无法编码 `​` → `UnicodeEncodeError`
4. `_safe_print` 的 `except (BrokenPipeError, OSError)` **无法捕获** `UnicodeEncodeError`（它不是 `OSError` 子类，而是 `ValueError` 子类）
5. 异常传播到 `main()` 的 `except BaseException`，尝试返回错误响应
6. 错误响应的 `json.dumps` 也可能失败（含相同 Unicode），最终 `_safe_print` 返回 False → `break` → server 退出
7. Rust 端 pending requests 收到 `BrokenPipe` 错误 → "Python process exited"

## Discarded Solutions

### 1. 在 Python 代码中用 `ensure_ascii=True` 替代 `ensure_ascii=False`

**丢弃原因：** `ensure_ascii=True` 会将所有非 ASCII 字符转义为 `\uXXXX` 形式，但这会改变输出格式，可能导致 Rust 端 `serde_json::from_value` 解析异常（中文内容变成转义序列）。而且这只是治标不治本——如果将来有其他地方输出 Unicode，问题会再次出现。

### 2. 在 `_safe_print` 中用 `errors='replace'` 或 `errors='ignore'`

**丢弃原因：** 需要重新打开 stdout 或修改 `sys.stdout` 的错误处理模式，侵入性强。而且丢弃字符会导致 JSON 响应不完整，Rust 端解析失败。`PYTHONIOENCODING=utf-8` 是更干净的方案。

### 3. 在 Python 代码中手动设置 `sys.stdout.reconfigure(encoding='utf-8')`

**丢弃原因：** 可行，但需要在 server.py 的 `main()` 入口处调用，且 Python 3.7+ 才支持 `TextIOWrapper.reconfigure()`。不如环境变量方案简洁，且如果 server.py 被其他方式调用（非 main），设置不会生效。

### 4. 在 Rust 端用 `encoding_rs` 解码 stdout 字节流

**丢弃原因：** 过度复杂。Rust 的 `tokio::io::BufReader::lines()` 默认用 UTF-8 解码，如果 Python 输出 GBK 编码的中文，解码会失败或产生乱码。根本解决应该在 Python 端统一编码。

## Applied Fix

**Rust 端 (`bridge.rs`)：** 在 spawn Python 进程时设置 `PYTHONIOENCODING=utf-8` 环境变量，强制 Python 使用 UTF-8 编码 stdin/stdout/stderr。

**Python 端 (`server.py`)：** 在 `_safe_print` 的 except 中增加 `UnicodeEncodeError` 捕获，作为防御性措施。

## Why This Works

`PYTHONIOENCODING=utf-8` 告诉 Python 解释器在初始化 I/O 时使用 UTF-8 编码，覆盖 Windows 默认的 GBK。UTF-8 可以编码所有 Unicode 字符，包括 U+200B 零宽空格。这是 Python 官方推荐的跨平台 Unicode 处理方式。
