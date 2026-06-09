# MiMo Token 自动刷新方案

## 背景

MiMo Service Token 是用户从小米平台浏览器 Cookie 中手动提取的，用于查询余额和套餐用量。Token 过期后需要手动更新，用户体验不佳。

## 当前架构

### Token 使用方式
- **用途**: 仅用于查询 MiMo 平台的余额和套餐用量，不参与 LLM API 调用
- **认证方式**: 浏览器 Cookie（`serviceToken` + `userId` + `slh` + `ph`）
- **API 端点**: `https://platform.xiaomimimo.com/api/v1/balance` 和 `/api/v1/tokenPlan/usage`
- **配置位置**: Usage 页面 -> 小米面板 -> 展开 MiMo Service Token 输入区

### Token 过期表现
- HTTP 响应状态码 401 或 403
- MiMo API 返回非零 code
- 前端显示 "MiMo authentication failed" 错误

### 当前刷新机制
- **无自动刷新**: 代码中没有任何 Token 自动刷新或续期逻辑
- **手动更新**: 用户需要手动从浏览器重新获取并更新
- **自动轮询**: 每 120 秒自动调用 `refreshBalanceStatus("all")` 重新查询余额（刷新的是数据，不是 Token）

## 可能的解决方案

### 方案 A: 浏览器 Cookie 自动同步（推荐）

#### 原理
通过浏览器扩展或本地代理，自动从浏览器中读取 MiMo Cookie，同步到 ClawGO 应用。

#### 实现方式
1. **浏览器扩展**: 开发 Chrome/Firefox 扩展，监听 MiMo 平台 Cookie 变化，通过本地 HTTP 服务器同步到 ClawGO
2. **本地代理**: 运行本地代理服务器，拦截浏览器请求，提取 Cookie 头信息

#### 优点
- 用户无需手动操作
- Token 过期后自动同步新 Token
- 实时性好

#### 缺点
- 需要开发浏览器扩展
- 用户需要安装扩展
- 安全性考虑（扩展权限）

### 方案 B: 小米账号 OAuth 授权（理想方案）

#### 原理
使用小米账号的 OAuth 授权流程，获取长期有效的 access_token 和 refresh_token。

#### 实现方式
1. **OAuth 流程**: 引导用户授权小米账号，获取 access_token
2. **Token 刷新**: 使用 refresh_token 自动刷新 access_token
3. **API 调用**: 使用 access_token 替代 Cookie 调用 MiMo API

#### 优点
- 标准 OAuth 流程，安全性高
- Token 自动刷新，用户体验好
- 无需手动操作

#### 缺点
- 需要小米平台支持 OAuth 授权
- 可能需要申请开发者权限
- 实现复杂度高

### 方案 C: 延长 Token 有效期（可行方案）

#### 原理
通过定期"保活"请求，延长 Token 的有效期。

#### 实现方式
1. **定期请求**: 每隔一定时间（如 1 小时）发送一次轻量级请求（如查询余额）
2. **Token 刷新**: 如果请求成功，Token 有效期会延长
3. **失败处理**: 如果请求失败，提示用户更新 Token

#### 优点
- 实现简单
- 无需额外依赖
- 可以延长 Token 有效期

#### 缺点
- 不保证 Token 一定延长
- 需要定期发送请求
- 用户仍需手动更新 Token（只是频率降低）

### 方案 D: 多 Token 轮换（备选方案）

#### 原理
支持多个 Token，当一个 Token 过期时自动切换到下一个。

#### 实现方式
1. **Token 池**: 支持用户配置多个 Token
2. **自动切换**: 当一个 Token 失败时，自动尝试下一个
3. **状态管理**: 记录每个 Token 的状态（有效/过期/失败）

#### 优点
- 提高可用性
- 减少手动更新频率
- 实现相对简单

#### 缺点
- 用户需要配置多个 Token
- 管理复杂度增加
- 不解决根本问题

### 方案 E: 模拟浏览器登录（技术方案）

#### 原理
通过模拟浏览器登录流程，自动获取新的 Token。

#### 实现方式
1. **自动化工具**: 使用 Playwright/Selenium 等工具模拟浏览器登录
2. **凭证管理**: 用户提供小米账号密码（加密存储）
3. **自动登录**: 定期自动登录，获取新 Token

#### 优点
- 全自动，用户体验最好
- 不依赖浏览器扩展
- 可以处理各种登录场景

#### 缺点
- 安全性风险（账号密码存储）
- 依赖小米平台登录流程
- 可能违反服务条款
- 实现复杂度高

## 推荐方案

### 短期方案（1-2 周）
**方案 C: 延长 Token 有效期**
- 实现简单，快速见效
- 可以显著减少手动更新频率
- 风险低，不影响现有功能

### 中期方案（1-2 月）
**方案 A: 浏览器 Cookie 自动同步**
- 用户体验好
- 需要开发浏览器扩展
- 可以作为独立功能发布

### 长期方案（3-6 月）
**方案 B: 小米账号 OAuth 授权**
- 最理想的解决方案
- 需要小米平台支持
- 可以作为核心功能

## 实施步骤（方案 C）

### 1. 添加保活机制
在 `balance.rs` 中添加定期保活请求：

```rust
// 保活间隔（秒）
const KEEP_ALIVE_INTERVAL: u64 = 3600; // 1 小时

// 保活请求
async fn keep_alive_mimo_token() -> Result<(), String> {
    let settings = settings::load();
    let Some(helper) = &settings.user.balance_helper else {
        return Ok(());
    };

    // 检查 MiMo 配置
    if helper.mimo_service_token.is_none() {
        return Ok(());
    }

    // 发送轻量级请求
    let client = reqwest::Client::new();
    let headers = build_mimo_headers(helper)?;
    let response = client
        .get("https://platform.xiaomimimo.com/api/v1/balance")
        .headers(headers)
        .send()
        .await?;

    if response.status().is_success() {
        log::debug!("[mimo] keep-alive request successful");
        Ok(())
    } else {
        log::warn!("[mimo] keep-alive request failed: {}", response.status());
        Err(format!("Keep-alive failed: {}", response.status()))
    }
}
```

### 2. 添加定时任务
在应用启动时添加定时任务：

```rust
// 启动保活定时任务
tokio::spawn(async {
    let mut interval = tokio::time::interval(Duration::from_secs(KEEP_ALIVE_INTERVAL));
    loop {
        interval.tick().await;
        if let Err(e) = keep_alive_mimo_token().await {
            log::warn!("[mimo] keep-alive failed: {}", e);
        }
    }
});
```

### 3. 添加状态监控
在前端添加 Token 状态监控：

```typescript
// 监控 Token 状态
let mimoTokenStatus = $state<'valid' | 'expiring' | 'expired' | 'unknown'>('unknown');
let lastKeepAlive = $state<string | null>(null);

// 检查 Token 状态
async function checkMimoTokenStatus() {
    try {
        const result = await api.refreshBalanceStatus('mimo');
        if (result.status === 'ok') {
            mimoTokenStatus = 'valid';
            lastKeepAlive = new Date().toISOString();
        } else {
            mimoTokenStatus = 'expired';
        }
    } catch {
        mimoTokenStatus = 'unknown';
    }
}
```

### 4. 添加用户提示
在 Token 即将过期时提示用户：

```svelte
{#if mimoTokenStatus === 'expiring'}
  <div class="warning-banner">
    MiMo Token 即将过期，请及时更新。
  </div>
{:else if mimoTokenStatus === 'expired'}
  <div class="error-banner">
    MiMo Token 已过期，请更新 Token。
  </div>
{/if}
```

## 预计效果

### 方案 C 实施后
- **Token 有效期**: 从几小时延长到几天甚至几周
- **手动更新频率**: 从每天多次降低到每周 1-2 次
- **用户体验**: 显著提升，减少打断感

### 后续优化
- 结合方案 A（浏览器扩展）可以实现全自动
- 监控 Token 过期模式，优化保活间隔
- 添加 Token 健康度评分，提前预警

## 风险评估

### 低风险
- 保活请求失败不影响核心功能
- 不会泄露用户凭证
- 不违反服务条款（只是常规 API 调用）

### 中风险
- 保活请求可能被小米平台限制
- 需要处理网络异常情况
- 需要监控保活成功率

### 缓解措施
- 添加重试机制
- 监控保活成功率
- 用户可手动禁用保活功能

## 总结

推荐采用**方案 C（延长 Token 有效期）**作为短期解决方案，可以快速改善用户体验。同时规划**方案 A（浏览器 Cookie 自动同步）**作为中期目标，最终实现**方案 B（小米账号 OAuth 授权）**作为长期目标。

这种渐进式方案可以：
1. 快速见效，提升用户体验
2. 降低实施风险
3. 为后续优化奠定基础
4. 保持代码可维护性