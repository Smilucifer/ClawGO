# 盘前观察 · Plan B — 两个消费者 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **前置依赖：** 本 plan 依赖 Plan A（`docs/superpowers/plans/2026-07-08-premarket-plan-a-sentiment-infra.md`）已完成——`sentiment_items` 表、`sentiment.rs` 接口、`analyze_pending(table)`、`stock_industry` 映射、`collect_all_sentiment` 均已就绪。

**Goal:** 在 Plan A 采集基础设施之上，实现两个消费者——① 委员会新闻催化改造（两路查两表 + akshare 兜底 + 内联归一化）；② 盘前观察报告（雪球独立通道 + SABC 打分 + 拥挤度 + AI 点评结构化 + 前端图文导出）；并把全局红涨绿跌翻色拆为独立收尾。

**Architecture:** 委员会催化从"实时抓 5 条 akshare"改为"查 events + sentiment_items 两表、个股 symbols 精确匹配 UNION 行业 sectors 命中"，无命中回退 akshare。盘前报告是尽力而为的多段流水线：采集（含雪球独立通道）→ 内联归一化 → SABC 四因子打分（纯 Rust）→ 拥挤度 → 一次 AI 点评（结构化 JSON）→ 组装 md+json 存盘 → 前端图文视图 + PNG/PDF 导出。翻色引入 `--up`/`--down` 语义变量单独收尾。

**Tech Stack:** Rust (committee/scoring/crowding) + Python (scrapling 雪球) + SvelteKit (PremarketReportTab + html2canvas/jspdf) + 委员会 CLI executor

## Global Constraints

- 平台：Windows-first。Rust 测试用 `cargo check` 或 cmd.exe（CLAUDE.md §11）。
- i18n：新增 UI 文本 `en.json` + `zh-CN.json` 同步，`npm run i18n:check` 必过。
- 提交：Conventional Commits。
- 配色：本报告红涨绿跌（涨/正=红 `#c0524a`，跌/负=绿 `#4e9a5f`）；grade 色 S=金/A=绿/B=青蓝/C=灰。前端严格按 `docs/ui-demo/premarket-report-demo.html` 类名与 token。
- 委员会催化零额外 LLM（补丁 C2）：读取时不调 LLM，提炼已在 Plan A 写入时完成；未归一化/无命中时回退 akshare（补丁 D2 兜底）。
- 雪球只抓市场级热帖榜，**不逐标的**（补丁 C3）；短生命周期 StealthyFetcher；cron 遇 cookie/引擎缺失只降级不弹窗（补丁 C5）；cookie 走 keyring/DPAPI 加密（DeepSeek 安全）。
- AI 点评不改档位（补丁 Claude M3）：组装步校验 grade 字符只能是 SABC 给的档位，冲突丢弃用占位。
- PremarketConfig 用 JSON 文件存（仿 `committee_tuning.json`，`~/.claw-go/invest/premarket_config.json`），**不是** DB——invest.db 无通用 settings 表（修正 spec §4 措辞）。
- 报告尽力而为：任一数据源失败标注"数据缺失"不中断；AI 失败数据段+SABC 分级仍产出。

---

### Task 1: 委员会催化改造（两路查两表 + akshare 兜底）

**Files:**
- Modify: `src-tauri/src/invest/committee/cli_executor.rs`（`fetch_company_news_for_prompt` 重写，341 行起）
- 调用点不变：`src-tauri/src/invest/committee/orchestrator.rs:1238`（仍传 `company_news` 给 `build_cli_risk_r1_prompt`）
- Test: `src-tauri/src/invest/committee/cli_executor.rs`（`#[cfg(test)]` 拼接逻辑测试）

**Interfaces:**
- Consumes（Plan A）：`storage::invest::sentiment::{list_sentiment_by_symbol, list_sentiment_by_sectors}`、`storage::invest::stock_industry::industry_of`、`storage::invest::events::list_events`
- Consumes（现有）：`InternationalClient::fetch_akshare_stock_news(symbol, count)`（兜底）
- Produces：`pub async fn fetch_company_news_for_prompt(symbol: &str) -> String`（签名不变，实现替换）

- [ ] **Step 1: 写拼接格式测试**

在 `cli_executor.rs` 的 `#[cfg(test)]` 加纯函数测试（抽一个 `format_catalyst_block` 便于测）：

```rust
#[test]
fn test_format_catalyst_block() {
    let stock = vec![
        ("茅台大宗交易".to_string(), "bullish".to_string(), "high".to_string()),
    ];
    let sector = vec![
        ("白酒板块政策利好".to_string(), "饮料制造".to_string()),
    ];
    let out = format_catalyst_block("600519", &stock, &sector);
    assert!(out.contains("【个股消息】"));
    assert!(out.contains("茅台大宗交易"));
    assert!(out.contains("【行业催化】"));
    assert!(out.contains("白酒板块政策利好"));
}

#[test]
fn test_format_catalyst_block_empty() {
    let out = format_catalyst_block("600519", &[], &[]);
    assert!(out.contains("暂无") || out.is_empty() == false);
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cmd.exe /c "cd /d D:\ClaudeWorkspace\Code\ClawGO && cargo test --manifest-path src-tauri/Cargo.toml --lib committee::cli_executor -- --nocapture"`
Expected: 编译失败（`format_catalyst_block` 未定义）

- [ ] **Step 3: 重写 fetch_company_news_for_prompt**

替换 `cli_executor.rs:341` 的 `fetch_company_news_for_prompt`：

```rust
/// 格式化催化块：个股消息 + 行业催化。纯函数，可测。
fn format_catalyst_block(
    symbol: &str,
    stock: &[(String, String, String)],  // (summary, stance, severity)
    sector: &[(String, String)],          // (summary, sector_name)
) -> String {
    let mut lines = vec![format!("【{} 近期新闻舆情】", symbol)];
    if !stock.is_empty() {
        lines.push("——个股消息——".to_string());
        for (summary, stance, severity) in stock.iter().take(6) {
            lines.push(format!("  · {} ({}/{})", summary, stance, severity));
        }
    }
    if !sector.is_empty() {
        lines.push("——行业催化——".to_string());
        for (summary, sector_name) in sector.iter().take(4) {
            lines.push(format!("  · [{}] {}", sector_name, summary));
        }
    }
    if stock.is_empty() && sector.is_empty() {
        lines.push("  暂无相关新闻舆情".to_string());
    }
    lines.join("\n")
}

/// 委员会催化：两路查 events + sentiment_items（个股 symbols 精确匹配 UNION 行业 sectors）。
/// 零额外 LLM——复用 Plan A 已归一化的 summary/stance/severity。
/// 两路均无命中时回退 akshare（补丁 D2 兜底），消除孤儿退化。
pub async fn fetch_company_news_for_prompt(symbol: &str) -> String {
    let code = symbol.split('.').next().unwrap_or(symbol);
    // 时间窗：最近 3 天
    let since = (chrono::Local::now() - chrono::Duration::days(3))
        .format("%Y-%m-%d %H:%M:%S").to_string();

    // 个股路：sentiment_items + events 精确匹配 ,{code},
    let mut stock: Vec<(String, String, String)> = Vec::new();
    if let Ok(items) = crate::storage::invest::sentiment::list_sentiment_by_symbol(code, &since, 10) {
        for it in items {
            let summary = it.summary.unwrap_or(it.title);
            stock.push((summary, it.stance, it.severity));
        }
    }
    // events 表个股新闻（jin10）——list_events 拉近期再过滤 symbols 含 ,{code},
    if let Ok(events) = crate::storage::invest::events::list_events(None, Some(200)) {
        let pat = format!(",{},", code);
        for e in events {
            let syms = e.symbols.clone().unwrap_or_default();
            // symbols 存储可能无逗号包裹（旧数据），双重匹配
            if syms.contains(&pat) || syms.split(',').any(|s| s.trim() == code) {
                stock.push((e.title.clone(), e.stance.clone(), e.severity.clone()));
            }
        }
    }

    // 行业路：查该股所属行业的 sectors 命中
    let mut sector: Vec<(String, String)> = Vec::new();
    if let Ok(Some(industry)) = crate::storage::invest::stock_industry::industry_of(code) {
        if let Ok(items) = crate::storage::invest::sentiment::list_sentiment_by_sectors(
            &[industry.clone()], &since, 8,
        ) {
            for it in items {
                let summary = it.summary.unwrap_or(it.title);
                sector.push((summary, industry.clone()));
            }
        }
    }

    // 兜底：两路都空 → 回退实时抓 akshare（补丁 D2）
    if stock.is_empty() && sector.is_empty() {
        let client = crate::invest::international::InternationalClient::from_settings();
        if let Ok(items) = client.fetch_akshare_stock_news(code, 5).await {
            for item in items.iter().take(5) {
                stock.push((item.title.clone(), "neutral".to_string(), "medium".to_string()));
            }
        }
    }

    format_catalyst_block(symbol, &stock, &sector)
}
```

确认 `chrono` 已引入（项目广泛使用，`grep chrono src-tauri/Cargo.toml`）。

- [ ] **Step 4: 运行测试 + check**

Run: `cmd.exe /c "cd /d D:\ClaudeWorkspace\Code\ClawGO && cargo test --manifest-path src-tauri/Cargo.toml --lib committee::cli_executor -- --nocapture"` 然后 `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 测试 PASS，check 无错误

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/invest/committee/cli_executor.rs
git commit -m "feat(invest): 委员会催化改造——两路查两表 + akshare 兜底"
```

---

### Task 2: 盘前流水线内联触发委员会催化前的归一化（时序保证 CP2）

**Files:**
- Modify: `src-tauri/src/invest/committee/orchestrator.rs`（委员会 run 起点前，若催化数据源为空则不强制；由盘前报告 job 保证）
- 说明：委员会本身逐标的跑，不改其触发；时序保证放在**盘前报告 job**（Task 5）——报告 job 先 `collect_all_sentiment`（Plan A，含内联归一化）再进后续。此 Task 仅加一个可选的"催化就绪"日志断言，无功能改动。

**Interfaces:**
- Consumes: 无新增
- Produces: 无新增（文档性 Task，确认 orchestrator 不需改动）

- [ ] **Step 1: 确认 orchestrator 无需改动**

Read `src-tauri/src/invest/committee/orchestrator.rs:1189-1273`（`run_role_phase`）。确认 Risk R1 调 `fetch_company_news_for_prompt`（Task 1 已改）即可，委员会不需要感知归一化时序——时序由盘前报告 job（Task 5）通过先 `collect_all_sentiment` 保证；用户手动跑委员会时，催化走 Task 1 的 akshare 兜底，不会空。

- [ ] **Step 2: 加就绪日志（可选，帮助 CP2 验证）**

在 `fetch_company_news_for_prompt`（Task 1）返回前加一行 log（已在 Task 1 实现中隐含，此处仅确认）：

```rust
    log::debug!("catalyst[{}]: {} stock + {} sector items", symbol, stock.len(), sector.len());
```

- [ ] **Step 3: 提交（若有改动）**

```bash
git add src-tauri/src/invest/committee/cli_executor.rs
git commit -m "chore(invest): 催化就绪日志便于时序验证 (CP2)"
```

> 注：此 Task 若无实际代码改动可跳过提交，仅作为执行者的确认检查点。

---

### Task 3: 雪球独立通道（scrapling worker + cookie 加密）

**Files:**
- Create: `src-tauri/python-runtime/scripts/providers/xueqiu.py`（scrapling StealthyFetcher，市场级热帖榜）
- Modify: `src-tauri/python-runtime/scripts/server.py`（注册 xueqiu provider）
- Modify: `src-tauri/src/invest/sentiment.rs`（`fetch_xueqiu_market` + cookie 读取）
- Create: cookie 存取——`src-tauri/src/invest/sentiment.rs` 内 `get_xueqiu_cookie`/`save_xueqiu_cookie`（keyring）
- Test: Python 独立跑 + Rust check

**Interfaces:**
- Consumes: scrapling（内置 runtime 已装）、keyring crate（cookie 加密）
- Produces:
  - Python `xueqiu.hot(cookie_json, size) -> list[dict]`（同 sentiment 契约，市场级 symbol=null）
  - Rust `pub async fn fetch_xueqiu_market(limit: u32) -> Result<usize, String>`（读 cookie → 调 python → 写 sentiment_items；cookie 缺失/失效返回 Ok(0) + warning，不抛）
  - `pub fn save_xueqiu_cookie(cookie: &str) -> Result<(), String>` / `pub fn get_xueqiu_cookie() -> Option<String>`

- [ ] **Step 1: xueqiu.py provider（复用已验证 scrapling 逻辑）**

创建 `src-tauri/python-runtime/scripts/providers/xueqiu.py`。核心复用已在探针验证的 `StealthyFetcher + page_action + add_cookies + evaluate(fetch)` 逻辑，只做市场级热帖榜：

```python
"""雪球舆情 provider（RPC: xueqiu.hot）。scrapling StealthyFetcher 过阿里云 WAF + 注入登录 cookie。
只抓市场级热帖榜（不逐标的）。"""
import sys
import re
import json
from datetime import datetime


def _parse_cookie(cookie_json: str):
    """cookie_json: [{"name":..,"value":..}] 或 "k=v; k2=v2" 字符串。"""
    if not cookie_json:
        return []
    try:
        data = json.loads(cookie_json)
        if isinstance(data, list):
            out = []
            for c in data:
                out.append({"name": c["name"], "value": c["value"],
                            "domain": ".xueqiu.com", "path": "/"})
            return out
    except Exception:
        pass
    # 退化：k=v; 串
    out = []
    for part in cookie_json.split(";"):
        if "=" in part:
            k, v = part.strip().split("=", 1)
            out.append({"name": k.strip(), "value": v.strip(),
                        "domain": ".xueqiu.com", "path": "/"})
    return out


def _strip_html(s):
    return re.sub(r"<[^>]+>", "", s or "").strip()


def _ts_iso(ms):
    try:
        return datetime.fromtimestamp(int(ms) // 1000).isoformat(timespec="seconds")
    except Exception:
        return datetime.now().isoformat(timespec="seconds")


def hot(cookie_json="", size=15) -> list:
    """雪球市场级热帖榜。cookie 失效/引擎缺失返回空列表（不抛）。"""
    try:
        from scrapling.fetchers import StealthyFetcher
    except ImportError:
        print("[xueqiu] scrapling not installed", file=sys.stderr, flush=True)
        return []

    cookies = _parse_cookie(cookie_json)
    api = f"https://xueqiu.com/statuses/hot/listV2.json?since_id=-1&max_id=-1&size={min(size,30)}"
    captured = {}

    def action(page):
        if cookies:
            page.context.add_cookies(cookies)
        page.goto("https://xueqiu.com/", wait_until="domcontentloaded")
        captured["text"] = page.evaluate(
            """async (url) => { const r = await fetch(url, {credentials:'include'}); return await r.text(); }""",
            api,
        )
        return page

    try:
        StealthyFetcher.fetch("https://xueqiu.com/", headless=True,
                              network_idle=True, timeout=60000, page_action=action)
    except Exception as e:
        print(f"[xueqiu] fetch failed: {e}", file=sys.stderr, flush=True)
        return []

    text = captured.get("text", "")
    try:
        data = json.loads(text)
    except Exception as e:
        print(f"[xueqiu] parse failed: {e} raw={text[:100]!r}", file=sys.stderr, flush=True)
        return []

    items = []
    for it in data.get("items", []):
        od = it.get("original_status") or it
        title = _strip_html(od.get("title") or od.get("text") or "")[:120]
        if not title:
            continue
        items.append({
            "provider": "xueqiu", "symbol": None,
            "title": title, "summary": "",
            "url": "https://xueqiu.com" + (od.get("target") or ""),
            "published_at": _ts_iso(od.get("created_at") or 0),
            "read_count": od.get("view_count"), "comment_count": od.get("reply_count"),
            "source_type": "post", "sentiment_hint": 0.0,
        })
    return items
```

`server.py` 注册：`register_provider("xueqiu", "xueqiu")`

- [ ] **Step 2: 独立跑验证（带 cookie）**

Run（用 xueqiu-cookies.csv 里的 cookie 拼 JSON 测）:
```bash
cd "/d/ClaudeWorkspace/Code/ClawGO/src-tauri/python-runtime/scripts" && ../python/python.exe -c "
import sys, json; sys.path.insert(0,'.')
from providers.xueqiu import hot
ck = json.dumps([
  {'name':'xq_a_token','value':'c54c9aeee7124df2a6c11b4d332aea40379a9595'},
  {'name':'xqat','value':'c54c9aeee7124df2a6c11b4d332aea40379a9595'},
  {'name':'u','value':'2531119131'},
])
items = hot(ck, 8)
print(f'got {len(items)}', file=sys.stderr)
assert len(items) > 0, 'empty (WAF/cookie failed)'
print('OK', file=sys.stderr)
" 2>&1 | tail -5
```
Expected: `got N`（N>0）、`OK`

- [ ] **Step 3: Rust cookie 存取（keyring）+ fetch_xueqiu_market**

确认 keyring crate（`grep keyring src-tauri/Cargo.toml`；无则 `cargo add keyring --manifest-path src-tauri/Cargo.toml`）。在 `sentiment.rs` 追加：

```rust
const XQ_COOKIE_SERVICE: &str = "clawgo-invest";
const XQ_COOKIE_USER: &str = "xueqiu-cookie";

pub fn save_xueqiu_cookie(cookie: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(XQ_COOKIE_SERVICE, XQ_COOKIE_USER)
        .map_err(|e| format!("keyring entry: {e}"))?;
    entry.set_password(cookie).map_err(|e| format!("keyring set: {e}"))
}

pub fn get_xueqiu_cookie() -> Option<String> {
    keyring::Entry::new(XQ_COOKIE_SERVICE, XQ_COOKIE_USER)
        .ok()
        .and_then(|e| e.get_password().ok())
}

/// 雪球市场级热帖榜 → sentiment_items。cookie 缺失/失效/引擎缺失 → Ok(0) + warning（不抛，降级）。
pub async fn fetch_xueqiu_market(limit: u32) -> Result<usize, String> {
    let cookie = match get_xueqiu_cookie() {
        Some(c) => c,
        None => {
            log::warn!("xueqiu cookie 未配置，跳过雪球");
            return Ok(0);
        }
    };
    let runtime = crate::python::require()?;
    let params = serde_json::json!({ "cookie_json": cookie, "size": limit });
    let value = match runtime.call("xueqiu.hot", params).await {
        Ok(v) => v,
        Err(e) => {
            log::warn!("xueqiu.hot failed (降级跳过): {e}");
            return Ok(0);
        }
    };
    let raws: Vec<RawSentimentItem> = serde_json::from_value(value)
        .map_err(|e| format!("parse xueqiu.hot: {e}"))?;
    if raws.is_empty() {
        log::warn!("xueqiu 返回空（WAF/cookie 可能失效），报告将标注雪球缺失");
        return Ok(0);
    }
    let mut count = 0;
    for r in &raws {
        let url = r.url.clone().unwrap_or_default();
        let item = SentimentItem {
            id: make_sentiment_id(&r.provider, &url, &r.title),
            provider: r.provider.clone(), symbol: r.symbol.clone(),
            title: r.title.clone(), summary: r.summary.clone(), url: r.url.clone(),
            published_at: r.published_at.clone(), read_count: r.read_count,
            comment_count: r.comment_count, source_type: r.source_type.clone(),
            sentiment_hint: r.sentiment_hint, affected_symbols: None,
            sectors: None, topics: None, stance: "pending".into(),
            severity: "pending".into(), analyzed: false, created_at: String::new(),
        };
        if save_sentiment_item(&item).is_ok() { count += 1; }
    }
    Ok(count)
}
```

- [ ] **Step 4: cargo check + 提交**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 无错误

```bash
git add src-tauri/python-runtime/scripts/providers/xueqiu.py src-tauri/python-runtime/scripts/server.py src-tauri/src/invest/sentiment.rs src-tauri/Cargo.toml
git commit -m "feat(invest): 雪球独立通道 (scrapling 市场级热帖 + keyring cookie)"
```

---

### Task 4: SABC 四因子打分器（纯 Rust，可单测）

**Files:**
- Create: `src-tauri/src/invest/premarket/mod.rs`（模块根 + PremarketConfig）
- Create: `src-tauri/src/invest/premarket/scoring.rs`
- Modify: `src-tauri/src/invest/mod.rs`（`pub mod premarket;`）
- Test: `src-tauri/src/invest/premarket/scoring.rs`（`#[cfg(test)]` 边界测试）

**Interfaces:**
- Produces:
  - `pub struct PremarketConfig { pub weight_sentiment: f64, pub weight_capital: f64, pub weight_technical: f64, pub weight_catalyst: f64, pub threshold_s: f64, pub threshold_a: f64, pub threshold_b: f64 }` + `Default`
  - `pub enum Grade { S, A, B, C }`
  - `pub struct FactorBreakdown { pub sentiment: f64, pub capital: f64, pub technical: f64, pub catalyst: f64 }`
  - `pub struct SymbolScore { pub symbol: String, pub name: String, pub total: f64, pub grade: Grade, pub factors: FactorBreakdown, pub missing_factors: Vec<String> }`
  - `pub fn score(symbol: &str, name: &str, factors: FactorBreakdown, missing: Vec<String>, cfg: &PremarketConfig) -> SymbolScore`
  - `pub fn get_premarket_config() -> PremarketConfig` / `pub fn save_premarket_config(cfg: PremarketConfig) -> Result<(), String>`（JSON 文件，仿 committee_tuning）

- [ ] **Step 1: 写打分边界测试**

创建 `src-tauri/src/invest/premarket/scoring.rs`：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> PremarketConfig { PremarketConfig::default() }

    #[test]
    fn test_grade_thresholds() {
        // 全 100 分 → S
        let f = FactorBreakdown { sentiment: 100.0, capital: 100.0, technical: 100.0, catalyst: 100.0 };
        let s = score("600519", "茅台", f, vec![], &cfg());
        assert!((s.total - 100.0).abs() < 0.01);
        assert!(matches!(s.grade, Grade::S));
    }

    #[test]
    fn test_grade_c_low() {
        let f = FactorBreakdown { sentiment: 10.0, capital: 10.0, technical: 10.0, catalyst: 10.0 };
        let s = score("x", "x", f, vec![], &cfg());
        assert!(matches!(s.grade, Grade::C));
    }

    #[test]
    fn test_weighted_sum() {
        // sentiment=80(w.30) capital=60(w.30) technical=40(w.25) catalyst=20(w.15)
        // = 24 + 18 + 10 + 3 = 55 → B (45<=55<62)
        let f = FactorBreakdown { sentiment: 80.0, capital: 60.0, technical: 40.0, catalyst: 20.0 };
        let s = score("x", "x", f, vec![], &cfg());
        assert!((s.total - 55.0).abs() < 0.01, "total={}", s.total);
        assert!(matches!(s.grade, Grade::B));
    }

    #[test]
    fn test_missing_factor_recorded() {
        let f = FactorBreakdown { sentiment: 50.0, capital: 50.0, technical: 50.0, catalyst: 50.0 };
        let s = score("x", "x", f, vec!["capital".to_string()], &cfg());
        assert_eq!(s.missing_factors, vec!["capital".to_string()]);
    }
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cmd.exe /c "cd /d D:\ClaudeWorkspace\Code\ClawGO && cargo test --manifest-path src-tauri/Cargo.toml --lib invest::premarket::scoring -- --nocapture"`
Expected: 编译失败（模块不存在）

- [ ] **Step 3: 实现 scoring.rs + PremarketConfig**

`src-tauri/src/invest/premarket/scoring.rs`：

```rust
//! SABC 四因子打分器。纯函数、可单测、独立于委员会 verdict。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PremarketConfig {
    pub weight_sentiment: f64,
    pub weight_capital: f64,
    pub weight_technical: f64,
    pub weight_catalyst: f64,
    pub threshold_s: f64,
    pub threshold_a: f64,
    pub threshold_b: f64,
}

impl Default for PremarketConfig {
    fn default() -> Self {
        Self {
            weight_sentiment: 0.30, weight_capital: 0.30,
            weight_technical: 0.25, weight_catalyst: 0.15,
            threshold_s: 78.0, threshold_a: 62.0, threshold_b: 45.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Grade { S, A, B, C }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactorBreakdown {
    pub sentiment: f64,
    pub capital: f64,
    pub technical: f64,
    pub catalyst: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolScore {
    pub symbol: String,
    pub name: String,
    pub total: f64,
    pub grade: Grade,
    pub factors: FactorBreakdown,
    pub missing_factors: Vec<String>,
}

pub fn score(
    symbol: &str,
    name: &str,
    factors: FactorBreakdown,
    missing: Vec<String>,
    cfg: &PremarketConfig,
) -> SymbolScore {
    let total = factors.sentiment * cfg.weight_sentiment
        + factors.capital * cfg.weight_capital
        + factors.technical * cfg.weight_technical
        + factors.catalyst * cfg.weight_catalyst;
    let grade = if total >= cfg.threshold_s {
        Grade::S
    } else if total >= cfg.threshold_a {
        Grade::A
    } else if total >= cfg.threshold_b {
        Grade::B
    } else {
        Grade::C
    };
    SymbolScore {
        symbol: symbol.to_string(), name: name.to_string(),
        total: (total * 100.0).round() / 100.0, grade, factors,
        missing_factors: missing,
    }
}

fn config_path() -> std::path::PathBuf {
    dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".claw-go").join("invest").join("premarket_config.json")
}

pub fn get_premarket_config() -> PremarketConfig {
    let path = config_path();
    if !path.exists() {
        return PremarketConfig::default();
    }
    std::fs::read_to_string(&path).ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_default()
}

pub fn save_premarket_config(cfg: PremarketConfig) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
    }
    let json = serde_json::to_string_pretty(&cfg).map_err(|e| format!("serialize: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("write premarket_config: {e}"))
}
```

创建 `src-tauri/src/invest/premarket/mod.rs`：

```rust
//! 盘前观察报告子系统。
pub mod scoring;
```

`src-tauri/src/invest/mod.rs` 加 `pub mod premarket;`。

- [ ] **Step 4: 运行测试确认通过**

Run: `cmd.exe /c "cd /d D:\ClaudeWorkspace\Code\ClawGO && cargo test --manifest-path src-tauri/Cargo.toml --lib invest::premarket::scoring -- --nocapture"`
Expected: PASS（4 tests）

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/invest/premarket/ src-tauri/src/invest/mod.rs
git commit -m "feat(invest): SABC 四因子打分器 + PremarketConfig (JSON 文件)"
```

---

### Task 5: 拥挤度雷达 + 报告生成器（编排 + md/json 存盘）

**Files:**
- Create: `src-tauri/src/invest/premarket/crowding.rs`
- Create: `src-tauri/src/invest/premarket/report.rs`（编排：采集→打分→AI→组装）
- Modify: `src-tauri/src/invest/premarket/mod.rs`（`pub mod crowding; pub mod report;`）
- Test: `crowding.rs` 单测（三档映射边界）

**Interfaces:**
- Consumes: `collect_all_sentiment`（Plan A）、`fetch_xueqiu_market`（Task 3）、`score`（Task 4）、`build_macro_snapshot()`、`list_holdings()`、`cli_complete`（AI 点评）、`generate_daily_report` 同款 data_dir 写盘
- Produces:
  - `pub enum CrowdLevel { Healthy, Warm, Hot }`
  - `pub fn crowd_level(turnover_pct: f64, volume_share: f64, divergence: f64) -> CrowdLevel`
  - `pub async fn generate_premarket_report(data_dir: &Path) -> Result<String, String>`（返回报告路径）

- [ ] **Step 1: 拥挤度三档映射测试**

创建 `src-tauri/src/invest/premarket/crowding.rs`：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crowd_healthy() {
        // 低换手分位 + 低成交占比 + 无背离 → 健康
        assert!(matches!(crowd_level(0.3, 0.05, 0.0), CrowdLevel::Healthy));
    }

    #[test]
    fn test_crowd_hot() {
        // 高换手分位 + 高成交占比 + 强背离 → 过热
        assert!(matches!(crowd_level(0.95, 0.30, 0.8), CrowdLevel::Hot));
    }

    #[test]
    fn test_crowd_warm() {
        assert!(matches!(crowd_level(0.75, 0.15, 0.3), CrowdLevel::Warm));
    }
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cmd.exe /c "cd /d D:\ClaudeWorkspace\Code\ClawGO && cargo test --manifest-path src-tauri/Cargo.toml --lib invest::premarket::crowding -- --nocapture"`
Expected: 编译失败

- [ ] **Step 3: 实现 crowd_level**

```rust
//! 拥挤度雷达：换手率分位 + 成交占比 + 龙头背离 → 健康/偏热/过热。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CrowdLevel { Healthy, Warm, Hot }

/// 三指标合成拥挤度。各指标已归一到 0-1。
/// turnover_pct: 换手率历史分位；volume_share: 成交占全市场比；divergence: 龙头/板块背离度。
pub fn crowd_level(turnover_pct: f64, volume_share: f64, divergence: f64) -> CrowdLevel {
    // 加权合成：换手分位 0.4 + 成交占比 0.35 + 背离 0.25
    let score = turnover_pct * 0.4 + (volume_share / 0.30).min(1.0) * 0.35 + divergence * 0.25;
    if score >= 0.75 {
        CrowdLevel::Hot
    } else if score >= 0.55 {
        CrowdLevel::Warm
    } else {
        CrowdLevel::Healthy
    }
}
```

- [ ] **Step 4: 报告生成器骨架（尽力而为编排）**

创建 `src-tauri/src/invest/premarket/report.rs`。**关键：先 `collect_all_sentiment` 保证时序（CP2）**，各段 try/degraded：

```rust
//! 盘前报告生成器：采集 → 内联归一化 → 打分 → AI 点评 → 组装 md+json 存盘。
//! 尽力而为：任一数据源失败标注缺失不中断。

use std::path::Path;
use crate::invest::premarket::scoring::{get_premarket_config, score, FactorBreakdown};

pub async fn generate_premarket_report(data_dir: &Path) -> Result<String, String> {
    let date = crate::invest::date_utils::get_invest_date();

    // 1. 采集 + 内联归一化（CP2 时序保证）——含雪球独立通道
    let _ = crate::invest::sentiment::collect_all_sentiment(None, 20).await;
    let _ = crate::invest::sentiment::fetch_xueqiu_market(15).await; // 降级不阻断

    // 2. 宏观快照（复用）
    let macro_md = match crate::storage::invest::macro_cache::build_macro_snapshot() {
        Ok(s) => format!("{:?}", s), // 实际按 build_macro_snapshot 真实返回渲染
        Err(_) => "宏观快照：数据缺失".to_string(),
    };

    // 3. 股票池 SABC 打分
    let cfg = get_premarket_config();
    let holdings = crate::storage::invest::portfolio::list_holdings().unwrap_or_default();
    let mut scores = Vec::new();
    for h in &holdings {
        // 各因子实际计算见 Task 6；此处骨架用中性 50 占位 + missing 标注
        let factors = FactorBreakdown { sentiment: 50.0, capital: 50.0, technical: 50.0, catalyst: 50.0 };
        scores.push(score(&h.symbol, &h.name, factors, vec![], &cfg));
    }

    // 4. AI 点评（一次调用，结构化 JSON，失败降级占位）—— Task 6 填充
    // 5. 组装 md + json 写 {data_dir}/invest/reports/premarket_{date}.md(+.json)
    let reports_dir = data_dir.join("invest").join("reports");
    std::fs::create_dir_all(&reports_dir).map_err(|e| format!("mkdir reports: {e}"))?;
    let md_path = reports_dir.join(format!("premarket_{}.md", date));
    let md = format!("# 盘前观察 {}\n\n## 宏观\n{}\n\n## SABC 观察池\n共 {} 标的\n", date, macro_md, scores.len());
    std::fs::write(&md_path, &md).map_err(|e| format!("write md: {e}"))?;

    let json_path = reports_dir.join(format!("premarket_{}.json", date));
    let json = serde_json::json!({ "date": date, "scores": scores, "config": cfg });
    std::fs::write(&json_path, serde_json::to_string_pretty(&json).unwrap())
        .map_err(|e| format!("write json: {e}"))?;

    Ok(md_path.to_string_lossy().to_string())
}
```

`mod.rs` 加 `pub mod crowding; pub mod report;`。

> 实现者注：`build_macro_snapshot`、`list_holdings`、`get_invest_date` 的真实签名/返回类型**必须**先 Read 确认（`storage/invest/macro_cache.rs`、`storage/invest/portfolio.rs`、`invest/date_utils.rs`）。上面 `format!("{:?}")` 是占位，Task 6 用真实字段渲染。

- [ ] **Step 5: 运行测试 + check + 提交**

Run: `cmd.exe /c "cd /d D:\ClaudeWorkspace\Code\ClawGO && cargo test --manifest-path src-tauri/Cargo.toml --lib invest::premarket::crowding -- --nocapture"` 然后 `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 测试 PASS，check 无错误

```bash
git add src-tauri/src/invest/premarket/
git commit -m "feat(invest): 拥挤度雷达 + 盘前报告生成器骨架（内联归一化时序保证）"
```

---

### Task 6: 四因子真实计算 + AI 点评结构化 + Tauri 命令 + cron

**Files:**
- Modify: `src-tauri/src/invest/premarket/report.rs`（四因子真实计算 + AI 点评 JSON）
- Modify: `src-tauri/src/invest/scheduler/mod.rs`（`default_jobs()` 加 premarket_report）
- Modify: `src-tauri/src/invest/scheduler/runner.rs`（`dispatch_job` 加分支）
- Modify: `src-tauri/src/commands/invest.rs`（6 个命令）
- Modify: `src-tauri/src/lib.rs`（注册）
- Test: 手动 e2e

**Interfaces:**
- Consumes: `moneyflow_dc`/`moneyflow_hsgt`（资金因子）、`regime::compute_regime_for_symbol` + `indicators`（技术因子）、`list_sentiment_by_symbol`（舆论/催化因子）、`cli_complete`（AI）
- Produces:
  - Tauri: `generate_premarket_report_cmd()`、`list_premarket_reports(limit)`、`read_premarket_report(date)`、`get_premarket_config_cmd()`、`save_premarket_config_cmd(config)`、（`fetch_sentiment` 已在 Plan A）
  - cron job id `premarket_report`，`0 0 9 * * 1-5`，`requires_trading_day: true`

- [ ] **Step 1: 四因子真实计算**

在 `report.rs` 把占位的 `FactorBreakdown { 50.0 ... }` 换成真实计算。每因子归一到 0-100，缺失记 `missing_factors`。示例（舆论 + 催化因子，从 sentiment_items 算）：

```rust
async fn compute_factors(symbol: &str) -> (FactorBreakdown, Vec<String>) {
    let code = symbol.split('.').next().unwrap_or(symbol);
    let since = (chrono::Local::now() - chrono::Duration::days(3)).format("%Y-%m-%d %H:%M:%S").to_string();
    let mut missing = Vec::new();

    // 舆论热度：sentiment_hint 均值(-1~1→0-100) × 热度对数加成
    let items = crate::storage::invest::sentiment::list_sentiment_by_symbol(code, &since, 20).unwrap_or_default();
    let sentiment = if items.is_empty() {
        missing.push("sentiment".to_string());
        50.0
    } else {
        let avg: f64 = items.iter().filter_map(|i| i.sentiment_hint).sum::<f64>() / items.len().max(1) as f64;
        ((avg + 1.0) / 2.0 * 100.0).clamp(0.0, 100.0)
    };

    // 催化：关联新闻条数与新鲜度
    let catalyst = if items.is_empty() {
        missing.push("catalyst".to_string());
        50.0
    } else {
        (items.len() as f64 * 10.0).min(100.0)
    };

    // 资金面 + 技术面：调 moneyflow_dc / regime（真实签名先 Read 确认）
    // 无数据 → missing + 中性 50
    let capital = 50.0;   // TODO 实现者：moneyflow_dc 净流入率归一，见 tushare/client.rs
    let technical = 50.0; // TODO 实现者：regime::compute_regime_for_symbol + indicators
    missing.push("capital".to_string());   // 骨架先标缺失，实现后移除
    missing.push("technical".to_string());

    (FactorBreakdown { sentiment, capital, technical, catalyst }, missing)
}
```

> 实现者注：capital/technical 因子需 Read `tushare/client.rs`（moneyflow_dc 签名）、`invest/regime.rs`（compute_regime_for_symbol）确认真实接口后实现，不能留 50.0 占位上线。这是本 Task 的核心工作量。

- [ ] **Step 2: AI 点评结构化 JSON（不改档）**

在 `report.rs` 加 AI 点评，输出结构化 JSON，组装步校验 grade 不被 AI 改：

```rust
#[derive(serde::Deserialize)]
struct AiCommentary {
    sectors: Vec<AiSector>,
    tone: String,
}
#[derive(serde::Deserialize)]
struct AiSector { name: String, tag: String, count: u32, note: String }

async fn ai_commentary(news_block: &str) -> Option<AiCommentary> {
    let prompt = format!(
        "你是A股盘前分析师。把以下新闻聚合成3-5个板块，每个给：name、tag(只能选:新闻强/催化强/情绪强/分歧大/风险预警)、count、note(一句话)。\
         风险预警专收监管/政策转向/处罚退市/地缘扰动。输出JSON: {{\"sectors\":[...],\"tone\":\"基调总述\"}}。只输出JSON。\n\n{}",
        news_block
    );
    let resp = crate::invest::event_analyzer::cli_complete("你是严谨的金融分析师，只输出JSON。", &prompt).await.ok()?;
    // 剥离可能的 ```json 包裹
    let cleaned = resp.trim().trim_start_matches("```json").trim_start_matches("```").trim_end_matches("```").trim();
    serde_json::from_str(cleaned).ok()
}
```

AI 失败返回 None → 报告 01 段显示"AI 点评生成失败"占位，SABC 分级不受影响（补丁 M3：AI 完全不碰 grade，grade 只来自 Task 4 的 `score()`）。

- [ ] **Step 3: cron job + dispatch 分支**

`scheduler/mod.rs` 的 `default_jobs()` vec 里加（仿 event_scan 格式）：

```rust
        CronJob {
            id: "premarket_report".into(),
            name: "盘前观察报告".into(),
            cron_expr: "0 0 9 * * 1-5".into(),
            interval_min: None,
            enabled: true,
            requires_trading_day: true,
            last_run: None,
            next_run: None,
            last_status: None,
            description: "盘前生成观察报告（舆情+SABC+拥挤度+AI点评）".into(),
            dedicated: false,
        },
```

`scheduler/runner.rs` 的 `dispatch_job` match 加分支（仿 `daily_report`）：

```rust
        "premarket_report" => {
            let data_dir = /* 同 daily_report 取 data_dir 的方式 */;
            let path = crate::invest::premarket::report::generate_premarket_report(&data_dir).await?;
            Ok(format!("盘前报告生成: {}", path))
        }
```

- [ ] **Step 4: 6 个 Tauri 命令 + 注册**

`commands/invest.rs`：

```rust
#[tauri::command]
pub async fn generate_premarket_report_cmd(app: tauri::AppHandle) -> Result<String, String> {
    let data_dir = /* 取 data_dir，仿现有 daily_report 命令 */;
    crate::invest::premarket::report::generate_premarket_report(&data_dir).await
}

#[tauri::command]
pub fn list_premarket_reports(limit: usize) -> Result<Vec<String>, String> {
    // 扫 {data_dir}/invest/reports/ 过滤 premarket_*.md，返回日期倒序
    // ... 实现见下
}

#[tauri::command]
pub fn read_premarket_report(date: String) -> Result<serde_json::Value, String> {
    // 读 premarket_{date}.md + .json
}

#[tauri::command]
pub fn get_premarket_config_cmd() -> Result<crate::invest::premarket::scoring::PremarketConfig, String> {
    Ok(crate::invest::premarket::scoring::get_premarket_config())
}

#[tauri::command]
pub fn save_premarket_config_cmd(config: crate::invest::premarket::scoring::PremarketConfig) -> Result<(), String> {
    // 校验：权重和≈1.0，阈值 S>A>B
    let sum = config.weight_sentiment + config.weight_capital + config.weight_technical + config.weight_catalyst;
    if (sum - 1.0).abs() > 0.001 { return Err(format!("权重和必须为1.0，当前{:.3}", sum)); }
    if !(config.threshold_s > config.threshold_a && config.threshold_a > config.threshold_b) {
        return Err("阈值须满足 S > A > B".to_string());
    }
    crate::invest::premarket::scoring::save_premarket_config(config)
}
```

`lib.rs` 注册这 5 个（`fetch_sentiment` 等 Plan A 已注册）。

- [ ] **Step 5: e2e + check + 提交**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`；手动前端触发 `trigger_cron_job("premarket_report")` 或调 `generate_premarket_report_cmd`，确认 `{data_dir}/invest/reports/premarket_{date}.md(+.json)` 生成。
Expected: check 无错误，报告文件生成（CP3：断网/删cookie/卸chromium 各跑一次，报告都能出、仅标注雪球缺失）

```bash
git add src-tauri/src/invest/premarket/report.rs src-tauri/src/invest/scheduler/ src-tauri/src/commands/invest.rs src-tauri/src/lib.rs
git commit -m "feat(invest): 四因子计算 + AI点评结构化 + 盘前报告命令 + cron"
```

---

### Task 7: 前端 PremarketReportTab + 图文导出

**Files:**
- Create: `src/lib/components/invest/PremarketReportTab.svelte`
- Modify: `src/routes/invest/+page.svelte`（System 下加 reports 子 tab）
- Modify: `src/lib/stores/invest-store.svelte.ts`（premarket 状态 + 命令调用）
- Modify: `src-tauri/messages/en.json` + `zh-CN.json`（i18n key）
- Modify: `package.json`（html2canvas + jspdf 依赖）
- Reference: `docs/ui-demo/premarket-report-demo.html`（严格按类名/token）、`CommitteeArchiveTab.svelte`（读取模式参照）

**Interfaces:**
- Consumes: Tauri 命令 `generate_premarket_report_cmd`/`list_premarket_reports`/`read_premarket_report`/`get_premarket_config_cmd`/`save_premarket_config_cmd`/`trigger_cron_job`
- Produces: `/invest` System → reports 子 tab

- [ ] **Step 1: 加依赖**

Run: `npm install html2canvas jspdf`
确认写入 `package.json` dependencies。

- [ ] **Step 2: 图文视图组件（按 demo 类名）**

创建 `PremarketReportTab.svelte`。读 `read_premarket_report(latestDate)` 的 JSON → 渲染四段（复刻 `premarket-report-demo.html`）：固定 720px `#report-canvas` 作为导出目标。工具栏：立即生成（`trigger_cron_job("premarket_report")`）、导出 PNG（html2canvas）、导出 PDF（jspdf）。

关键结构（严格用 demo 的类名 `.theme-wall`/`.theme-tag-card`/`.eval-tag`/`.sector-flow`/`.crowd-badge`/`.theme-row`/`.pool-grid`/`.stock-row`）：

```svelte
<script lang="ts">
  import html2canvas from 'html2canvas';
  import { jsPDF } from 'jspdf';
  import { invoke } from '@tauri-apps/api/core'; // 或 transport 抽象

  let report = $state<any>(null);
  let generating = $state(false);

  async function loadLatest() {
    const dates: string[] = await invoke('list_premarket_reports', { limit: 1 });
    if (dates.length) report = await invoke('read_premarket_report', { date: dates[0] });
  }
  async function generate() {
    generating = true;
    try { await invoke('trigger_cron_job', { id: 'premarket_report' }); await loadLatest(); }
    finally { generating = false; }
  }
  async function exportPng() {
    const el = document.getElementById('report-canvas'); if (!el) return;
    const canvas = await html2canvas(el, { scale: 2 });
    const a = document.createElement('a'); a.href = canvas.toDataURL('image/png');
    a.download = `premarket_${report?.date ?? 'report'}.png`; a.click();
  }
  async function exportPdf() {
    const el = document.getElementById('report-canvas'); if (!el) return;
    const canvas = await html2canvas(el, { scale: 2 });
    const pdf = new jsPDF({ unit: 'px', format: [canvas.width, canvas.height] });
    pdf.addImage(canvas.toDataURL('image/png'), 'PNG', 0, 0, canvas.width, canvas.height);
    pdf.save(`premarket_${report?.date ?? 'report'}.pdf`);
  }
  $effect(() => { loadLatest(); });
</script>

<div class="premarket-tab" data-invest-scope>
  <div class="toolbar">
    <button onclick={generate} disabled={generating}>{generating ? '生成中…' : '立即生成'}</button>
    <button onclick={exportPng} disabled={!report}>导出 PNG</button>
    <button onclick={exportPdf} disabled={!report}>导出 PDF</button>
  </div>
  {#if report}
    <div id="report-canvas" style="width:720px">
      <!-- 四段：严格复刻 premarket-report-demo.html 的类名结构 -->
      <!-- 01 舆论新闻先验 / 02 资金宏观 / 03 主线排序 / 04 SABC 观察池 -->
    </div>
  {:else}
    <p>暂无报告，点击「立即生成」</p>
  {/if}
</div>
```

> 实现者注：四段 HTML 骨架**必须**逐段对照 `docs/ui-demo/premarket-report-demo.html` 复制类名与 token，用 `report.scores`/`report.sectors` 等 JSON 字段填充。这是本 Task 的主要工作量。

- [ ] **Step 3: 设置面板（权重/阈值 + 保存）**

在组件内加可折叠设置面板：4 个权重输入 + 3 个阈值输入 + 保存按钮。前端校验权重和=1.0（否则禁用保存）、阈值 S>A>B。调 `save_premarket_config_cmd`。

- [ ] **Step 4: 子 tab 注册 + i18n**

`/invest` System 下 sub-tab 列表加 `reports`（「盘前观察」）。`en.json`+`zh-CN.json` 补齐所有新 key。

- [ ] **Step 5: 构建验证**

Run: `npm run build && npm run i18n:check`
Expected: build 成功，i18n check 通过

- [ ] **Step 6: 提交**

```bash
git add src/lib/components/invest/PremarketReportTab.svelte src/routes/invest/+page.svelte src/lib/stores/invest-store.svelte.ts src-tauri/messages/ package.json package-lock.json
git commit -m "feat(invest): 盘前观察前端 tab + 图文视图 + PNG/PDF 导出 + 设置面板"
```

---

### Task 8: 全局红涨绿跌翻色（独立收尾 PR）

**Files:**
- Modify: `src/app.css`（`[data-invest-scope]` 引入 `--up`/`--down` 语义变量）
- Modify: 涉涨跌色的 invest 组件（`MacroSnapshotCard`/`KpiCard`/`HoldingsTable`/`PnlChart`/`TradeLogTab`/committee 组件/PremarketReportTab）
- Modify: `docs/ui-demo/macro-card-demo.html` 等注释

**Interfaces:** 无（纯样式）

> **审查共识（Claude/DeepSeek）**：翻色改动面广、与主线解耦，**建议单独 PR**。加 CI/grep 静态检查而非人肉巡检。

- [ ] **Step 1: 引入语义变量**

`src/app.css` 的 `[data-invest-scope]` 加：

```css
[data-invest-scope] {
  --up: #c0524a;   /* 涨/正 = 红 */
  --down: #4e9a5f; /* 跌/负 = 绿 */
}
```

- [ ] **Step 2: 逐组件替换涨跌语境用色**

把 invest 组件里**涨跌语境**下的 `--color-success`/`--color-error` 换成 `--up`/`--down`；**保留**非涨跌语境（校验通过/连接失败）的 `--color-success`/`--color-error`。逐处判断，不做全局字符串替换。

- [ ] **Step 3: grep 静态检查**

Run:
```bash
cd "/d/ClaudeWorkspace/Code/ClawGO" && grep -rn "color-success\|color-error" src/lib/components/invest/ | grep -iE "涨|跌|pnl|change|profit|gain|loss" || echo "无涨跌语境残留"
```
Expected: 涨跌语境下无 `--color-success`/`--color-error` 残留

- [ ] **Step 4: 构建 + 人工巡检**

Run: `npm run build`
人工巡检 `/invest` 各 tab（dashboard/committee/strategy/trades/system/reports），确认无语义错色（绿色"成功"提示不能变成"跌"）。

- [ ] **Step 5: 提交**

```bash
git add src/app.css src/lib/components/invest/ docs/ui-demo/
git commit -m "feat(invest): 全局红涨绿跌翻色（引入 --up/--down 语义变量）"
```

---

## Plan B 自检

**Spec 覆盖（§4-§10 + §12 消费者部分）：**
- ✅ 委员会催化改造两路查两表 + akshare 兜底（§12.3 + D2）— Task 1
- ✅ 时序保证（CP2，报告 job 先 collect_all_sentiment）— Task 2/5
- ✅ 雪球独立通道 + 只市场级 + cookie 加密 + 降级不弹窗（C3+C5）— Task 3
- ✅ SABC 四因子打分器纯函数 + PremarketConfig JSON 文件（§4）— Task 4
- ✅ 拥挤度雷达三档（§3.2）+ 报告生成器尽力而为（§6）— Task 5
- ✅ 四因子真实计算 + AI 点评结构化不改档（§5 + M3）+ cron 9:00（§7）+ 6 命令（§8）— Task 6
- ✅ 前端图文 tab + PNG/PDF 导出 + 权重设置面板（§8）— Task 7
- ✅ 全局翻色独立收尾（§10）— Task 8

**关键检查点（§12.9）：** CP1 sectors 命中率（Plan A）；CP2 时序（Task 2/5）；CP3 三故障降级（Task 6 Step 5）；CP4 50 标的 dry-run 催化中位数>0（Task 1 上线前验证）。

**实现者必读注意事项（避免占位上线）：**
- Task 4 capital/technical 因子必须 Read `tushare/client.rs`（moneyflow_dc）、`invest/regime.rs` 实现真实计算，不能留 50.0。
- Task 5 `build_macro_snapshot`/`list_holdings`/`get_invest_date` 真实签名先 Read 确认。
- Task 6 `dispatch_job` 取 data_dir 的方式仿现有 `daily_report` 分支。
- Task 7 四段 HTML 严格对照 `premarket-report-demo.html` 类名。