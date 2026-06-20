---
name: group-chat-disable-plugins
description: 群聊参与者（非executor）通过 --settings JSON 注入时禁用所有 plugins 和 hooks，从工具层面断路
status: wip
---

# 群聊参与者插件/钩子断路设计

## 问题

群聊参与者（planner、custom 角色）的 CLI 启动时，通过 `--settings` 注入的 JSON 继承了宿主 `~/.claude/settings.json` 中的所有插件配置（superpowers、TDD、debugging、code-review 等），加上代码强制启用的 `superpowers@claude-plugins-official`。这导致参与者会被插件驱动执行计划外操作（如自动 brainstorming、TDD 流程、code review 等），偏离群聊分配的任务。

hooks 同理——用户配置的 hooks 可能触发插件相关行为，也需要禁用。

MCP servers 不受影响，因为它们通过独立的 `--mcp-config` 路径注入。

## 范围

- **受影响角色**：planner、custom（通过 `permission_mode_override.is_some()` 判断）
- **不受影响**：executor（保留完整工具能力）、普通聊天路径
- **禁用内容**：plugins（`enabledPlugins`）、hooks
- **保留内容**：MCP servers、provider env/auth、基础行为字段

## 变更清单

### 1. `src-tauri/src/agent/provider_claude_config.rs`

#### 1a. `provider_config_json_from_env()` — 增加 `disable_plugins` 参数

```rust
fn provider_config_json_from_env(
    env: &HashMap<String, String>,
    managed: &ManagedConfig,
    disable_plugins: bool,  // 新增
) -> Value {
```

当 `disable_plugins` 为 true 时：
- 跳过 hooks overlay（当前行 651-661）
- 跳过 enabled_plugins overlay（当前行 664-676）
- 跳过强制 superpowers（当前行 678-684）
- 在函数末尾，强制 `obj.insert("enabledPlugins", json!({}))`

#### 1b. `write_provider_claude_config()` — 透传 `disable_plugins`

```rust
pub fn write_provider_claude_config(
    provider_id: &str,
    platform_id: &str,
    cred: &PlatformCredential,
    run_id: &str,
    managed: &ManagedConfig,
    disable_plugins: bool,  // 新增
) -> Result<ProviderClaudeConfigMaterialized, String> {
```

传递给 `provider_config_json_from_env(&env, managed, disable_plugins)`.

#### 1c. `write_managed_settings()` — 透传 `disable_plugins`

```rust
pub fn write_managed_settings(
    run_id: &str,
    managed: &ManagedConfig,
    disable_plugins: bool,  // 新增
) -> Result<PathBuf, String> {
```

传递给 `provider_config_json_from_env(&HashMap::new(), managed, disable_plugins)`.

### 2. `src-tauri/src/commands/session.rs`

#### 2a. `start_session_impl()` — 计算并应用 `disable_plugins`

在构建 `ManagedConfig`（行 626-630）之后，计算：

```rust
let disable_plugins = is_group_chat && permission_mode_override.is_some();
```

当 `disable_plugins` 为 true 时，用空 HashMap 替代 managed 的 hooks 和 plugins：

```rust
let empty_hooks: HashMap<String, serde_json::Value> = HashMap::new();
let empty_plugins: HashMap<String, bool> = HashMap::new();
let managed = crate::agent::provider_claude_config::ManagedConfig {
    mcp_servers: &user_settings.mcp_servers,
    hooks: if disable_plugins { &empty_hooks } else { &user_settings.hooks },
    enabled_plugins: if disable_plugins { &empty_plugins } else { &user_settings.enabled_plugins },
};
```

将 `disable_plugins` 传递给 `write_provider_claude_config()` 和 `write_managed_settings()` 调用。

#### 2b. `has_managed_configs` 条件扩展

当 `disable_plugins` 为 true 时，即使 managed hooks/plugins 为空，也需要强制写 `--settings` 文件来覆盖原生插件：

```rust
let has_managed_configs = !managed.hooks.is_empty()
    || !managed.enabled_plugins.is_empty()
    || disable_plugins;  // 强制写入以清除原生插件
```

## 结果 JSON 对比

### 普通聊天 / executor 群聊参与者（不变）

```json
{
  "permissions": { "defaultMode": "bypassPermissions" },
  "includeCoAuthoredBy": false,
  "thinking": false,
  "language": "简体中文",
  "env": { "ANTHROPIC_BASE_URL": "...", "ANTHROPIC_AUTH_TOKEN": "..." },
  "hooks": { "PreToolUse": [...] },
  "enabledPlugins": {
    "some-user-plugin": true,
    "superpowers@claude-plugins-official": true
  }
}
```

### planner/custom 群聊参与者（新行为）

```json
{
  "permissions": { "defaultMode": "bypassPermissions" },
  "includeCoAuthoredBy": false,
  "thinking": false,
  "language": "简体中文",
  "env": { "ANTHROPIC_BASE_URL": "...", "ANTHROPIC_AUTH_TOKEN": "..." },
  "enabledPlugins": {}
}
```

无 hooks、无 plugins。MCP 通过独立 `--mcp-config` 正常注入。

## 测试策略

- 单元测试：`provider_config_json_from_env` 的 `disable_plugins=true` 路径，验证输出 JSON 无 hooks、plugins 为空
- 集成验证：群聊创建 planner 参与者，检查生成的 `session-{run_id}.json` 文件内容
