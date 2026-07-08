"""
ClawGO Python Data Server
JSON-RPC over stdin/stdout for Yahoo Finance and future data providers.
"""

import sys
import json
import importlib
import traceback

# Provider registry
PROVIDERS = {}


def register_provider(name: str, module_name: str):
    """Lazy-load provider module on first use."""
    PROVIDERS[name] = module_name


def get_provider(name: str):
    """Import and return provider module."""
    if name not in PROVIDERS:
        raise ValueError(f"Unknown provider: {name}")
    module_name = PROVIDERS[name]
    try:
        module = importlib.import_module(f"providers.{module_name}")
    except ImportError as e:
        raise ValueError(f"Provider '{name}' import failed (missing dependency?): {e}") from e
    except Exception as e:
        raise ValueError(f"Provider '{name}' import error: {e}") from e
    return module


def handle_builtin(method: str, req_id) -> dict | None:
    """Handle built-in methods that don't belong to any provider."""
    if method == "ping":
        return {"jsonrpc": "2.0", "result": "pong", "id": req_id}
    if method == "sys.version":
        return {"jsonrpc": "2.0", "result": sys.version, "id": req_id}
    if method == "yfinance.version":
        try:
            import yfinance
            ver = getattr(yfinance, "__version__", "unknown")
        except ImportError:
            ver = "not installed"
        return {"jsonrpc": "2.0", "result": ver, "id": req_id}
    return None


def handle_request(req: dict) -> dict:
    """Process a single JSON-RPC request and return response."""
    req_id = req.get("id")
    method = req.get("method", "")
    params = req.get("params", {})

    try:
        # Check built-in methods first (ping, sys.version, yfinance.version)
        builtin = handle_builtin(method, req_id)
        if builtin is not None:
            return builtin

        # Route: "provider.method" -> providers.{provider}.{method}
        parts = method.split(".", 1)
        if len(parts) != 2:
            return {"jsonrpc": "2.0", "error": {"code": -32601, "message": f"Invalid method: {method}"}, "id": req_id}

        provider_name, func_name = parts
        provider = get_provider(provider_name)

        func = getattr(provider, func_name, None)
        if func is None:
            return {"jsonrpc": "2.0", "error": {"code": -32601, "message": f"Unknown method: {method}"}, "id": req_id}

        result = func(**params)
        return {"jsonrpc": "2.0", "result": result, "id": req_id}

    except Exception as e:
        tb = traceback.format_exc()
        print(f"[server] Error in {method}: {e}\n{tb}", file=sys.stderr, flush=True)
        return {"jsonrpc": "2.0", "error": {"code": -32000, "message": str(e)}, "id": req_id}


def _safe_print(text: str) -> bool:
    """Write to stdout, returning False if the pipe is broken or encoding fails."""
    try:
        print(text, flush=True)
        return True
    except (BrokenPipeError, OSError, UnicodeEncodeError):
        return False


def main():
    # Register providers
    register_provider("yahoo", "yahoo")
    register_provider("eastmoney", "eastmoney")
    register_provider("jinshi", "jinshi")
    register_provider("akshare", "akshare_news")
    register_provider("akshare_market", "akshare_market")
    register_provider("xtdata", "xtdata")
    register_provider("sentiment", "sentiment")
    register_provider("xueqiu", "xueqiu")

    print("[server] ClawGO Python Data Server started", file=sys.stderr, flush=True)

    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue

        try:
            req = json.loads(line)
        except json.JSONDecodeError as e:
            resp = {"jsonrpc": "2.0", "error": {"code": -32700, "message": f"Parse error: {e}"}, "id": None}
            if not _safe_print(json.dumps(resp)):
                break
            continue

        try:
            resp = handle_request(req)
        except BaseException as e:
            # Last-resort catch: prevent the server loop from dying on ANY
            # unhandled error (including SystemExit, MemoryError, etc.).
            req_id = req.get("id")
            tb = traceback.format_exc()
            try:
                print(f"[server] Unhandled error in main loop: {e}\n{tb}", file=sys.stderr, flush=True)
            except (BrokenPipeError, OSError):
                pass  # stderr also broken — nothing we can do
            resp = {"jsonrpc": "2.0", "error": {"code": -32603, "message": f"Internal error: {e}"}, "id": req_id}

        if not _safe_print(json.dumps(resp, ensure_ascii=False)):
            break


if __name__ == "__main__":
    main()
