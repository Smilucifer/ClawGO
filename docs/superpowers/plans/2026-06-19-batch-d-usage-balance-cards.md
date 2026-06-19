# 批次 D — /usage 余额卡片(PackyAPI + cookie 自动续期) 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 `/usage` 余额卡片新增 PackyAPI 余额面板(任务9 前半),并用 reqwest cookie jar 让小米与 PackyAPI 的 cookie 自动续期,减少手动重取(任务9 后半)。

**Architecture:** 后端 `commands/balance.rs` 新增 `query_packyapi_balance` 并把 source 枚举扩到 `packyapi`;`BalanceHelperSettings` 加 PackyAPI 三字段(session / TDC_itoken / user_id);用显式 `Arc<reqwest::cookie::Jar>` 构造 client,请求后从 jar 回读续期后的 cookie 写盘。前端在 DeepSeek 面板右侧加 PackyAPI 面板,网格改三列,小米独占一行。

**Tech Stack:** Rust(reqwest + cookie feature、serde)、Svelte 5、i18n。

## Global Constraints

- 关联 spec:`docs/superpowers/specs/2026-06-19-multi-module-maintenance-design.md`(批次 D 节)。
- 本机 Rust 单测有 §11 运行时问题 → 编译验证 `cargo check`;纯函数(格式化、quota 换算)写单测,沿用 balance.rs 既有 `#[cfg(test)]` 风格。
- **PackyAPI 接口(已实测确认):** `GET https://www.packyapi.com/api/user/self`,头 `Cookie: session=<...>; TDC_itoken=<...>` + `New-Api-User: <user_id>`;响应 `data.quota`(剩余)、`data.used_quota`(已用),单位 **500000 = $1**;`data.display_name` 可选展示。
- cookie 续期靠服务端 `Set-Cookie`;用显式 `Arc<Jar>` 才能回读(`cookie_store(true)` 的内部 store 不可读取)。需确认 reqwest 启用 `cookies` feature(Step 检查 Cargo.toml)。
- 凭据脱敏:错误信息经 `redacted_operational_error`(balance.rs:42)处理;PackyAPI 的 session/token 同样不得明文进 error。
- 任何 UI 文案同步 en.json + zh-CN.json,过 `npm run i18n:check`。
- Conventional Commits。

## 关键事实(已核对)

- `refresh_balance_status_inner`(balance.rs:343)source 校验 `:347`(当前 `all|deepseek|mimo`);reqwest client 构造 `:353-356`;cache 写入 `helper.cache.insert(...)` `:366, 389`。
- `query_deepseek_balance(client, api_key, base_url)`(`:103`)是最简模板:GET + 鉴权 + JSON 取字段。
- `balance_cache_entry(source, Result<String,String>)`(`:10`)写普通余额;`balance_cache_entry_with_tokens`(`:14`)写带 token plan 的。
- `BalanceHelperSettings`(models.rs:493-519,实现时 Read 确认):含 `mimo_service_token/mimo_user_id/mimo_slh/mimo_ph/auto_refresh_secs/cache`。
- `apply_balance_helper`(settings.rs:702-739):增量 patch,每字段 trim 后空→None。新字段按此模式加。
- 前端 Balance Card:`routes/usage/+page.svelte:486-685`,DeepSeek 面板 `:516-557`(`md:grid-cols-2`),小米面板 `:559-682`(`md:col-span-2`);`balanceStatusText(source)` `:46-84`;`refreshBalanceStatus` 枚举 `api.ts:480-485`。

---

## Task 1: BalanceHelperSettings 加 PackyAPI 字段

**Files:**
- Modify: `src-tauri/src/models.rs:493-519`(`BalanceHelperSettings`)
- Modify: `src-tauri/src/storage/settings.rs:702-739`(`apply_balance_helper`)

**Interfaces:**
- Produces: `BalanceHelperSettings` 新增 `packyapi_session: Option<String>`、`packyapi_itoken: Option<String>`、`packyapi_user_id: Option<String>`。

- [ ] **Step 1: Read 结构体确认现状**

Read `src-tauri/src/models.rs:492-519`,确认 `BalanceHelperSettings` 字段。**已核实:** derive 是 `#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]`,**没有 `Default` derive**——`Default` 是 `:508-519` 的**手写 `impl Default`**,逐字段列出。结构体**未用** `#[serde(rename_all = "camelCase")]`,故序列化为 snake_case,前端按 snake 读(mimo 字段即 `mimo_service_token` 等 snake)。

- [ ] **Step 2: 加三字段(结构体定义 + 手写 Default 同步)**

(a) 在 `BalanceHelperSettings` 结构体的 mimo 字段之后、`auto_refresh_secs` 之前插入:

```rust
    #[serde(default)]
    pub packyapi_session: Option<String>,
    #[serde(default)]
    pub packyapi_itoken: Option<String>,
    #[serde(default)]
    pub packyapi_user_id: Option<String>,
```

(b) **【必须同步】** 在 `impl Default for BalanceHelperSettings`(`models.rs:508-519`)的字段初始化里补三行,否则 `cargo check` 报 `missing fields in initializer`:

```rust
            packyapi_session: None,
            packyapi_itoken: None,
            packyapi_user_id: None,
```

(命名沿用 mimo 的 snake_case,前端按 snake 读。)

- [ ] **Step 3: apply_balance_helper 处理新字段**

在 `settings.rs:723`(`mimo_ph` 处理之后)插入:

```rust
    if let Some(s) = v.get("packyapi_session") {
        next.packyapi_session = s.as_str().map(str::trim).filter(|s| !s.is_empty()).map(|s| s.to_string());
    }
    if let Some(s) = v.get("packyapi_itoken") {
        next.packyapi_itoken = s.as_str().map(str::trim).filter(|s| !s.is_empty()).map(|s| s.to_string());
    }
    if let Some(s) = v.get("packyapi_user_id") {
        next.packyapi_user_id = s.as_str().map(str::trim).filter(|s| !s.is_empty()).map(|s| s.to_string());
    }
```

- [ ] **Step 4: 编译验证**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 通过。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/models.rs src-tauri/src/storage/settings.rs
git commit -m "feat(balance): BalanceHelperSettings 新增 PackyAPI 凭据字段"
```

---

## Task 2: PackyAPI 余额查询 + quota 格式化

**Files:**
- Modify: `src-tauri/src/commands/balance.rs`(新增常量、`format_packyapi_balance`、`query_packyapi_balance`、单测)

**Interfaces:**
- Consumes: `BalanceHelperSettings.packyapi_*`(Task 1)。
- Produces:
  - `format_packyapi_balance(body: &Value) -> Result<String, String>` — 从 `data.quota`/`data.used_quota` 格式化为 `"$105.64 剩 / $795.36 用"`。
  - `query_packyapi_balance(client, session, itoken, user_id) -> Result<String, String>`。

- [ ] **Step 1: 写 quota 格式化单测**

在 balance.rs 测试模块(`:408`)加:

```rust
    #[test]
    fn formats_packyapi_quota() {
        let body = serde_json::json!({
            "success": true,
            "data": { "quota": 52819275, "used_quota": 397680725, "display_name": "Tester" }
        });
        let s = format_packyapi_balance(&body).unwrap();
        assert!(s.contains("$105.6"), "got: {s}");   // 52819275 / 500000
        assert!(s.contains("$795.3"), "got: {s}");   // 397680725 / 500000
    }

    #[test]
    fn rejects_packyapi_missing_data() {
        let body = serde_json::json!({ "success": false, "message": "unauthorized" });
        assert!(format_packyapi_balance(&body).is_err());
    }
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test --manifest-path src-tauri/Cargo.toml balance::tests::formats_packyapi -- --nocapture`
Expected: 编译失败(`format_packyapi_balance` 未定义)。若 §11 阻断运行,以编译失败为信号。

- [ ] **Step 3: 加常量 + 格式化函数**

在 balance.rs 常量区(`:8` 之后)加:

```rust
const PACKYAPI_BASE_URL: &str = "https://www.packyapi.com";
/// New-API quota unit: 500000 quota = $1.
const PACKYAPI_QUOTA_PER_USD: f64 = 500_000.0;
```

在 `format_deepseek_balance` 之后加:

```rust
fn format_packyapi_balance(body: &Value) -> Result<String, String> {
    let data = body
        .get("data")
        .filter(|d| !d.is_null())
        .ok_or_else(|| "PackyAPI response did not include account data".to_string())?;
    let quota = data
        .get("quota")
        .and_then(Value::as_f64)
        .ok_or_else(|| "PackyAPI response missing quota".to_string())?;
    let used = data.get("used_quota").and_then(Value::as_f64).unwrap_or(0.0);
    let remain_usd = quota / PACKYAPI_QUOTA_PER_USD;
    let used_usd = used / PACKYAPI_QUOTA_PER_USD;
    Ok(format!("${:.2} 剩 / ${:.2} 用", remain_usd, used_usd))
}
```

- [ ] **Step 4: 加查询函数**

在 `query_deepseek_balance` 之后加。注意 New-Api-User 头与两个 cookie:

```rust
async fn query_packyapi_balance(
    client: &reqwest::Client,
    session: &str,
    itoken: &str,
    user_id: &str,
) -> Result<String, String> {
    let session = session.trim();
    let user_id = user_id.trim();
    if session.is_empty() {
        return Err("PackyAPI session cookie is not configured".to_string());
    }
    if user_id.is_empty() {
        return Err("PackyAPI user id is not configured".to_string());
    }

    let cookie_value = if itoken.trim().is_empty() {
        format!("session={session}")
    } else {
        format!("session={session}; TDC_itoken={}", itoken.trim())
    };

    let url = format!("{}/api/user/self", PACKYAPI_BASE_URL);
    let response = client
        .get(url)
        .header(COOKIE, HeaderValue::from_str(&cookie_value)
            .map_err(|_| "PackyAPI credentials contain invalid header characters".to_string())?)
        .header("New-Api-User", HeaderValue::from_str(user_id)
            .map_err(|_| "PackyAPI user id is invalid".to_string())?)
        .header("Accept", HeaderValue::from_static("application/json"))
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                "PackyAPI balance request timed out".to_string()
            } else {
                format!("PackyAPI balance request failed: {e}")
            }
        })?;

    let status = response.status();
    if !status.is_success() {
        return Err(match status.as_u16() {
            401 | 403 => "PackyAPI authentication failed".to_string(),
            429 => "PackyAPI balance request was rate limited".to_string(),
            code => format!("PackyAPI balance request failed with HTTP {code}"),
        });
    }

    let body = response
        .json::<Value>()
        .await
        .map_err(|_| "PackyAPI balance response was not valid JSON".to_string())?;

    // New-API returns success:false + message on auth/business errors with HTTP 200
    if body.get("success").and_then(Value::as_bool) == Some(false) {
        let msg = body.get("message").and_then(Value::as_str).unwrap_or("unknown error");
        return Err(format!("PackyAPI: {msg}"));
    }
    format_packyapi_balance(&body)
}
```

- [ ] **Step 5: 运行测试 / 编译**

Run: `cargo test --manifest-path src-tauri/Cargo.toml balance::tests -- --nocapture`(或 `cargo check`)
Expected: 新测试 PASS / 编译通过。

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands/balance.rs
git commit -m "feat(balance): PackyAPI 余额查询(New-API /api/user/self,quota→USD)"
```

---

## Task 3: cookie jar 自动续期 + 接入 packyapi source

**Files:**
- Modify: `src-tauri/src/commands/balance.rs`(`refresh_balance_status_inner` `:343-399`、source 校验 `:347`)
- Modify: `src-tauri/Cargo.toml`(确认 reqwest `cookies` feature)

**Interfaces:**
- Consumes: `query_packyapi_balance`(Task 2)、`query_mimo_balance`、`query_deepseek_balance`。
- Produces: client 带 `Arc<Jar>`;mimo/packyapi 请求后回写续期 cookie 到 `helper`;source 支持 `packyapi`。

**说明:** cookie 续期需显式 `reqwest::cookie::Jar`。`cookie_store(true)` 用内部不可读 store,无法回写。改为 `.cookie_provider(jar.clone())`,请求后用 `jar.cookies(&url)` 读回 `Set-Cookie` 更新的值。

- [ ] **Step 1: 给 reqwest 添加 cookies feature(硬性前置)**

**已核实:** `src-tauri/Cargo.toml:32` 当前为 `reqwest = { version = "0.12", features = ["json", "stream"] }`,**未含 `"cookies"`**——没有它,Step3 的 `reqwest::cookie::{Jar, CookieStore}` 与 `.cookie_provider(jar)` 全部编译失败。把该行改为:

```toml
reqwest = { version = "0.12", features = ["json", "stream", "cookies"] }
```

Run: `cargo check --manifest-path src-tauri/Cargo.toml`(确认 feature 拉取成功)。

- [ ] **Step 2: 扩 source 校验**

`balance.rs:347` 改为:

```rust
    if !matches!(requested.as_str(), "all" | "deepseek" | "mimo" | "packyapi") {
        return Err(format!("Unknown balance source: {requested}"));
    }
```

- [ ] **Step 3: client 用显式 cookie jar**

替换 `:353-356` 的 client 构造:

```rust
    use std::sync::Arc;
    use reqwest::cookie::{Jar, CookieStore};

    let jar = Arc::new(Jar::default());
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .cookie_provider(jar.clone())
        .build()
        .map_err(|e| format!("Balance HTTP client build failed: {e}"))?;
```

- [ ] **Step 4: PackyAPI 查询块 + 续期回写**

在 mimo 块(`:372-393`)之后加 packyapi 块。请求前把存盘 cookie 注入 jar,请求后回读:

```rust
    if requested == "all" || requested == "packyapi" {
        let url: reqwest::Url = format!("{}/api/user/self", PACKYAPI_BASE_URL)
            .parse()
            .map_err(|e| format!("packyapi url: {e}"))?;
        // Seed jar with stored cookies so the request authenticates.
        if let Some(ref sess) = helper.packyapi_session {
            jar.add_cookie_str(&format!("session={sess}"), &url);
        }
        if let Some(ref it) = helper.packyapi_itoken {
            jar.add_cookie_str(&format!("TDC_itoken={it}"), &url);
        }
        let result = query_packyapi_balance(
            &client,
            helper.packyapi_session.as_deref().unwrap_or(""),
            helper.packyapi_itoken.as_deref().unwrap_or(""),
            helper.packyapi_user_id.as_deref().unwrap_or(""),
        )
        .await;
        // Read back any renewed cookies the server set.
        if let Some(renewed) = jar.cookies(&url) {
            if let Ok(cookie_hdr) = renewed.to_str() {
                update_cookie_from_jar(&mut helper.packyapi_session, cookie_hdr, "session");
                update_cookie_from_jar(&mut helper.packyapi_itoken, cookie_hdr, "TDC_itoken");
            }
        }
        helper.cache.insert("packyapi".to_string(), balance_cache_entry("packyapi", result));
    }
```

- [ ] **Step 5: 加 cookie 回读辅助 + 小米同样续期**

在 balance.rs 加辅助函数(解析 `name=value; name2=value2` 的合并 cookie 头,提取某 name 的新值):

```rust
/// Extract `name`'s value from a merged cookie header (`a=1; b=2`) and, if present
/// and non-empty, overwrite the stored credential so server-renewed cookies persist.
fn update_cookie_from_jar(stored: &mut Option<String>, cookie_header: &str, name: &str) {
    for part in cookie_header.split(';') {
        let part = part.trim();
        if let Some(val) = part.strip_prefix(&format!("{name}=")) {
            if !val.is_empty() {
                *stored = Some(val.to_string());
            }
            return;
        }
    }
}
```

对小米块(`:372-393`)做同样的 jar 注入 + 回读:请求前 `jar.add_cookie_str` 注入 `api-platform_serviceToken`/`api-platform_slh`/`api-platform_ph`(URL 用 `https://platform.xiaomimimo.com`),请求后回读更新 `helper.mimo_service_token`/`mimo_slh`/`mimo_ph`。

> 实现细节:小米 `build_mimo_headers` 当前是手动拼 Cookie 头(`:159-165`),与 jar 双轨。最小改动方案:保留 `build_mimo_headers` 用于首次请求鉴权,额外在请求后从 jar 回读续期值。确认 `query_mimo_balance` 内部用的是 `client`(带 jar 的同一个),则 `Set-Cookie` 会进 jar。若 `query_mimo_balance` 自建 client 则需改为接收外部 client —— 实现时 Read `query_mimo_balance`(`:295-341`)确认它用传入的 `client` 参数(从 `:373` 看是传入的),故同一 jar 生效。

- [ ] **Step 6: 编译验证**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 通过。确认 `CookieStore` trait 在作用域内(`jar.cookies` 需要)。

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/commands/balance.rs src-tauri/Cargo.toml
git commit -m "feat(balance): cookie jar 自动续期(小米+PackyAPI)并接入 packyapi source"
```

---

## Task 4: 前端 PackyAPI 面板 + 三列网格

**Files:**
- Modify: `src/routes/usage/+page.svelte`(Balance Card `:486-685`、`balanceStatusText` 已通用、凭据保存)
- Modify: `src/lib/api.ts:480-485`(`refreshBalanceStatus` source 枚举)
- Modify: `src/lib/types.ts`(`BalanceHelperSettings` TS 类型加 packyapi 字段)
- Modify: `messages/en.json`、`messages/zh-CN.json`(根目录,**不在 src-tauri 下**)

**Interfaces:**
- Consumes: 后端 `cache["packyapi"]`(Task 3)、`BalanceHelperSettings.packyapi_*`。
- Produces: DeepSeek 右侧 PackyAPI 面板 + 凭据输入。

- [ ] **Step 1: Read 确认前端结构**

Read `src/routes/usage/+page.svelte:486-685`(Balance Card、DeepSeek/小米面板、网格 class、凭据保存函数如 `saveMimoCredentials`)与 `src/lib/api.ts:480-485`(`refreshBalanceStatus` 类型)。**已核实关键事实:**
- `balanceStatusText(source)` 返回结构是 `{ label, balance, sub, dotClass, tokenPlan? }` —— **没有 `.text` 字段**。DeepSeek/小米面板渲染的是:左侧 `dotClass`(状态点)+ `label`(状态文案),右侧 `balance`(金额大字)+ `sub`(refreshed_at 或 error 文案)。
- 前端 TS `BalanceHelperSettings` 在 `src/lib/types.ts`,需同步加 packyapi 字段(否则 `npm run check` strict 报错)。

- [ ] **Step 2: 扩 api.ts source 枚举 + TS 类型同步**

(a) `src/lib/api.ts:480-485`:把 `refreshBalanceStatus` 的 source 参数类型从 `"all" | "deepseek" | "mimo"` 改为 `"all" | "deepseek" | "mimo" | "packyapi"`。

(b) `src/lib/types.ts` 的 `BalanceHelperSettings` 接口,在 mimo 字段旁加(snake_case,与后端一致):

```ts
  packyapi_session?: string | null;
  packyapi_itoken?: string | null;
  packyapi_user_id?: string | null;
```

- [ ] **Step 3: 加 PackyAPI 面板**

在 DeepSeek 面板(`:516-557`)之后、小米面板之前插入 PackyAPI 面板。布局:把外层网格从 `md:grid-cols-2` 改为 `md:grid-cols-3`(DeepSeek/PackyAPI 各一列),小米面板 class 从 `md:col-span-2` 改为 `md:col-span-3`(独占整行)。PackyAPI 面板用绿色系渐变区分,**布局照搬 DeepSeek 面板模板**(用 `.label/.balance/.sub/.dotClass`,**无 `.text`**):

```svelte
      <!-- PackyAPI panel — mirror the DeepSeek panel layout -->
      {@const packy = balanceStatusText("packyapi")}
      <div class="rounded-xl bg-gradient-to-br from-emerald-500/10 ... p-4 space-y-2">
        <div class="flex items-center justify-between">
          <span class="font-medium">PackyAPI</span>
          <span class="h-2 w-2 rounded-full {packy.dotClass}"></span>
        </div>
        <div class="flex items-baseline justify-between">
          <span class="text-xs text-[var(--text-tertiary)]">{packy.label}</span>
          <span class="text-lg font-bold font-[var(--font-mono)]">{packy.balance}</span>
        </div>
        {#if packy.sub}<div class="text-xs text-[var(--text-tertiary)]">{packy.sub}</div>{/if}
        <!-- collapsible credential inputs: session / TDC_itoken / user_id -->
        <details>
          <summary>{t('settings_balance_packyapi_creds')}</summary>
          <input bind:value={packyapiSession} placeholder="session" />
          <input bind:value={packyapiItoken} placeholder="TDC_itoken" />
          <input bind:value={packyapiUserId} placeholder="New-Api-User (用户ID)" />
          <button onclick={savePackyapiCredentials}>{t('settings_balance_save')}</button>
        </details>
      </div>
```

(精确的 Tailwind class 与 spacing 照搬 Step1 读到的 DeepSeek 面板;`packy.label/balance/sub/dotClass` 字段名已核实。)

- [ ] **Step 4: 加凭据 state + 保存函数**

仿 `saveMimoCredentials`:加 `packyapiSession`/`packyapiItoken`/`packyapiUserId` 的 `$state`,从 `getUserSettings().balance_helper` 初始化(读 `balance_helper.packyapi_session` 等 snake 字段);`savePackyapiCredentials` 调 `api.updateUserSettings({ balance_helper: { packyapi_session: packyapiSession, packyapi_itoken: packyapiItoken, packyapi_user_id: packyapiUserId } })` 后 `refreshBalanceStatus("packyapi")`。

- [ ] **Step 5: 新增 i18n 键(根目录 messages/)**

**已核实:** `settings_balance_save` 在 `messages/en.json:1726` 与 zh-CN **已存在**,直接复用,本批次**只新增** `settings_balance_packyapi_creds`。

`messages/zh-CN.json` 新增:
```json
  "settings_balance_packyapi_creds": "PackyAPI 凭据",
```
`messages/en.json` 新增:
```json
  "settings_balance_packyapi_creds": "PackyAPI Credentials",
```

- [ ] **Step 6: 验证**

Run: `npm run check && npm run i18n:check`
Expected: 通过。

- [ ] **Step 7: 运行目测**

`npm run tauri dev` → /usage:
1. 三列网格,DeepSeek | PackyAPI | (小米独占下一行)。
2. 在 PackyAPI 凭据输入 session/TDC_itoken/user_id(用实测值:user_id=98264),保存后显示余额(应约 `$105.6x 剩 / $795.3x 用`),状态点绿色。
3. 凭据错误时显示红点 + 脱敏错误文案。

- [ ] **Step 8: Commit**

```bash
git add src/routes/usage/+page.svelte src/lib/api.ts src/lib/types.ts messages/en.json messages/zh-CN.json
git commit -m "feat(usage): 新增 PackyAPI 余额面板(DeepSeek 右侧,三列网格)"
```

---

## Task 5: 批次 D 收尾验证

- [ ] **Step 1: 全量验证**

Run:
```bash
cargo check  --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
npm run check && npm run lint && npm run i18n:check && npm run build
```
Expected: 全通过。

- [ ] **Step 2: cookie 续期验证(关键)**

`npm run tauri dev`,PackyAPI/小米填入有效 cookie,保持 app 运行并多次手动刷新(或等 auto_refresh)。确认:
- 余额持续可查,不会很快失效。
- 检查 `~/.claw-go/settings.json` 的 `balance_helper`,确认 session/serviceToken 等值随刷新被更新(与初始粘贴值不同即说明续期生效)。
- 若服务端不回 Set-Cookie(值不变),功能仍正常(回写辅助对空值 no-op);失效时显示明确错误。

- [ ] **Step 3: 补提交(若有)**

```bash
git add -A
git commit -m "chore(usage): 批次 D 收尾修正"
```

---

## Self-Review 记录

- **Spec 覆盖:** D1 PackyAPI 卡片 → Task1(字段)+Task2(查询)+Task4(前端);D2 cookie 自动续期 → Task3(jar+回写,覆盖小米与 PackyAPI)。全覆盖。
- **类型一致性:** `query_packyapi_balance(client, session, itoken, user_id)` 在定义(Task2)与调用(Task3 Step4)一致;`packyapi_session/packyapi_itoken/packyapi_user_id` 在 models(Task1)、apply_balance_helper(Task1)、refresh inner(Task3)、前端(Task4)一致;`update_cookie_from_jar(&mut Option<String>, &str, &str)` 定义与调用一致;`format_packyapi_balance(&Value)` 单测与实现一致。
- **占位符扫描:** 代码步骤含完整代码;前端面板的精确 Tailwind class 标注为"与 DeepSeek/小米对齐"(实现时 Read 后照搬现有面板样式),非占位符。
- **PackyAPI 实测验证:** 接口/字段/单位均来自实测响应(quota=52819275, used_quota=397680725, 500000=$1),单测用真实数值。
- **已知确认点:** Task3 Step1 reqwest cookies feature 是否已启用;Task3 Step5 `query_mimo_balance` 用传入 client(已从 `:373` 确认);Task4 `balanceStatusText` 返回结构(Step1 Read 确认)。
