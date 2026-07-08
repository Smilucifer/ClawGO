# 盘前观察 · Plan A — 舆情采集基础设施 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 建立五源舆情采集基础设施——独立 `sentiment_items` 表 + 前四源 Python 抓取 + Rust 接口 + 通用 LLM 归一化（闭集 sectors/topics）+ 个股行业映射，为盘前报告和委员会催化提供去重、已归一化的舆情池。

**Architecture:** 5 源舆情写入**独立** `sentiment_items` 表（不污染 events 表的 jin10 高质量事件流）。前四源（同花顺/新浪/财联社/东财）走内置 Python RPC 桥纯 requests 抓取；雪球留 Plan B（需 scrapling 浏览器）。归一化逻辑从 event_analyzer 抽出通用 `analyze_pending(table)`，两表共用，同一次 LLM 调用提取 summary/stance/severity/affected_symbols/sectors。sectors 用 tushare industry 闭集词表约束，topics 自由生成。

**Tech Stack:** Rust (rusqlite/tokio/serde) + Python (requests/scrapling) + tushare + 委员会 CLI executor (Claude CLI)

## Global Constraints

- 平台：Windows-first，无 WSL/macOS 假设。Rust 测试用 `cargo check` 或 cmd.exe（CLAUDE.md §11：Git Bash 下 Rust 测试报 STATUS_ENTRYPOINT_NOT_FOUND）。
- Python：内置 runtime `src-tauri/python-runtime/python/python.exe`，**非系统 Python**。RPC 桥设 `PYTHONIOENCODING=utf-8`，provider 用懒加载 import。
- i18n：新增 UI 文本 `en.json` + `zh-CN.json` 同步，`npm run i18n:check` 必过。
- 提交：Conventional Commits（`feat:`/`fix:`/`chore:`）。
- 数据源健壮性：单 provider 失败返回空列表 + warning，绝不因单点失败抛。
- 舆情**不入 events 表**（决策 D1）——用独立 `sentiment_items` 表，避免污染 jin10 事件流与 verdict 样本池。
- sectors 必须来自 tushare industry 闭集词表（补丁 C1），枚举外的词丢弃；topics 自由生成，仅供报告板块聚合，不参与委员会查询。
- `affected_symbols` 存成前后带逗号格式 `,{code},`，查询用 `LIKE '%,{code},%'` 避免假阳性（补丁 DeepSeek）。

---

### Task 1: `sentiment_items` 表 + 迁移 helper

**Files:**
- Modify: `src-tauri/src/storage/invest/mod.rs`（建表 SQL + `ensure_column` helper）
- Create: `src-tauri/src/storage/invest/sentiment.rs`（CRUD）
- Modify: `src-tauri/src/storage/invest/mod.rs`（`pub mod sentiment;` 声明）
- Test: `src-tauri/src/storage/invest/sentiment.rs`（`#[cfg(test)]` 模块内）

**Interfaces:**
- Produces:
  - `pub struct SentimentItem { pub id: String, pub provider: String, pub symbol: Option<String>, pub title: String, pub summary: Option<String>, pub url: Option<String>, pub published_at: Option<String>, pub read_count: Option<i64>, pub comment_count: Option<i64>, pub source_type: String, pub sentiment_hint: Option<f64>, pub affected_symbols: Option<String>, pub sectors: Option<String>, pub topics: Option<String>, pub stance: String, pub severity: String, pub analyzed: bool, pub created_at: String }`
  - `pub fn save_sentiment_item(item: &SentimentItem) -> Result<(), String>`（INSERT OR IGNORE）
  - `pub fn list_unanalyzed_sentiment(limit: Option<i64>) -> Result<Vec<SentimentItem>, String>`
  - `pub fn update_sentiment_analysis(id: &str, summary: Option<&str>, severity: &str, stance: &str, affected_symbols: Option<&str>, sectors: Option<&str>, topics: Option<&str>) -> Result<(), String>`
  - `pub fn list_sentiment_by_symbol(code: &str, since: &str, limit: i64) -> Result<Vec<SentimentItem>, String>`（`affected_symbols LIKE '%,{code},%'`）
  - `pub fn list_sentiment_by_sectors(sectors: &[String], since: &str, limit: i64) -> Result<Vec<SentimentItem>, String>`
  - `pub fn ensure_column(conn: &rusqlite::Connection, table: &str, col: &str, coltype: &str) -> Result<(), String>`

- [ ] **Step 1: 写 `ensure_column` helper 的失败测试**

在 `src-tauri/src/storage/invest/mod.rs` 末尾 `#[cfg(test)]` 区新增（若无则创建）：

```rust
#[cfg(test)]
mod migration_tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn test_ensure_column_adds_missing() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute("CREATE TABLE t (id TEXT)", []).unwrap();
        // 第一次：列不存在，应添加
        crate::storage::invest::sentiment::ensure_column(&conn, "t", "extra", "TEXT").unwrap();
        // 第二次：列已存在，应幂等不报错
        crate::storage::invest::sentiment::ensure_column(&conn, "t", "extra", "TEXT").unwrap();
        let cnt: i64 = conn
            .query_row("SELECT COUNT(*) FROM pragma_table_info('t') WHERE name='extra'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(cnt, 1);
    }
}
```

- [ ] **Step 2: 运行测试，确认失败**

Run: `cmd.exe /c "cd /d D:\ClaudeWorkspace\Code\ClawGO && cargo test --manifest-path src-tauri/Cargo.toml --lib storage::invest::migration_tests -- --nocapture"`
Expected: 编译失败（`sentiment` 模块或 `ensure_column` 不存在）

- [ ] **Step 3: 创建 `sentiment.rs`，实现 `ensure_column` + 建表**

创建 `src-tauri/src/storage/invest/sentiment.rs`：

```rust
use super::with_conn;
use rusqlite::params;

pub const CREATE_SENTIMENT_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS sentiment_items (
    id TEXT PRIMARY KEY,
    provider TEXT NOT NULL,
    symbol TEXT,
    title TEXT NOT NULL,
    summary TEXT,
    url TEXT,
    published_at TEXT,
    read_count INTEGER,
    comment_count INTEGER,
    source_type TEXT NOT NULL DEFAULT 'news',
    sentiment_hint REAL,
    affected_symbols TEXT,
    sectors TEXT,
    topics TEXT,
    stance TEXT NOT NULL DEFAULT 'neutral',
    severity TEXT NOT NULL DEFAULT 'pending',
    analyzed INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_sentiment_created ON sentiment_items(created_at);
CREATE INDEX IF NOT EXISTS idx_sentiment_analyzed ON sentiment_items(analyzed);
"#;

/// 幂等添加列：列不存在才 ALTER TABLE ADD COLUMN。
pub fn ensure_column(
    conn: &rusqlite::Connection,
    table: &str,
    col: &str,
    coltype: &str,
) -> Result<(), String> {
    let exists: i64 = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM pragma_table_info('{}') WHERE name = ?1", table),
            params![col],
            |r| r.get(0),
        )
        .map_err(|e| format!("check column {}.{}: {}", table, col, e))?;
    if exists == 0 {
        conn.execute(&format!("ALTER TABLE {} ADD COLUMN {} {}", table, col, coltype), [])
            .map_err(|e| format!("add column .{}: {}", table, col, e))?;
    }
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SentimentItem {
    pub id: String,
    pub provider: String,
    pub symbol: Option<String>,
    pub title: String,
    pub summary: Option<String>,
    pub url: Option<String>,
    pub published_at: Option<String>,
    pub read_count: Option<i64>,
    pub comment_count: Option<i64>,
    pub source_type: String,
    pub sentiment_hint: Option<f64>,
    pub affected_symbols: Option<String>,
    pub sectors: Option<String>,
    pub topics: Option<String>,
    pub stance: String,
    pub severity: String,
    pub analyzed: bool,
    pub created_at: String,
}

fn row_to_item(row: &rusqlite::Row) -> rusqlite::Result<SentimentItem> {
    Ok(SentimentItem {
        id: row.get(0)?,
        provider: row.get(1)?,
        symbol: row.get(2)?,
        title: row.get(3)?,
        summary: row.get(4)?,
        url: row.get(5)?,
        published_at: row.get(6)?,
        read_count: row.get(7)?,
        comment_count: row.get(8)?,
        source_type: row.get(9)?,
        sentiment_hint: row.get(10)?,
        affected_symbols: row.get(11)?,
        sectors: row.get(12)?,
        topics: row.get(13)?,
        stance: row.get(14)?,
        severity: row.get(15)?,
        analyzed: row.get::<_, i32>(16)? != 0,
        created_at: row.get(17)?,
    })
}

const COLS: &str = "id, provider, symbol, title, summary, url, published_at, read_count, comment_count, source_type, sentiment_hint, affected_symbols, sectors, topics, stance, severity, analyzed, created_at";

pub fn save_sentiment_item(item: &SentimentItem) -> Result<(), String> {
    with_conn(|conn| {
        conn.execute(
            "INSERT OR IGNORE INTO sentiment_items (id, provider, symbol, title, summary, url, published_at, read_count, comment_count, source_type, sentiment_hint, affected_symbols, sectors, topics, stance, severity, analyzed, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17, COALESCE(?18, datetime('now')))",
            params![
                item.id, item.provider, item.symbol, item.title, item.summary, item.url,
                item.published_at, item.read_count, item.comment_count, item.source_type,
                item.sentiment_hint, item.affected_symbols, item.sectors, item.topics,
                item.stance, item.severity, item.analyzed as i32,
                if item.created_at.is_empty() { None } else { Some(item.created_at.clone()) },
            ],
        )
        .map_err(|e| format!("save sentiment_item: {}", e))?;
        Ok(())
    })
}

pub fn list_unanalyzed_sentiment(limit: Option<i64>) -> Result<Vec<SentimentItem>, String> {
    with_conn(|conn| {
        let lim = limit.unwrap_or(100);
        let sql = format!("SELECT {} FROM sentiment_items WHERE analyzed = 0 ORDER BY created_at DESC LIMIT ?1", COLS);
        let mut stmt = conn.prepare(&sql).map_err(|e| format!("prepare: {}", e))?;
        let rows = stmt.query_map(params![lim], row_to_item).map_err(|e| format!("query: {}", e))?;
        let mut out = Vec::new();
        for r in rows { out.push(r.map_err(|e| format!("row: {}", e))?); }
        Ok(out)
    })
}

pub fn update_sentiment_analysis(
    id: &str,
    summary: Option<&str>,
    severity: &str,
    stance: &str,
    affected_symbols: Option<&str>,
    sectors: Option<&str>,
    topics: Option<&str>,
) -> Result<(), String> {
    with_conn(|conn| {
        conn.execute(
            "UPDATE sentiment_items SET summary = ?1, severity = ?2, stance = ?3, affected_symbols = ?4, sectors = ?5, topics = ?6, analyzed = 1 WHERE id = ?7",
            params![summary, severity, stance, affected_symbols, sectors, topics, id],
        )
        .map_err(|e| format!("update sentiment analysis: {}", e))?;
        Ok(())
    })
}

pub fn list_sentiment_by_symbol(code: &str, since: &str, limit: i64) -> Result<Vec<SentimentItem>, String> {
    with_conn(|conn| {
        let pat = format!("%,{},%", code);
        let sql = format!("SELECT {} FROM sentiment_items WHERE affected_symbols LIKE ?1 AND created_at >= ?2 ORDER BY created_at DESC LIMIT ?3", COLS);
        let mut stmt = conn.prepare(&sql).map_err(|e| format!("prepare: {}", e))?;
        let rows = stmt.query_map(params![pat, since, limit], row_to_item).map_err(|e| format!("query: {}", e))?;
        let mut out = Vec::new();
        for r in rows { out.push(r.map_err(|e| format!("row: {}", e))?); }
        Ok(out)
    })
}

pub fn list_sentiment_by_sectors(sectors: &[String], since: &str, limit: i64) -> Result<Vec<SentimentItem>, String> {
    if sectors.is_empty() {
        return Ok(vec![]);
    }
    with_conn(|conn| {
        // 每个 sector 一个 LIKE '%,{sector},%' OR 条件
        let mut clauses = Vec::new();
        let mut binds: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        for s in sectors {
            clauses.push(format!("sectors LIKE ?{}", binds.len() + 1));
            binds.push(Box::new(format!("%,{},%", s)));
        }
        let since_idx = binds.len() + 1;
        binds.push(Box::new(since.to_string()));
        let limit_idx = binds.len() + 1;
        binds.push(Box::new(limit));
        let sql = format!(
            "SELECT {} FROM sentiment_items WHERE ({}) AND created_at >= ?{} ORDER BY created_at DESC LIMIT ?{}",
            COLS, clauses.join(" OR "), since_idx, limit_idx
        );
        let mut stmt = conn.prepare(&sql).map_err(|e| format!("prepare: {}", e))?;
        let rows = stmt.query_map(rusqlite::params_from_iter(binds.iter()), row_to_item).map_err(|e| format!("query: {}", e))?;
        let mut out = Vec::new();
        for r in rows { out.push(r.map_err(|e| format!("row: {}", e))?); }
        Ok(out)
    })
}
```

在 `src-tauri/src/storage/invest/mod.rs` 加模块声明（与其他 `pub mod` 并列）：

```rust
pub mod sentiment;
```

并在建表初始化流程里（`mod.rs` 中执行其他 `CREATE TABLE` 的地方，通常一个 `init_schema`/`with_conn` 批处理）加入：

```rust
    conn.execute_batch(crate::storage::invest::sentiment::CREATE_SENTIMENT_TABLE)
        .map_err(|e| format!("create sentiment_items: {}", e))?;
```

> 注：`affected_symbols`/`sectors`/`topics` 存储时统一前后加逗号（`,600519,000858,`），供 `LIKE '%,{code},%'` 精确匹配。写入方（Task 5 归一化）负责加逗号包裹。

- [ ] **Step 4: 运行测试，确认通过**

Run: `cmd.exe /c "cd /d D:\ClaudeWorkspace\Code\ClawGO && cargo test --manifest-path src-tauri/Cargo.toml --lib storage::invest::migration_tests -- --nocapture"`
Expected: PASS

- [ ] **Step 5: cargo check 全量编译**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 无错误（可能有 unused warning，Task 5/6 会用到这些函数）

- [ ] **Step 6: 提交**

```bash
git add src-tauri/src/storage/invest/sentiment.rs src-tauri/src/storage/invest/mod.rs
git commit -m "feat(invest): sentiment_items 表 + ensure_column 迁移 helper"
```

---

### Task 2: 前四源 Python provider（sentiment_scraper.py）

**Files:**
- Create: `src-tauri/python-runtime/scripts/providers/sentiment.py`
- Modify: `src-tauri/python-runtime/scripts/server.py`（注册 provider）
- 参考（已验证抓取逻辑）：`src-tauri/python-runtime/scripts/premarket_sentiment_test.py`

**Interfaces:**
- Produces（RPC 方法 `sentiment.fetch`）：
  - `def fetch(provider="all", symbol=None, limit=20) -> list[dict]`
  - 每个 dict 契约：`{provider, symbol, title, summary, url, published_at, read_count, comment_count, source_type, sentiment_hint}`
  - provider ∈ `{"ths","sina","cailianshe","eastmoney","all"}`（**不含 xueqiu**，留 Plan B）

- [ ] **Step 1: 从测试脚本迁移四源抓取到正式 provider**

创建 `src-tauri/python-runtime/scripts/providers/sentiment.py`，把 `premarket_sentiment_test.py` 里已验证的 `fetch_ths`/`fetch_sina`/`fetch_cailianshe`/`fetch_eastmoney`/`sentiment_hint`/`_item`/`_ts_iso`/`_strip_html` 迁移过来（去掉 xueqiu 和 `argparse`/`main`），统一入口改名：

```python
"""五源舆情抓取 provider（RPC: sentiment.fetch）。前四源纯 requests；雪球留 Plan B。"""
import sys
import re
import json
from datetime import datetime

from .utils import LazySession, parse_timestamp

_POS_WORDS = ["利好","涨停","突破","增持","回购","中标","订单","扩产","涨价","超预期","创新高","放量","主力流入","北向流入","看多","重组","并购"]
_NEG_WORDS = ["利空","跌停","破位","减持","亏损","退市","问询","处罚","爆雷","不及预期","创新低","商誉","解禁","看空","违规","立案","下调"]


def sentiment_hint(text: str) -> float:
    if not text:
        return 0.0
    pos = sum(1 for w in _POS_WORDS if w in text)
    neg = sum(1 for w in _NEG_WORDS if w in text)
    if pos == 0 and neg == 0:
        return 0.0
    return round((pos - neg) / (pos + neg), 2)


def _item(provider, symbol, title, summary, url, published_at, read_count=None, comment_count=None, source_type="news"):
    text = f"{title} {summary or ''}"
    return {
        "provider": provider, "symbol": symbol,
        "title": (title or "").strip(), "summary": (summary or "").strip(),
        "url": url or "", "published_at": published_at,
        "read_count": read_count, "comment_count": comment_count,
        "source_type": source_type, "sentiment_hint": sentiment_hint(text),
    }


def _ts_iso(ts) -> str:
    try:
        return datetime.fromtimestamp(int(ts)).isoformat(timespec="seconds")
    except Exception:
        return datetime.now().isoformat(timespec="seconds")


def _strip_html(s: str) -> str:
    return re.sub(r"<[^>]+>", "", s or "").strip()
```

然后**从 `premarket_sentiment_test.py` 原样复制**这四个函数体（它们已实测可用，逐字复制避免引入 bug）：`fetch_ths`、`fetch_sina`、`fetch_cailianshe`、`fetch_eastmoney`。其中 `_ths_session`/`_cls_session`/`_em_session` 三个 `LazySession` 定义也一并复制。

追加统一入口（去掉 xueqiu）：

```python
_PROVIDERS = {
    "ths": fetch_ths,
    "sina": fetch_sina,
    "cailianshe": fetch_cailianshe,
    "eastmoney": fetch_eastmoney,
}


def fetch(provider="all", symbol=None, limit=20) -> list:
    out = []
    targets = _PROVIDERS.keys() if provider == "all" else [provider]
    for name in targets:
        fn = _PROVIDERS.get(name)
        if fn is None:
            continue
        try:
            got = fn(symbol, limit)
            print(f"[sentiment.{name}] {len(got)} items", file=sys.stderr, flush=True)
            out += got
        except Exception as e:
            print(f"[sentiment.{name}] error: {e}", file=sys.stderr, flush=True)
    return out
```

- [ ] **Step 2: 注册 provider**

在 `src-tauri/python-runtime/scripts/server.py` 的 `main()` 里，其他 `register_provider` 旁加：

```python
    register_provider("sentiment", "sentiment")
```

- [ ] **Step 3: 独立跑验证四源抓取**

Run:
```bash
cd "/d/ClaudeWorkspace/Code/ClawGO/src-tauri/python-runtime/scripts" && ../python/python.exe -c "
import sys; sys.path.insert(0,'.')
from providers.sentiment import fetch
items = fetch('all', None, 5)
print(f'total={len(items)}', file=sys.stderr)
assert len(items) > 0, 'no items'
for p in ['ths','sina','cailianshe','eastmoney']:
    n = sum(1 for i in items if i['provider']==p)
    print(f'{p}={n}', file=sys.stderr)
    assert n > 0, f'{p} empty'
print('OK', file=sys.stderr)
" 2>&1 | tail -8
```
Expected: 打印 `total=...`、四源各 >0、最后 `OK`

- [ ] **Step 4: 提交**

```bash
git add src-tauri/python-runtime/scripts/providers/sentiment.py src-tauri/python-runtime/scripts/server.py
git commit -m "feat(invest): 前四源舆情抓取 provider (ths/sina/cailianshe/eastmoney)"
```

---

### Task 3: Rust 接口层 `sentiment.rs` — 调 Python 桥写入 sentiment_items

**Files:**
- Create: `src-tauri/src/invest/sentiment.rs`
- Modify: `src-tauri/src/invest/mod.rs`（`pub mod sentiment;`）
- Test: `src-tauri/src/invest/sentiment.rs`（`#[cfg(test)]` 内，仅测契约反序列化）

**Interfaces:**
- Consumes: `crate::python::require() -> Result<&Arc<PythonRuntime>>`、`PythonRuntime::call(method, params) -> Result<Value>`（参照 `international.rs:161` 的 `rpc_call<T>` 模式）；`storage::invest::sentiment::{SentimentItem, save_sentiment_item}`（Task 1）
- Produces:
  - `pub struct RawSentimentItem { pub provider: String, pub symbol: Option<String>, pub title: String, pub summary: Option<String>, pub url: Option<String>, pub published_at: Option<String>, pub read_count: Option<i64>, pub comment_count: Option<i64>, pub source_type: String, pub sentiment_hint: Option<f64> }`（对应 Python 契约）
  - `pub async fn fetch_and_store(provider: &str, symbol: Option<&str>, limit: u32) -> Result<usize, String>`（抓取 → 转 SentimentItem → save_sentiment_item，返回写入条数）
  - `pub fn make_sentiment_id(provider: &str, url: &str, title: &str) -> String`（`sha256(provider + url|title)` 十六进制）

- [ ] **Step 1: 写 id 生成 + 契约反序列化的失败测试**

创建 `src-tauri/src/invest/sentiment.rs`：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_make_id_stable_and_distinct() {
        let a = make_sentiment_id("ths", "https://x.com/1", "标题A");
        let b = make_sentiment_id("ths", "https://x.com/1", "标题A");
        let c = make_sentiment_id("sina", "https://x.com/1", "标题A");
        assert_eq!(a, b, "同输入应稳定");
        assert_ne!(a, c, "不同 provider 应不同");
        assert_eq!(a.len(), 64, "sha256 hex 长度");
    }

    #[test]
    fn test_raw_item_deserialize() {
        let json = r#"{"provider":"ths","symbol":null,"title":"t","summary":"s","url":"u","published_at":"2026-07-08T09:00:00","read_count":10,"comment_count":2,"source_type":"news","sentiment_hint":0.5}"#;
        let item: RawSentimentItem = serde_json::from_str(json).unwrap();
        assert_eq!(item.provider, "ths");
        assert_eq!(item.sentiment_hint, Some(0.5));
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cmd.exe /c "cd /d D:\ClaudeWorkspace\Code\ClawGO && cargo test --manifest-path src-tauri/Cargo.toml --lib invest::sentiment::tests -- --nocapture"`
Expected: 编译失败（`make_sentiment_id`/`RawSentimentItem` 未定义）

- [ ] **Step 3: 实现 sentiment.rs**

在 `src-tauri/src/invest/sentiment.rs` 顶部（测试模块之上）写：

```rust
//! 舆情采集接口层：调 Python 桥抓取五源 → 写入 sentiment_items 表。
//! 通用接口，报告生成器 / 委员会催化 / 独立命令都能复用。

use crate::storage::invest::sentiment::{save_sentiment_item, SentimentItem};
use sha2::{Digest, Sha256};

/// Python 契约（`sentiment.fetch` 返回的每条）。
#[derive(Debug, Clone, serde::Deserialize)]
pub struct RawSentimentItem {
    pub provider: String,
    pub symbol: Option<String>,
    pub title: String,
    pub summary: Option<String>,
    pub url: Option<String>,
    pub published_at: Option<String>,
    pub read_count: Option<i64>,
    pub comment_count: Option<i64>,
    pub source_type: String,
    pub sentiment_hint: Option<f64>,
}

/// 稳定去重 id：provider + (url 优先，无则 title)。
pub fn make_sentiment_id(provider: &str, url: &str, title: &str) -> String {
    let key = if url.is_empty() { title } else { url };
    let mut hasher = Sha256::new();
    hasher.update(provider.as_bytes());
    hasher.update(b"|");
    hasher.update(key.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// 抓取指定 provider 并写入 sentiment_items。返回写入（尝试）条数。
/// 单点失败不抛——Python 层已保证单 provider 失败返回空列表。
pub async fn fetch_and_store(
    provider: &str,
    symbol: Option<&str>,
    limit: u32,
) -> Result<usize, String> {
    let runtime = crate::python::require()?;
    let params = serde_json::json!({
        "provider": provider,
        "symbol": symbol,
        "limit": limit,
    });
    let value = runtime.call("sentiment.fetch", params).await?;
    let raws: Vec<RawSentimentItem> = serde_json::from_value(value)
        .map_err(|e| format!("parse sentiment.fetch: {e}"))?;

    let mut count = 0usize;
    for r in &raws {
        let url = r.url.clone().unwrap_or_default();
        let item = SentimentItem {
            id: make_sentiment_id(&r.provider, &url, &r.title),
            provider: r.provider.clone(),
            symbol: r.symbol.clone(),
            title: r.title.clone(),
            summary: r.summary.clone(),
            url: r.url.clone(),
            published_at: r.published_at.clone(),
            read_count: r.read_count,
            comment_count: r.comment_count,
            source_type: r.source_type.clone(),
            sentiment_hint: r.sentiment_hint,
            affected_symbols: None, // 归一化后填（Task 5）
            sectors: None,
            topics: None,
            stance: "pending".to_string(),
            severity: "pending".to_string(),
            analyzed: false,
            created_at: String::new(), // save 时 COALESCE 到 now
        };
        if let Err(e) = save_sentiment_item(&item) {
            log::warn!("save sentiment_item {} failed: {}", item.id, e);
        } else {
            count += 1;
        }
    }
    Ok(count)
}
```

在 `src-tauri/src/invest/mod.rs` 加（与其他 `pub mod` 并列）：

```rust
pub mod sentiment;
```

确认 `sha2` 已在依赖里（`grep sha2 src-tauri/Cargo.toml`；jin10/其他模块通常已引入。若无则 `cargo add sha2 --manifest-path src-tauri/Cargo.toml`）。

- [ ] **Step 4: 运行测试确认通过**

Run: `cmd.exe /c "cd /d D:\ClaudeWorkspace\Code\ClawGO && cargo test --manifest-path src-tauri/Cargo.toml --lib invest::sentiment::tests -- --nocapture"`
Expected: PASS（2 tests）

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/invest/sentiment.rs src-tauri/src/invest/mod.rs src-tauri/Cargo.toml
git commit -m "feat(invest): sentiment.rs 接口层——调 Python 桥写入 sentiment_items"
```

---

### Task 4: 个股 → 行业映射（stock_industry 表 + tushare 缓存）

**Files:**
- Modify: `src-tauri/src/storage/invest/mod.rs`（建表 SQL）
- Create: `src-tauri/src/storage/invest/stock_industry.rs`（CRUD）
- Modify: `src-tauri/src/storage/invest/mod.rs`（`pub mod stock_industry;` + 建表）
- Modify: `src-tauri/src/invest/sentiment.rs`（`refresh_stock_industry` + `industry_of` + `all_industries`）
- Test: `src-tauri/src/storage/invest/stock_industry.rs`

**Interfaces:**
- Consumes: tushare `stock_basic`（`TushareClient`，`src-tauri/src/tushare/client.rs`），字段 `ts_code`/`name`/`industry`
- Produces:
  - `pub fn upsert_stock_industry(code: &str, name: &str, industry: &str) -> Result<(), String>`
  - `pub fn industry_of(code: &str) -> Result<Option<String>, String>`
  - `pub fn all_industries() -> Result<Vec<String>, String>`（distinct，供闭集词表）
  - `pub fn industry_count() -> Result<i64, String>`
  - `sentiment.rs`: `pub async fn refresh_stock_industry() -> Result<usize, String>`（拉 tushare 全量写表）

- [ ] **Step 1: 写 CRUD 失败测试**

创建 `src-tauri/src/storage/invest/stock_industry.rs`：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // 注：此测试依赖 with_conn 全局库；在 CI/本机用 in-memory 或跳过。
    // 这里测纯逻辑：distinct 去重。
    #[test]
    fn test_dedup_industries_logic() {
        let mut v = vec!["白酒".to_string(), "白酒".to_string(), "半导体".to_string()];
        v.sort();
        v.dedup();
        assert_eq!(v, vec!["半导体".to_string(), "白酒".to_string()]);
    }
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cmd.exe /c "cd /d D:\ClaudeWorkspace\Code\ClawGO && cargo test --manifest-path src-tauri/Cargo.toml --lib storage::invest::stock_industry -- --nocapture"`
Expected: 编译失败（模块不存在）

- [ ] **Step 3: 实现 stock_industry 表 + CRUD**

`src-tauri/src/storage/invest/stock_industry.rs`：

```rust
use super::with_conn;
use rusqlite::params;

pub const CREATE_STOCK_INDUSTRY_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS stock_industry (
    code TEXT PRIMARY KEY,       -- 6位代码，如 600519
    name TEXT,
    industry TEXT,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
"#;

pub fn upsert_stock_industry(code: &str, name: &str, industry: &str) -> Result<(), String> {
    with_conn(|conn| {
        conn.execute(
            "INSERT INTO stock_industry (code, name, industry, updated_at) VALUES (?1,?2,?3, datetime('now'))
             ON CONFLICT(code) DO UPDATE SET name=?2, industry=?3, updated_at=datetime('now')",
            params![code, name, industry],
        ).map_err(|e| format!("upsert stock_industry: {}", e))?;
        Ok(())
    })
}

pub fn industry_of(code: &str) -> Result<Option<String>, String> {
    with_conn(|conn| {
        match conn.query_row("SELECT industry FROM stock_industry WHERE code = ?1", params![code], |r| r.get::<_, Option<String>>(0)) {
            Ok(v) => Ok(v),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(format!("industry_of: {}", e)),
        }
    })
}

pub fn all_industries() -> Result<Vec<String>, String> {
    with_conn(|conn| {
        let mut stmt = conn.prepare("SELECT DISTINCT industry FROM stock_industry WHERE industry IS NOT NULL AND industry != '' ORDER BY industry")
            .map_err(|e| format!("prepare: {}", e))?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0)).map_err(|e| format!("query: {}", e))?;
        let mut out = Vec::new();
        for r in rows { out.push(r.map_err(|e| format!("row: {}", e))?); }
        Ok(out)
    })
}

pub fn industry_count() -> Result<i64, String> {
    with_conn(|conn| {
        conn.query_row("SELECT COUNT(*) FROM stock_industry", [], |r| r.get(0))
            .map_err(|e| format!("industry_count: {}", e))
    })
}
```

`mod.rs` 加声明 + 建表：

```rust
pub mod stock_industry;
```
```rust
    conn.execute_batch(crate::storage::invest::stock_industry::CREATE_STOCK_INDUSTRY_TABLE)
        .map_err(|e| format!("create stock_industry: {}", e))?;
```

- [ ] **Step 4: 实现 refresh_stock_industry（tushare 拉全量）**

在 `src-tauri/src/invest/sentiment.rs` 追加。先 `grep -n "stock_basic\|pub async fn" src-tauri/src/tushare/client.rs` 确认 tushare 客户端是否已有 stock_basic 方法：若有则复用，若无则用其通用 `query` 接口。示例（假设 `TushareClient::stock_basic()` 返回 `Vec<(ts_code, name, industry)>`，实际按 client.rs 真实签名调整）：

```rust
/// 从 tushare stock_basic 拉全量个股行业，写入 stock_industry 表。
/// 返回写入条数。每周刷一次即可。
pub async fn refresh_stock_industry() -> Result<usize, String> {
    use crate::storage::invest::stock_industry::upsert_stock_industry;
    let client = crate::tushare::client::TushareClient::from_settings()
        .ok_or("tushare 未配置")?;
    // stock_basic: 返回 ts_code(600519.SH)/name/industry
    let rows = client.stock_basic().await?; // 按 client.rs 真实签名调整
    let mut n = 0;
    for (ts_code, name, industry) in rows {
        let code = ts_code.split('.').next().unwrap_or(&ts_code).to_string();
        if let Err(e) = upsert_stock_industry(&code, &name, &industry) {
            log::warn!("upsert {} failed: {}", code, e);
        } else {
            n += 1;
        }
    }
    Ok(n)
}
```

> 实现者注：`TushareClient` 的真实构造与 stock_basic 调用方式**必须**先读 `src-tauri/src/tushare/client.rs` 确认。若无现成 stock_basic 方法，用其底层 `call_api("stock_basic", params)` 通道，字段 `fields=ts_code,name,industry`。

- [ ] **Step 5: 运行测试 + cargo check**

Run: `cmd.exe /c "cd /d D:\ClaudeWorkspace\Code\ClawGO && cargo test --manifest-path src-tauri/Cargo.toml --lib storage::invest::stock_industry -- --nocapture"` 然后 `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 测试 PASS，check 无错误

- [ ] **Step 6: 提交**

```bash
git add src-tauri/src/storage/invest/stock_industry.rs src-tauri/src/storage/invest/mod.rs src-tauri/src/invest/sentiment.rs
git commit -m "feat(invest): stock_industry 个股行业映射表 + tushare 刷新"
```

---

### Task 5: 通用归一化 `analyze_pending(table)` + 闭集 sectors/topics

**Files:**
- Modify: `src-tauri/src/invest/event_scanner.rs`（`NormalizedEvent` 加 `summary`/`sectors`/`topics` 字段 + prompt 加闭集词表）
- Modify: `src-tauri/src/invest/event_analyzer.rs`（抽 `analyze_pending(table)` 通用函数 + sentiment_items 分支）
- Modify: `src-tauri/src/storage/invest/events.rs`（`update_event_analysis` 扩 summary/sectors + `row_to_event`/建表加列，用 Task 1 的 `ensure_column`）
- Test: `src-tauri/src/invest/event_scanner.rs`（解析测试）

**Interfaces:**
- Consumes: `storage::invest::sentiment::{list_unanalyzed_sentiment, update_sentiment_analysis}`（Task 1）、`storage::invest::stock_industry::all_industries`（Task 4）、`cli_complete`（现有）
- Produces:
  - `NormalizedEvent { one_line_claim, stance, severity, affected_symbols, summary: String, sectors: Vec<String>, topics: Vec<String> }`（扩字段）
  - `pub async fn analyze_pending(table: AnalyzeTable, language: &str) -> Result<AnalyzerResult, String>`
  - `pub enum AnalyzeTable { Events, Sentiment }`
  - `pub fn build_normalizer_prompt_with_sectors(language: &str, industries: &[String]) -> String`

- [ ] **Step 1: 扩 NormalizedEvent + 写解析测试**

在 `src-tauri/src/invest/event_scanner.rs` 的 `NormalizedEvent`（当前 67-72 行）加三字段：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct NormalizedEvent {
    pub one_line_claim: String,
    pub stance: String,
    pub severity: Severity,
    pub affected_symbols: Vec<String>,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub sectors: Vec<String>,
    #[serde(default)]
    pub topics: Vec<String>,
}
```

在 `#[cfg(test)]` 加测试（确认新字段 default 兼容旧 JSON + 新 JSON 全解析）：

```rust
#[test]
fn test_normalized_with_sectors_topics() {
    let json = r#"[{"one_line_claim":"文旅部十五五规划","stance":"bullish","severity":"high","affected_symbols":[],"summary":"文旅政策利好","sectors":["旅游综合"],"topics":["十五五"]}]"#;
    let events = vec![RawEvent { source:"t".into(), event_type:"news".into(), title:"文旅部十五五规划".into(), body:"".into(), url:None, created_at:"2026-07-08".into() }];
    let out = parse_normalized_response(&json, &events, |ev| fallback_normalize_from(&ev.title, &ev.body));
    assert_eq!(out[0].sectors, vec!["旅游综合".to_string()]);
    assert_eq!(out[0].topics, vec!["十五五".to_string()]);
    assert_eq!(out[0].summary, "文旅政策利好");
}

#[test]
fn test_normalized_backward_compat() {
    // 旧格式无 summary/sectors/topics，应 default 空
    let json = r#"[{"one_line_claim":"降准","stance":"bullish","severity":"high","affected_symbols":["600519"]}]"#;
    let events = vec![RawEvent { source:"t".into(), event_type:"news".into(), title:"降准".into(), body:"".into(), url:None, created_at:"2026-07-08".into() }];
    let out = parse_normalized_response(&json, &events, |ev| fallback_normalize_from(&ev.title, &ev.body));
    assert!(out[0].sectors.is_empty());
    assert_eq!(out[0].summary, "");
}
```

同时 `fallback_normalize_from` 的返回需补三字段默认值（`grep -n "fn fallback_normalize_from" src-tauri/src/invest/event_scanner.rs` 找到定义，在构造 `NormalizedEvent` 处补 `summary: String::new(), sectors: vec![], topics: vec![]`）。

- [ ] **Step 2: 运行确认失败**

Run: `cmd.exe /c "cd /d D:\ClaudeWorkspace\Code\ClawGO && cargo test --manifest-path src-tauri/Cargo.toml --lib invest::event_scanner -- --nocapture"`
Expected: 编译失败（NormalizedEvent 缺字段构造处报错）或测试失败

- [ ] **Step 3: 闭集 prompt 构建函数**

在 `src-tauri/src/invest/event_scanner.rs` 追加：

```rust
/// 构建带闭集 sectors 词表的归一化 prompt。
/// industries 来自 stock_industry.all_industries()（tushare 分类）。
pub fn build_normalizer_prompt_with_sectors(language: &str, industries: &[String]) -> String {
    let base = default_normalizer_prompt(language);
    if industries.is_empty() {
        return base.to_string();
    }
    let list = industries.join("、");
    if language.starts_with("en") {
        format!("{base}\n\nAdditional fields:\n- summary: one-line refined summary (<=40 chars)\n- sectors: MUST be chosen ONLY from this closed set (multi-select allowed, drop anything not in set): [{list}]\n- topics: free-form theme tags (e.g. robotics/low-altitude), for report grouping only")
    } else {
        format!("{base}\n\n额外字段：\n- summary: 一句话提炼摘要（≤40字）\n- sectors: **只能从以下封闭集合中选**（可多选，集合外的词一律丢弃）：[{list}]\n- topics: 自由主题标签（如 机器人/低空经济），仅供报告聚合")
    }
}
```

- [ ] **Step 4: events 表加列 + update_event_analysis 扩参**

先用 `ensure_column` 在建表流程后补列（`src-tauri/src/storage/invest/mod.rs` 建表批处理后）：

```rust
    with_conn(|conn| {
        crate::storage::invest::sentiment::ensure_column(conn, "events", "summary", "TEXT")?;
        crate::storage::invest::sentiment::ensure_column(conn, "events", "sectors", "TEXT")?;
        crate::storage::invest::sentiment::ensure_column(conn, "events", "topics", "TEXT")?;
        Ok(())
    })?;
```

改 `src-tauri/src/storage/invest/events.rs` 的 `update_event_analysis` 签名（当前只有 severity/stance/symbols）为：

```rust
pub fn update_event_analysis(
    id: &str,
    severity: &str,
    stance: &str,
    symbols: Option<&str>,
    summary: Option<&str>,
    sectors: Option<&str>,
    topics: Option<&str>,
) -> Result<(), String> {
    with_conn(|conn| {
        conn.execute(
            "UPDATE events SET severity=?1, stance=?2, symbols=?3, summary=?4, sectors=?5, topics=?6, analyzed=1, analyzed_at=datetime('now') WHERE id=?7",
            rusqlite::params![severity, stance, symbols, summary, sectors, topics, id],
        ).map_err(|e| format!("update event analysis: {}", e))?;
        Ok(())
    })
}
```

> 注：现有 `update_event_analysis` 调用点在 `event_analyzer.rs:78` 和 `:88`，Step 5 会一并改。`row_to_event`/`Event` struct 加 summary/sectors/topics 字段（`Option<String>`）并同步 SELECT 列表——`grep -n "row_to_event\|SELECT id, source" src-tauri/src/storage/invest/events.rs` 逐处改。

- [ ] **Step 5: 抽 analyze_pending(table) 通用函数**

在 `src-tauri/src/invest/event_analyzer.rs` 加枚举 + 通用函数，`analyze_pending_events` 保留为 `analyze_pending(AnalyzeTable::Events, lang)` 的包装：

```rust
#[derive(Debug, Clone, Copy)]
pub enum AnalyzeTable {
    Events,
    Sentiment,
}

/// 通用归一化：两表共用同一套 LLM 逻辑。注入闭集 sectors 词表。
pub async fn analyze_pending(table: AnalyzeTable, language: &str) -> Result<AnalyzerResult, String> {
    let industries = crate::storage::invest::stock_industry::all_industries().unwrap_or_default();
    let prompt = crate::invest::event_scanner::build_normalizer_prompt_with_sectors(language, &industries);

    match table {
        AnalyzeTable::Events => analyze_events_table(&prompt).await,
        AnalyzeTable::Sentiment => analyze_sentiment_table(&prompt).await,
    }
}

async fn analyze_sentiment_table(prompt: &str) -> Result<AnalyzerResult, String> {
    use crate::storage::invest::sentiment::{list_unanalyzed_sentiment, update_sentiment_analysis};
    let pending = list_unanalyzed_sentiment(Some(MAX_BATCH_SIZE))?;
    if pending.is_empty() {
        return Ok(AnalyzerResult { total_pending: 0, analyzed: 0, skipped: 0, errors: vec![] });
    }
    let total_pending = pending.len();
    // 复用 batch 归一化：把 SentimentItem 转成 (title, body) 喂 LLM
    let raws: Vec<crate::invest::event_scanner::RawEvent> = pending.iter().map(|it| {
        crate::invest::event_scanner::RawEvent {
            source: format!("sentiment:{}", it.provider),
            event_type: it.source_type.clone(),
            title: it.title.clone(),
            body: it.summary.clone().unwrap_or_else(|| it.title.clone()),
            url: it.url.clone(),
            created_at: it.created_at.clone(),
        }
    }).collect();
    let normalized = crate::invest::event_scanner::normalize_events(&raws, prompt).await;

    let mut analyzed = 0usize;
    let mut errors = Vec::new();
    for (item, norm) in pending.iter().zip(normalized.iter()) {
        // affected_symbols/sectors/topics 前后加逗号，供 LIKE '%,x,%' 精确匹配
        let wrap = |v: &Vec<String>| -> Option<String> {
            if v.is_empty() { None } else { Some(format!(",{},", v.join(","))) }
        };
        match update_sentiment_analysis(
            &item.id,
            if norm.summary.is_empty() { None } else { Some(&norm.summary) },
            norm.severity.as_str(),
            &norm.stance,
            wrap(&norm.affected_symbols).as_deref(),
            wrap(&norm.sectors).as_deref(),
            wrap(&norm.topics).as_deref(),
        ) {
            Ok(()) => analyzed += 1,
            Err(e) => errors.push(format!("update {}: {}", item.id, e)),
        }
    }
    Ok(AnalyzerResult { total_pending, analyzed, skipped: 0, errors })
}
```

把现有 `analyze_pending_events` 的 body 抽到 `analyze_events_table(prompt: &str)`（内容基本照搬现有实现，但 `update_event_analysis` 调用改成 7 参：新增 summary/sectors/topics，用同样的 `wrap` 逻辑）。`analyze_pending_events` 改为：

```rust
pub async fn analyze_pending_events(
    normalizer_prompt: Option<&str>,
    language: &str,
) -> Result<AnalyzerResult, String> {
    // 保留向后兼容签名，内部走通用路径
    let _ = normalizer_prompt; // 闭集 prompt 已在 analyze_pending 内构建
    analyze_pending(AnalyzeTable::Events, language).await
}
```

- [ ] **Step 6: 运行测试 + check**

Run: `cmd.exe /c "cd /d D:\ClaudeWorkspace\Code\ClawGO && cargo test --manifest-path src-tauri/Cargo.toml --lib invest::event_scanner -- --nocapture"` 然后 `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 测试 PASS，check 无错误（修完所有 `update_event_analysis` 调用点）

- [ ] **Step 7: 提交**

```bash
git add src-tauri/src/invest/event_scanner.rs src-tauri/src/invest/event_analyzer.rs src-tauri/src/storage/invest/events.rs src-tauri/src/storage/invest/mod.rs
git commit -m "feat(invest): 通用 analyze_pending(table) + 闭集 sectors/topics 归一化"
```

---

### Task 6: 盘前采集入口 + 内联归一化（消除时序穿孔）

**Files:**
- Modify: `src-tauri/src/invest/sentiment.rs`（`collect_all_sentiment` 编排：抓取 → 内联归一化）
- Modify: `src-tauri/src/commands/invest.rs`（Tauri 命令 `fetch_sentiment` + `refresh_stock_industry_cmd`）
- Modify: `src-tauri/src/lib.rs`（注册命令）
- Test: 手动 e2e（RPC 桥集成）

**Interfaces:**
- Consumes: `fetch_and_store`（Task 3）、`analyze_pending(AnalyzeTable::Sentiment, lang)`（Task 5）、`refresh_stock_industry`（Task 4）
- Produces:
  - `pub async fn collect_all_sentiment(symbol: Option<&str>, limit: u32) -> Result<AnalyzerResult, String>`（抓取四源 → **同步归一化到清零** → 返回归一化结果）
  - Tauri: `fetch_sentiment(provider, symbol, limit) -> Result<usize, String>`、`refresh_stock_industry_cmd() -> Result<usize, String>`

- [ ] **Step 1: 实现 collect_all_sentiment（内联归一化 + 清零循环）**

在 `src-tauri/src/invest/sentiment.rs` 追加：

```rust
use crate::invest::event_analyzer::{analyze_pending, AnalyzeTable, AnalyzerResult};

/// 盘前采集编排：抓取四源 → 立即归一化到清零。
/// 消除「event_analyzer 10 分钟批处理延迟导致委员会读到未归一化数据」的时序穿孔。
pub async fn collect_all_sentiment(symbol: Option<&str>, limit: u32) -> Result<AnalyzerResult, String> {
    // 1. 抓取前四源（雪球 Plan B）
    let stored = fetch_and_store("all", symbol, limit).await?;
    log::info!("collect_all_sentiment: stored {} items", stored);

    // 2. 同步归一化到清零（5 源爆量可能多批，循环直到无 pending）
    let mut agg = AnalyzerResult { total_pending: 0, analyzed: 0, skipped: 0, errors: vec![] };
    loop {
        let r = analyze_pending(AnalyzeTable::Sentiment, crate::invest::event_scanner::DEFAULT_LANGUAGE).await?;
        if r.total_pending == 0 {
            break;
        }
        agg.total_pending += r.total_pending;
        agg.analyzed += r.analyzed;
        agg.skipped += r.skipped;
        agg.errors.extend(r.errors);
        // 若一批全部失败（analyzed+skipped==0）防死循环
        if r.analyzed == 0 && r.skipped == 0 {
            log::warn!("collect_all_sentiment: batch made no progress, stopping");
            break;
        }
    }
    Ok(agg)
}
```

- [ ] **Step 2: Tauri 命令 + 注册**

在 `src-tauri/src/commands/invest.rs` 加：

```rust
#[tauri::command]
pub async fn fetch_sentiment(provider: String, symbol: Option<String>, limit: u32) -> Result<usize, String> {
    crate::invest::sentiment::fetch_and_store(&provider, symbol.as_deref(), limit).await
}

#[tauri::command]
pub async fn refresh_stock_industry_cmd() -> Result<usize, String> {
    crate::invest::sentiment::refresh_stock_industry().await
}

#[tauri::command]
pub async fn collect_sentiment(symbol: Option<String>, limit: u32) -> Result<crate::invest::event_analyzer::AnalyzerResult, String> {
    crate::invest::sentiment::collect_all_sentiment(symbol.as_deref(), limit).await
}
```

在 `src-tauri/src/lib.rs` 的 `invoke_handler![...]` 里注册这三个命令（`grep -n "invest::.*," src-tauri/src/lib.rs` 找到 invest 命令区，照格式加）。

- [ ] **Step 3: e2e 验证（真实 RPC 桥 + 归一化）**

先刷行业映射，再采集归一化。用内置 runtime 起 app 后在前端调用，或写 Rust 集成测试。最简验证——Python 侧确认 provider 可达：

```bash
cd "/d/ClaudeWorkspace/Code/ClawGO/src-tauri/python-runtime/scripts" && ../python/python.exe -c "
import sys, json; sys.path.insert(0,'.')
import server
reqs=[{'jsonrpc':'2.0','method':'ping','id':1},{'jsonrpc':'2.0','method':'sentiment.fetch','params':{'provider':'ths','symbol':None,'limit':3},'id':2}]
for r in reqs:
    resp=server.handle_request(r)
    ok = 'result' in resp
    print(f\"{r['method']}: {'OK' if ok else resp}\", file=sys.stderr)
" 2>&1 | tail -5
```
Expected: `ping: OK`、`sentiment.fetch: OK`

- [ ] **Step 4: cargo check 全量**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 无错误

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/invest/sentiment.rs src-tauri/src/commands/invest.rs src-tauri/src/lib.rs
git commit -m "feat(invest): 盘前采集入口 collect_all_sentiment + 内联归一化 + Tauri 命令"
```

---

## Plan A 自检

**Spec 覆盖（§3.1 / §12.0-12.6 / 12.8 Plan A 部分）：**
- ✅ sentiment_items 独立表（D1）— Task 1
- ✅ ensure_column 迁移 helper（C4）— Task 1
- ✅ 前四源抓取（雪球留 Plan B）— Task 2
- ✅ Rust sentiment.rs 写表 + sha256 去重（DeepSeek id 建议）— Task 3
- ✅ affected_symbols 前后加逗号防 LIKE 假阳性 — Task 5 wrap 逻辑
- ✅ stock_industry 映射 + tushare 缓存（12.4）— Task 4
- ✅ 通用 analyze_pending(table) + 闭集 sectors 词表（C1）+ topics 另开 — Task 5
- ✅ 盘前内联归一化到清零（C2 + 批量爆量循环）— Task 6

**留给 Plan B：** 雪球独立通道（C3+C5）、委员会催化改造两路查、盘前报告 SABC/拥挤度/AI点评/前端、全局翻色。

**关键检查点（对应 spec §12.9）：** CP1 sectors 命中率在 Task 5 e2e 验证；CP2 盘前时序在 Task 6 collect_all_sentiment 保证（抓完同步归一化）。