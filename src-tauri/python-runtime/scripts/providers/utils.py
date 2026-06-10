"""
Shared utilities for data providers.

Eliminates duplication of session management, keyword matching, and
timestamp parsing across jinshi, eastmoney, akshare_news, and future providers.
"""

import sys
from datetime import datetime

# ---------------------------------------------------------------------------
# Session factory (lazy import for requests)
# ---------------------------------------------------------------------------

_BASE_USER_AGENT = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36"


def create_session(referer: str, origin: str | None = None):
    """Create a requests.Session with standard headers.

    Returns None if `requests` is not installed (lazy import — avoids
    crashing the RPC server when the package is missing).

    Args:
        referer: Referer header value.
        origin: Optional Origin header value.
    """
    try:
        import requests
    except ImportError:
        return None

    session = requests.Session()
    headers = {
        "User-Agent": _BASE_USER_AGENT,
        "Referer": referer,
    }
    if origin:
        headers["Origin"] = origin
    session.headers.update(headers)
    return session


# ---------------------------------------------------------------------------
# Lazy session with sentinel (avoids retry-on-every-call when requests missing)
# ---------------------------------------------------------------------------

_UNINITIALIZED = object()


class LazySession:
    """Lazy-initialized requests.Session that logs once on failure.

    Usage::

        _session = LazySession("my_provider", referer="https://example.com/")

        def fetch():
            session = _session.get()   # None if requests unavailable
            if session is None:
                return []
            ...
    """

    def __init__(self, provider_name: str, referer: str, origin: str | None = None):
        self._provider = provider_name
        self._referer = referer
        self._origin = origin
        self._session = _UNINITIALIZED

    def get(self):
        if self._session is _UNINITIALIZED:
            self._session = create_session(
                referer=self._referer,
                origin=self._origin,
            )
            if self._session is None:
                print(
                    f"[{self._provider}] requests library not available — provider disabled",
                    file=sys.stderr,
                    flush=True,
                )
        return self._session


# ---------------------------------------------------------------------------
# DataFrame cleaning (EastMoney APIs use "-" for empty cells)
# ---------------------------------------------------------------------------

def clean_dataframe(df):
    """Clean a pandas DataFrame from EastMoney data sources.

    Replaces NaN and literal "-" (EastMoney's empty-cell sentinel) with
    empty strings, so downstream code never sees ``"nan"`` or ``"-"`` as
    field values.

    Args:
        df: A pandas DataFrame.
    Returns:
        The same DataFrame, modified in-place (fillna) and with "-" replaced.
    """
    return df.fillna("").replace("-", "")
# ---------------------------------------------------------------------------

def matches_query(title: str, query: str) -> bool:
    """Check if *title* matches any whitespace/comma-separated keyword in *query*.

    Case-insensitive.  Returns True when *query* is empty.
    """
    keywords = [kw.strip() for kw in query.replace(",", " ").split() if kw.strip()]
    if not keywords:
        return True
    title_lower = title.lower()
    return any(kw.lower() in title_lower for kw in keywords)


# ---------------------------------------------------------------------------
# Timestamp parsing
# ---------------------------------------------------------------------------

_STRPTIME_FORMATS = ["%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M", "%Y-%m-%d"]


def parse_timestamp(time_str: str) -> int:
    """Parse a date/time string into a Unix timestamp (seconds).

    Handles:
    - "2026-06-09 10:30:00" / "2026-06-09" / "2026-06-09 10:30" formats
    - Numeric timestamps (milliseconds auto-converted to seconds)
    - "nan" / empty string → current time (pandas compatibility)

    Returns int Unix timestamp in seconds.
    """
    if not time_str or time_str == "nan":
        return int(datetime.now().timestamp())

    for fmt in _STRPTIME_FORMATS:
        try:
            return int(datetime.strptime(time_str, fmt).timestamp())
        except ValueError:
            continue

    # Numeric timestamp (possibly milliseconds)
    try:
        ts = int(time_str)
        if ts > 1e12:
            return ts // 1000
        return ts
    except ValueError:
        pass

    return int(datetime.now().timestamp())
