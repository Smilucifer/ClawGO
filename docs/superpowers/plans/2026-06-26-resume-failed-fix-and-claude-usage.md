# resume 误判修复 + Claude 订阅 usage 显示 — 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复 resume 时把 `0x40010004`（外部 TerminateProcess）误判为 `failed` 的 bug；并在聊天页 top-bar 仅对官方 Claude 订阅会话显示 5h/周额度用量。

**Architecture:** 两个独立子系统。Part A 改 actor 的 EOF 终态分类（Rust，纯函数 + 集成点）。Part B 新增一个 Tauri 命令读 `~/.claude/.credentials.json` 的 OAuth token 调 `GET /api/oauth/usage`，前端加 store + top-bar 徽标，回合结束刷新。

**Tech Stack:** Rust (tokio, reqwest 0.12 + json), Tauri 2.x, SvelteKit + Svelte 5 runes, Vitest。

## Global Constraints

- Windows-first：路径用 `PathBuf`，不假设 WSL/Unix。
- Rust 单测在本机有运行时问题（§11 CLAUDE.md），**验证以 `cargo check` / `cargo clippy -- -D warnings` 为准**；分类逻辑抽成纯函数，单测仍写但允许本机不跑通。
- 所有 UI 文案必须同时加进 `messages/en.json` 与 `messages/zh-CN.json`，并通过 `npm run i18n:check`。
- Conventional Commits（`feat:`/`fix:`/`chore:`）。
- 不改 `/usage` 页、不改现有 `balance.rs` 第三方余额逻辑。
- 绝不提交任何 token / credentials / 运行期状态。
- provider 身份 ≠ 执行身份（§5）：官方 Claude 订阅判定见 Part B Task 5。

---

# Part A — 修复 resume 误判 failed

**受影响文件：**
- Modify: `src-tauri/src/agent/session_actor.rs`
  - 结构体增 `stopping: bool` 字段（结构体定义处，初始化处）
  - `handle_stop()`（约 1287 行）开头置 `stopping`
  - `handle_eof()`（约 2201-2215 行）终态分类
  - 新增纯函数 `classify_eof_state(...)` + `is_windows_termination_code(...)`
- Test: 同文件 `#[cfg(test)] mod tests`（纯函数测试）

**Interfaces：**
- Produces:
  - `fn is_windows_termination_code(code: i32) -> bool`
  - `fn classify_eof_state(got_result: bool, cancelled: bool, stopping: bool, exit_code: Option<i32>) -> &'static str`（返回 `"completed"` / `"stopped"` / `"failed"`）

---

### Task A1: 抽出 EOF 终态分类纯函数 + 终止码识别

**Files:**
- Modify: `src-tauri/src/agent/session_actor.rs`（新增两个自由函数 + 测试模块）

**Interfaces:**
- Produces: `is_windows_termination_code`、`classify_eof_state`（签名见上）

- [ ] **Step 1: 写失败测试**

在 `session_actor.rs` 末尾的测试模块（若无则新建 `#[cfg(test)] mod eof_classify_tests { use super::*; ... }`）加入：

```rust
#[cfg(test)]
mod eof_classify_tests {
    use super::{classify_eof_state, is_windows_termination_code};

    #[test]
    fn windows_termination_codes_recognized() {
        assert!(is_windows_termination_code(0x4001_0004u32 as i32)); // DBG_TERMINATE_PROCESS
        assert!(is_windows_termination_code(0xC000_013Au32 as i32)); // STATUS_CONTROL_C_EXIT
        assert!(!is_windows_termination_code(0));
        assert!(!is_windows_termination_code(1));
    }

    #[test]
    fn got_result_event_takes_no_override() {
        // got_result==true 时本函数不应被调用；这里只测 false 分支语义
        assert_eq!(classify_eof_state(false, false, false, Some(0)), "completed");
    }

    #[test]
    fn explicit_stop_is_stopped() {
        assert_eq!(classify_eof_state(false, true, false, Some(1)), "stopped"); // cancel
        assert_eq!(classify_eof_state(false, false, true, Some(1)), "stopped"); // stopping flag
    }

    #[test]
    fn external_termination_code_is_stopped_not_failed() {
        assert_eq!(
            classify_eof_state(false, false, false, Some(0x4001_0004u32 as i32)),
            "stopped"
        );
    }

    #[test]
    fn genuine_nonzero_exit_is_failed() {
        assert_eq!(classify_eof_state(false, false, false, Some(1)), "failed");
        assert_eq!(classify_eof_state(false, false, false, None), "failed");
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 编译失败 —「cannot find function `classify_eof_state`/`is_windows_termination_code`」。

- [ ] **Step 3: 实现两个纯函数**

在 `session_actor.rs` 的 `impl SessionActor` **之外**（模块级自由函数，紧邻 `map_state_to_run_status` 附近）加入：

```rust
/// 已知的 Windows「强制终止」退出码（非崩溃、非业务失败）。
/// 注意：我们自身的 kill 用退出码 1/127，不会落进这里。
fn is_windows_termination_code(code: i32) -> bool {
    matches!(
        code as u32,
        0x4001_0004 // DBG_TERMINATE_PROCESS（外部 TerminateProcess）
            | 0x4001_0005 // DBG_CONTROL_C
            | 0xC000_013A // STATUS_CONTROL_C_EXIT（Ctrl-C）
    )
}

/// 在未收到 result 事件的 EOF 路径下，决定终态字符串。
/// 返回 "completed" / "stopped" / "failed"。
fn classify_eof_state(
    _got_result: bool,
    cancelled: bool,
    stopping: bool,
    exit_code: Option<i32>,
) -> &'static str {
    if cancelled || stopping {
        return "stopped";
    }
    match exit_code {
        Some(0) => "completed",
        Some(code) if is_windows_termination_code(code) => "stopped",
        _ => "failed",
    }
}
```

- [ ] **Step 4: 运行测试**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`（编译通过即视为本步通过；§11 运行期问题下不强求 `cargo test` 跑通，但若环境允许可 `cargo test --manifest-path src-tauri/Cargo.toml eof_classify_tests`）
Expected: 编译通过；clippy 无警告。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/agent/session_actor.rs
git commit -m "feat(agent): add classify_eof_state + Windows termination code helper"
```

---

### Task A2: 接入 `stopping` 标志与新分类逻辑

**Files:**
- Modify: `src-tauri/src/agent/session_actor.rs`（结构体字段、`spawn_actor` 初始化、`handle_stop`、`handle_eof`）

**Interfaces:**
- Consumes: `classify_eof_state`（A1）

- [ ] **Step 1: 给 actor 结构体加 `stopping` 字段**

在 `SessionActor`（或对应结构体）字段定义处，紧邻 `state: String` 之类的运行态字段，加入：

```rust
    /// 是否正在“有意终止”（handle_stop 进行中）。用于让竞争的 handle_eof
    /// 把这次终止判为 stopped 而非 failed。
    stopping: bool,
```

在该结构体被构造的地方（`spawn_actor` 内创建 actor 实例处）初始化：

```rust
            stopping: false,
```

- [ ] **Step 2: `handle_stop` 开头置标志**

把 `handle_stop`（约 1287 行）改为在最开头置 `stopping`：

```rust
    async fn handle_stop(&mut self) -> Result<(), String> {
        log::debug!("[actor] handle_stop: run_id={}", self.run_id);
        self.stopping = true; // 标记有意终止，供竞争的 handle_eof 判定

        // Drop stdin to signal EOF to CLI
        self.stdin.take();

        // Kill process
        if let Some(ref mut child) = self.child {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }

        Ok(())
    }
```

- [ ] **Step 3: `handle_eof` 改用 `classify_eof_state` + 诊断日志**

把 `handle_eof`（约 2201-2215 行）的 `if !self.protocol.got_result_event { ... }` 分支替换为：

```rust
        if !self.protocol.got_result_event {
            let cancelled = self.cancel.is_cancelled();
            let state_str = classify_eof_state(
                self.protocol.got_result_event,
                cancelled,
                self.stopping,
                exit_code,
            );

            // 诊断日志：外部终止来源尚未坐实，复现时据此定位
            if state_str != "completed" {
                log::warn!(
                    "[actor] EOF terminal classify: run_id={} state={} exit_code={:?} \
                     stopping={} cancelled={} is_resume={}",
                    self.run_id,
                    state_str,
                    exit_code,
                    self.stopping,
                    cancelled,
                    self.protocol.is_resume()
                );
            }

            let error_msg = if state_str == "failed" {
                Some(format!("Process exited with code {:?}", exit_code))
            } else {
                None
            };
            self.emit_state(state_str, exit_code, error_msg, true);
        } else {
            self.finalize_meta(exit_code);
        }
```

- [ ] **Step 4: 校验编译与 lint**

Run: `cargo check --manifest-path src-tauri/Cargo.toml && cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`
Expected: 通过，无警告。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/agent/session_actor.rs
git commit -m "fix(agent): treat external TerminateProcess (0x40010004) on resume as stopped, not failed"
```

---

# Part B — 聊天页 Claude 订阅 usage 显示

**受影响文件：**
- Create: `src-tauri/src/commands/claude_usage.rs`（新命令）
- Modify: `src-tauri/src/commands/mod.rs`（挂模块）
- Modify: `src-tauri/src/lib.rs`（`generate_handler!` 注册，约 302 行 balance 命令旁）
- Create: `src/lib/stores/claude-usage-store.svelte.ts`
- Create: `src/lib/components/ClaudeUsageBadge.svelte`
- Modify: `src/lib/api.ts`（命令封装）
- Modify: `src/lib/types.ts`（返回类型）
- Modify: `src/routes/chat/+page.svelte`（插入徽标 + 回合结束刷新）
- Modify: `messages/en.json`、`messages/zh-CN.json`

**Interfaces：**
- Produces（后端）：Tauri 命令 `get_claude_subscription_usage() -> Result<ClaudeSubscriptionUsage, String>`
- Produces（前端）：`getClaudeSubscriptionUsage(): Promise<ClaudeSubscriptionUsage>`、`ClaudeUsageStore`、`<ClaudeUsageBadge />`

---

### Task B1: 抓取真实 `/api/oauth/usage` 响应（去占位 spike）

**目的**：固化 parser 前先拿到真实 JSON 形状，消除 spec 里「实现期核对」。**不写入仓库、不提交任何 token。**

- [ ] **Step 1: 用本机 OAuth token 抓一次响应**

Run（PowerShell；token 只在内存，不落盘）：
```powershell
$cred = Get-Content "$env:USERPROFILE\.claude\.credentials.json" | ConvertFrom-Json
$tok = $cred.claudeAiOauth.accessToken
curl.exe -s -H "Authorization: Bearer $tok" https://api.anthropic.com/api/oauth/usage
```
Expected: 返回 JSON，含 `five_hour` / `seven_day` 等键。

- [ ] **Step 2: 记录字段形状**

把响应的**键结构**（不含任何敏感值）记到本任务笔记：确认 `five_hour`/`seven_day` 下的利用率字段名（是 `utilization` 还是 `used`/`limit`）、`resets_at` 时间格式、`rate_limit_tier` 是否在顶层、是否需要额外 header。若实际字段名与下文 struct 不符，则在 B2 据实调整字段名。

- [ ] **Step 3: 无需提交**（spike，不产生仓库改动）

---

### Task B2: 后端命令 `get_claude_subscription_usage`

**Files:**
- Create: `src-tauri/src/commands/claude_usage.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: B1 确认的真实字段名；`crate::storage::teams::claude_home_dir() -> PathBuf`
- Produces: `#[tauri::command] pub async fn get_claude_subscription_usage() -> Result<ClaudeSubscriptionUsage, String>`

- [ ] **Step 1: 写失败测试（凭据解析纯函数）**

新建 `src-tauri/src/commands/claude_usage.rs`，先放纯函数 + 测试（HTTP 不便单测，只测本地解析）：

```rust
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
pub struct UsageWindow {
    pub utilization: f64,
    pub resets_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClaudeSubscriptionUsage {
    pub five_hour: Option<UsageWindow>,
    pub seven_day: Option<UsageWindow>,
    pub seven_day_opus: Option<UsageWindow>,
    pub subscription_type: Option<String>,
    pub rate_limit_tier: Option<String>,
    pub fetched_at: String,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CredentialsFile {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: Option<OauthBlock>,
}

#[derive(Debug, Deserialize)]
struct OauthBlock {
    #[serde(rename = "accessToken")]
    access_token: Option<String>,
    #[serde(rename = "subscriptionType")]
    subscription_type: Option<String>,
}

/// 从 credentials JSON 文本里取 (access_token, subscription_type)。
fn parse_credentials(text: &str) -> Result<(String, Option<String>), String> {
    let parsed: CredentialsFile =
        serde_json::from_str(text).map_err(|e| format!("credentials parse error: {e}"))?;
    let oauth = parsed
        .claude_ai_oauth
        .ok_or_else(|| "no claudeAiOauth block".to_string())?;
    let token = oauth
        .access_token
        .filter(|t| !t.trim().is_empty())
        .ok_or_else(|| "no accessToken".to_string())?;
    Ok((token, oauth.subscription_type))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_credentials_extracts_token_and_type() {
        let json = r#"{"claudeAiOauth":{"accessToken":"abc","subscriptionType":"max"}}"#;
        let (tok, sub) = parse_credentials(json).unwrap();
        assert_eq!(tok, "abc");
        assert_eq!(sub.as_deref(), Some("max"));
    }

    #[test]
    fn parse_credentials_errors_when_missing() {
        assert!(parse_credentials(r#"{}"#).is_err());
        assert!(parse_credentials(r#"{"claudeAiOauth":{"accessToken":""}}"#).is_err());
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

先把模块挂上：在 `src-tauri/src/commands/mod.rs` 加 `pub mod claude_usage;`。
Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: 编译通过但测试未跑（或 `cargo test ... claude_usage` 在可运行时通过）。本步主要确认模块挂载与解析函数可编译。

- [ ] **Step 3: 实现命令本体（读凭据 → 调 endpoint → 组装）**

在 `claude_usage.rs` 追加：

```rust
fn credentials_path() -> PathBuf {
    crate::storage::teams::claude_home_dir().join(".credentials.json")
}

fn json_window(v: &serde_json::Value, key: &str) -> Option<UsageWindow> {
    let w = v.get(key)?;
    Some(UsageWindow {
        // 字段名以 B1 实测为准；若实测为 used/limit 则在此换算成 0..1 的 utilization
        utilization: w.get("utilization").and_then(|x| x.as_f64()).unwrap_or(0.0),
        resets_at: w
            .get("resets_at")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
    })
}

#[tauri::command]
pub async fn get_claude_subscription_usage() -> Result<ClaudeSubscriptionUsage, String> {
    let fetched_at = crate::models::now_iso();

    let text = match std::fs::read_to_string(credentials_path()) {
        Ok(t) => t,
        Err(e) => {
            return Ok(empty_with_error(fetched_at, format!("no credentials: {e}")));
        }
    };
    let (token, sub_type) = match parse_credentials(&text) {
        Ok(v) => v,
        Err(e) => return Ok(empty_with_error(fetched_at, e)),
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .get("https://api.anthropic.com/api/oauth/usage")
        .bearer_auth(&token)
        .send()
        .await;

    let resp = match resp {
        Ok(r) => r,
        Err(e) => return Ok(empty_with_error(fetched_at, format!("request failed: {e}"))),
    };
    if !resp.status().is_success() {
        // 401 等：token 过期，优雅降级（不自刷 refreshToken）
        return Ok(empty_with_error(
            fetched_at,
            format!("usage http {}", resp.status().as_u16()),
        ));
    }
    let body: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => return Ok(empty_with_error(fetched_at, format!("bad json: {e}"))),
    };

    Ok(ClaudeSubscriptionUsage {
        five_hour: json_window(&body, "five_hour"),
        seven_day: json_window(&body, "seven_day"),
        seven_day_opus: json_window(&body, "seven_day_opus"),
        subscription_type: sub_type,
        rate_limit_tier: body
            .get("rate_limit_tier")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()),
        fetched_at,
        error: None,
    })
}

fn empty_with_error(fetched_at: String, error: String) -> ClaudeSubscriptionUsage {
    ClaudeSubscriptionUsage {
        five_hour: None,
        seven_day: None,
        seven_day_opus: None,
        subscription_type: None,
        rate_limit_tier: None,
        fetched_at,
        error: Some(error),
    }
}
```
> 若 B1 实测响应里 `rate_limit_tier`/字段名不同，按实测改 `json_window` 与顶层取值；逻辑骨架不变。

- [ ] **Step 4: 注册命令**

在 `src-tauri/src/lib.rs` 的 `tauri::generate_handler![ ... ]`（约 302 行 `commands::balance::refresh_balance_status` 旁）加入一行：

```rust
            commands::claude_usage::get_claude_subscription_usage,
```

- [ ] **Step 5: 校验编译与 lint**

Run: `cargo check --manifest-path src-tauri/Cargo.toml && cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`
Expected: 通过。

- [ ] **Step 6: 提交**

```bash
git add src-tauri/src/commands/claude_usage.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs
git commit -m "feat(usage): backend command to fetch Claude subscription usage via oauth/usage"
```

---

### Task B3: 前端类型 + api 封装

**Files:**
- Modify: `src/lib/types.ts`
- Modify: `src/lib/api.ts`

**Interfaces:**
- Produces: `ClaudeSubscriptionUsage` 类型；`getClaudeSubscriptionUsage()`

- [ ] **Step 1: 加类型**

在 `src/lib/types.ts` 末尾追加（字段与后端 serde 对齐，serde 默认 snake_case）：

```typescript
export interface UsageWindow {
  utilization: number;
  resets_at: string | null;
}

export interface ClaudeSubscriptionUsage {
  five_hour: UsageWindow | null;
  seven_day: UsageWindow | null;
  seven_day_opus: UsageWindow | null;
  subscription_type: string | null;
  rate_limit_tier: string | null;
  fetched_at: string;
  error: string | null;
}
```

- [ ] **Step 2: 加 api 封装**

在 `src/lib/api.ts` 现有 balance/usage 封装附近追加：

```typescript
import type { ClaudeSubscriptionUsage } from "./types";

export async function getClaudeSubscriptionUsage(): Promise<ClaudeSubscriptionUsage> {
  dbg("api", "getClaudeSubscriptionUsage");
  return invoke<ClaudeSubscriptionUsage>("get_claude_subscription_usage");
}
```
> 若 `ClaudeSubscriptionUsage` 已在文件顶部统一 import，则只补函数体，勿重复 import。

- [ ] **Step 3: 校验类型**

Run: `npm run check`
Expected: 无类型错误。

- [ ] **Step 4: 提交**

```bash
git add src/lib/types.ts src/lib/api.ts
git commit -m "feat(usage): frontend type + api wrapper for claude subscription usage"
```

---

### Task B4: usage store

**Files:**
- Create: `src/lib/stores/claude-usage-store.svelte.ts`
- Test: `src/lib/stores/claude-usage-store.test.ts`

**Interfaces:**
- Consumes: `getClaudeSubscriptionUsage`（B3）
- Produces: `ClaudeUsageStore`（`data`、`loading`、`refresh()`）

- [ ] **Step 1: 写失败测试**

新建 `src/lib/stores/claude-usage-store.test.ts`：

```typescript
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("$lib/api", () => ({
  getClaudeSubscriptionUsage: vi.fn(),
}));
import { getClaudeSubscriptionUsage } from "$lib/api";
import { ClaudeUsageStore } from "./claude-usage-store.svelte";

describe("ClaudeUsageStore", () => {
  beforeEach(() => vi.resetAllMocks());

  it("refresh stores fetched data", async () => {
    (getClaudeSubscriptionUsage as any).mockResolvedValue({
      five_hour: { utilization: 0.42, resets_at: null },
      seven_day: { utilization: 0.18, resets_at: null },
      seven_day_opus: null,
      subscription_type: "max",
      rate_limit_tier: "tier_x",
      fetched_at: "2026-06-26T00:00:00Z",
      error: null,
    });
    const store = new ClaudeUsageStore();
    await store.refresh();
    expect(store.data?.five_hour?.utilization).toBe(0.42);
    expect(store.loading).toBe(false);
  });

  it("keeps previous data on fetch error (stale)", async () => {
    const store = new ClaudeUsageStore();
    (getClaudeSubscriptionUsage as any).mockResolvedValue({
      five_hour: { utilization: 0.5, resets_at: null },
      seven_day: null, seven_day_opus: null,
      subscription_type: null, rate_limit_tier: null,
      fetched_at: "t1", error: null,
    });
    await store.refresh();
    (getClaudeSubscriptionUsage as any).mockRejectedValue(new Error("boom"));
    await store.refresh();
    expect(store.data?.five_hour?.utilization).toBe(0.5); // 保留上次
  });
});
```

- [ ] **Step 2: 运行测试确认失败**

Run: `npm test -- src/lib/stores/claude-usage-store.test.ts`
Expected: FAIL —「Cannot find module './claude-usage-store.svelte'」。

- [ ] **Step 3: 实现 store**

新建 `src/lib/stores/claude-usage-store.svelte.ts`：

```typescript
import * as api from "$lib/api";
import type { ClaudeSubscriptionUsage } from "$lib/types";

export class ClaudeUsageStore {
  data = $state<ClaudeSubscriptionUsage | null>(null);
  loading = $state(false);

  async refresh(): Promise<void> {
    this.loading = true;
    try {
      const next = await api.getClaudeSubscriptionUsage();
      // 成功才覆盖；后端用 error 字段表达软失败，仍覆盖以更新 error 状态
      this.data = next;
    } catch {
      // 硬失败（IPC 异常）：保留上一次数据，标记 stale 由 fetched_at 体现
    } finally {
      this.loading = false;
    }
  }
}

/** 单例：聊天页共享。 */
export const claudeUsageStore = new ClaudeUsageStore();
```

- [ ] **Step 4: 运行测试**

Run: `npm test -- src/lib/stores/claude-usage-store.test.ts`
Expected: PASS（2 passed）。

- [ ] **Step 5: 提交**

```bash
git add src/lib/stores/claude-usage-store.svelte.ts src/lib/stores/claude-usage-store.test.ts
git commit -m "feat(usage): claude usage store with stale-on-error"
```

---

### Task B5: top-bar 徽标组件 + i18n

**Files:**
- Create: `src/lib/components/ClaudeUsageBadge.svelte`
- Modify: `messages/en.json`、`messages/zh-CN.json`

**Interfaces:**
- Consumes: `claudeUsageStore`（B4）、`t`（`$lib/i18n/index.svelte`）
- Produces: `<ClaudeUsageBadge />`（无 props，自取单例 store）

- [ ] **Step 1: 加 i18n 文案**

在 `messages/en.json` 加：
```json
  "claudeUsage_5h": "5h",
  "claudeUsage_weekly": "Weekly",
  "claudeUsage_opus": "Opus weekly",
  "claudeUsage_resetsAt": "resets {time}",
  "claudeUsage_tier": "Tier",
  "claudeUsage_plan": "Plan",
  "claudeUsage_stale": "may be stale",
```
在 `messages/zh-CN.json` 加同 key：
```json
  "claudeUsage_5h": "5 小时",
  "claudeUsage_weekly": "本周",
  "claudeUsage_opus": "Opus 周额",
  "claudeUsage_resetsAt": "{time} 重置",
  "claudeUsage_tier": "等级",
  "claudeUsage_plan": "套餐",
  "claudeUsage_stale": "可能已过期",
```

- [ ] **Step 2: 实现徽标组件**

新建 `src/lib/components/ClaudeUsageBadge.svelte`（紧凑徽标 + hover/click popover）：

```svelte
<script lang="ts">
  import { claudeUsageStore } from "$lib/stores/claude-usage-store.svelte";
  import { t } from "$lib/i18n/index.svelte";

  let open = $state(false);
  const data = $derived(claudeUsageStore.data);
  const pct = (u: number | undefined | null) =>
    u == null ? "—" : `${Math.round(u * 100)}%`;
  const fiveHour = $derived(data?.five_hour?.utilization ?? null);
  const weekly = $derived(data?.seven_day?.utilization ?? null);
  // 颜色分档：<0.7 绿，<0.9 黄，否则红
  const tone = (u: number | null) =>
    u == null ? "text-zinc-400" : u < 0.7 ? "text-green-500" : u < 0.9 ? "text-amber-500" : "text-red-500";
</script>

{#if data && !data.error}
  <button
    class="flex items-center gap-1.5 px-2 py-0.5 rounded text-xs hover:bg-zinc-700/40"
    onclick={() => (open = !open)}
    title="Claude usage"
  >
    <span class={tone(fiveHour)}>{t("claudeUsage_5h")} {pct(fiveHour)}</span>
    <span class="text-zinc-500">·</span>
    <span class={tone(weekly)}>{t("claudeUsage_weekly")} {pct(weekly)}</span>
  </button>

  {#if open}
    <div class="absolute z-50 mt-6 right-0 w-64 p-3 rounded-lg bg-zinc-800 border border-zinc-700 shadow-xl text-xs space-y-2">
      {#each [["claudeUsage_5h", data.five_hour], ["claudeUsage_weekly", data.seven_day], ["claudeUsage_opus", data.seven_day_opus]] as [label, w]}
        {#if w}
          <div>
            <div class="flex justify-between mb-0.5">
              <span>{t(label as any)}</span>
              <span class={tone(w.utilization)}>{pct(w.utilization)}</span>
            </div>
            <div class="h-1.5 rounded bg-zinc-700 overflow-hidden">
              <div class="h-full bg-current {tone(w.utilization)}" style="width:{Math.min(100, Math.round(w.utilization * 100))}%"></div>
            </div>
            {#if w.resets_at}
              <div class="text-[10px] text-zinc-500 mt-0.5">{t("claudeUsage_resetsAt", { time: w.resets_at })}</div>
            {/if}
          </div>
        {/if}
      {/each}
      <div class="pt-1 border-t border-zinc-700 text-[10px] text-zinc-400 flex justify-between">
        <span>{t("claudeUsage_plan")}: {data.subscription_type ?? "—"}</span>
        <span>{t("claudeUsage_tier")}: {data.rate_limit_tier ?? "—"}</span>
      </div>
    </div>
  {/if}
{/if}
```
> 样式跟随项目现有 Tailwind 约定；如项目用不同色板，套用邻近组件的类名即可。

- [ ] **Step 3: 校验 i18n + 类型**

Run: `npm run i18n:check && npm run check`
Expected: i18n 无缺失键；类型通过。

- [ ] **Step 4: 提交**

```bash
git add src/lib/components/ClaudeUsageBadge.svelte messages/en.json messages/zh-CN.json
git commit -m "feat(usage): ClaudeUsageBadge top-bar component + i18n"
```

---

### Task B6: 接入聊天页（门控显示 + 回合结束刷新）

**Files:**
- Modify: `src/routes/chat/+page.svelte`

**Interfaces:**
- Consumes: `ClaudeUsageBadge`（B5）、`claudeUsageStore`（B4）、`store.agent`/`store.platformId`/`store.connectionProfileId`/`store.phase`

- [ ] **Step 1: import 组件与 store**

在 `+page.svelte` 顶部 script 的 import 区加：

```typescript
  import ClaudeUsageBadge from "$lib/components/ClaudeUsageBadge.svelte";
  import { claudeUsageStore } from "$lib/stores/claude-usage-store.svelte";
```

- [ ] **Step 2: 官方 Claude 订阅门控 + 渲染徽标**

在 script 内加派生判定（官方订阅 = claude 执行体 + 无 platform + 无 custom 连接档）：

```typescript
  // 仅官方 Claude 订阅：排除 codex、Claude-兼容(platformId 非空)、custom-*(connectionProfileId 非空)、API 直连 anthropic(platformId="anthropic")
  const isOfficialClaudeSub = $derived(
    store.agent === "claude" && !store.platformId && !store.connectionProfileId,
  );
```

在 `SessionStatusBar`（约 4000 行 `<SessionStatusBar ... />` 之后、MCP panel 之前）插入：

```svelte
{#if isOfficialClaudeSub}
  <div class="relative">
    <ClaudeUsageBadge />
  </div>
{/if}
```
> 具体放进 top-bar 的哪个 flex 容器，对齐 `SessionStatusBar` 内现有右侧操作区的排版；若 `SessionStatusBar` 自身是独立组件，则把上面这段放进 `SessionStatusBar.svelte` 的右侧操作区，并把 `isOfficialClaudeSub` 作为 prop 传入。

- [ ] **Step 3: 进入 claude 会话拉一次 + 回合结束刷新**

在 script 内加一个 `$effect`（参考现有 phase 监听写法）：

```typescript
  let _prevPhase: string = "empty";
  $effect(() => {
    const phase = store.phase;
    if (!isOfficialClaudeSub) {
      _prevPhase = phase;
      return;
    }
    // 进入可用会话首次打底
    if (claudeUsageStore.data == null && (phase === "idle" || phase === "ready")) {
      void claudeUsageStore.refresh();
    }
    // 回合结束：running → idle
    if (_prevPhase === "running" && phase === "idle") {
      void claudeUsageStore.refresh();
    }
    _prevPhase = phase;
  });
```

- [ ] **Step 4: 校验类型 + 构建**

Run: `npm run check && npm run build`
Expected: 通过。

- [ ] **Step 5: 提交**

```bash
git add src/routes/chat/+page.svelte
git commit -m "feat(usage): gate ClaudeUsageBadge to official Claude sub + refresh on turn end"
```

---

### Task B7: 全量验证

- [ ] **Step 1: 全量验证**

Run: `npm run verify`
Expected: lint + fmt + i18n + tests + build + Rust checks 全通过（§11：Rust 单测运行期问题以 `cargo check`/`clippy` 通过为准）。

- [ ] **Step 2: 手动验收**

- 官方 Claude 订阅会话：top-bar 出现徽标，显示 5h/周百分比；点开有进度条 + reset + 套餐/tier。
- 切到 deepseek/glm/custom/codex 会话：徽标**不显示**。
- 发一条消息、回合结束后：数字刷新；空闲时不再打接口。
- token 过期（可临时改坏 `.credentials.json` 测试后还原）：徽标隐藏或保留旧值不报错、不打断聊天。

- [ ] **Step 3: 更新 changelog**

在 `docs/changelog.md` 加一条（版本号按 release 流程定）：
```
- fix(agent): resume 时不再把外部 TerminateProcess(0x40010004) 误判为 failed
- feat(usage): 聊天页 top-bar 显示官方 Claude 订阅 5h/周额度
```

- [ ] **Step 4: 提交**

```bash
git add docs/changelog.md
git commit -m "docs: changelog for resume-fix + claude usage badge"
```

---

## 自审记录（writing-plans self-review）
- **spec 覆盖**：Part A 覆盖 spec 第一部分三层（识别终止码=A1/A2、stopping 标志=A2、诊断日志=A2 Step3）；Part B 覆盖数据来源(B1/B2)、过期降级(B2 Step3)、store(B4)、门控显示(B6 Step2)、回合刷新(B6 Step3)、i18n(B5)。
- **占位扫描**：无 TBD/TODO；B1 spike 显式抓真实响应以消除「实现期核对」，残留的「以实测为准」仅限字段名微调，骨架代码完整。
- **类型一致**：`ClaudeSubscriptionUsage`/`UsageWindow` 字段在后端(B2)、前端类型(B3)、store 测试(B4)、组件(B5)一致；命令名 `get_claude_subscription_usage` 在 B2 定义、B3 调用一致。
