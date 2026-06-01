# UI 重构计划：统一应用风格至 Demo 设计系统

## Context

当前 ClawGO 使用 Tailwind + HSL CSS 变量（shadcn 风格）。Demo 页面 (`docs/ui-demo/`) 定义了一套暖色暗黑设计系统（`#1a1918` 底色、`#c9a96e` 强调色、`Inter` 字体）。目标是将实际应用的 **侧边栏 + 顶部标题栏** 贴合 Demo 视觉风格，同时不丢失任何功能入口。

**决策：移除主题切换和色彩方案切换，仅保留暖色暗黑风格。**

---

## 功能入口 Checklist

### A. 窗口标题栏（当前无自定义 title bar，使用系统默认）
- [ ] 自定义标题栏：拖拽区域、应用名、最小化/最大化/关闭按钮
- [ ] `UpdateBanner`（更新提示横幅，页面顶部）

### B. Icon Rail（左侧 44px 图标栏）
- [ ] Logo（`/logo.png`，点击无行为）
- [ ] Chat `/chat` — message 图标
- [ ] Invest `/invest` — trendingUp 图标
- [ ] Explorer `/explorer` — folder 图标
- [ ] Memory `/memory` — book 图标
- [ ] User Memory `/memory-mgmt` — database 图标
- [ ] Plugins `/plugins` — zap 图标
- [ ] History `/history` — clock 图标
- [ ] Usage `/usage` — chart 图标
- [ ] **分隔线**
- [ ] Settings `/settings` — settings 图标（底部固定）
- [ ] App 版本号（点击 → AboutModal）
- [ ] 语言切换器（popup → `LOCALE_REGISTRY`）
- ~~主题切换~~ — **移除**（固定暗色）
- ~~色彩方案切换~~ — **移除**（固定暖色）

### C. Content Panel（侧边栏内容面板，可拖拽调宽）
- [ ] **Panel Header**：页面名称 + 3 个按钮
  - [ ] CLI Session Browser 按钮（鱼图标 → `showCliBrowser`）
  - [ ] 新建对话按钮（+ → `newChat`）
  - [ ] 新建群聊按钮（users → `showGroupChatCreateDialog`）
- [ ] **Chat 页面侧边栏**：
  - [ ] Tab 切换：私聊 | 群聊（`panelTab`）
  - [ ] 私聊 Tab：
    - [ ] 搜索输入框（`runSearchQuery`，深度搜索）
    - [ ] 搜索结果列表（`visibleSearchResults`）
    - [ ] 项目文件夹树（`projectFolders` → `ProjectFolderItem`）
    - [ ] 每个对话项：选中、恢复、删除、强制删除、置顶
    - [ ] "打开文件夹"按钮（`pickFolder`）
    - [ ] 空状态提示
  - [ ] 群聊 Tab：
    - [ ] 新建群聊虚线按钮
    - [ ] 群聊列表项（导航、删除）
    - [ ] 空状态提示
- [ ] **Explorer 页面侧边栏**：
  - [ ] Tab 切换：Files | Git
  - [ ] 项目选择器下拉
  - [ ] 文件树（`treeNodes` snippet）
  - [ ] Git 分支信息 + 变更文件列表
- [ ] **Memory 页面侧边栏**：
  - [ ] 项目文件夹树 + 记忆文件列表
  - [ ] Global scope 折叠区
  - [ ] "打开文件夹"按钮
- [ ] **Plugins 页面侧边栏**：
  - [ ] 5 个 Section 导航（Skills/MCP/Hooks/Plugins/Agents）
- [ ] **拖拽调宽手柄**（`startResize`）

### D. 主内容区顶部栏（非 Chat 页面）
- [ ] 侧边栏切换按钮（`toggleSidebar`）
- [ ] 面包屑（AppName > PageName）
- [ ] Memo 切换按钮（→ `GlobalMemoPanel`）
- [ ] `MoreMenu` 组件
- [ ] 用户记忆切换按钮（→ `UserMemoryPanel`）

### E. Chat 页面顶部栏（`SessionStatusBar`）
- [ ] 由 `SessionStatusBar` 组件自行渲染（不在本次重构范围）

### F. 全局 Modals & Panels
- [ ] `CommandPalette`（Ctrl+P）
- [ ] `SetupWizard`（首次启动）
- [ ] `AboutModal`
- [ ] `PermissionsModal`
- [ ] `CliSessionBrowser`
- [ ] 删除对话确认 Modal
- [ ] 删除项目确认 Modal
- [ ] 群聊创建 Modal
- [ ] `GlobalMemoPanel`（右侧滑出）
- [ ] `UserMemoryPanel`（右侧滑出）
- [ ] `DoctorPanel`（右侧滑出）
- [ ] `PythonSetupOverlay`

---

## 实施方案

### Step 1: 重写 `app.css` 中的 CSS 变量（暖色暗黑固定主题）

移除 light/scheme-neutral 变量，仅保留 `.dark` 分支，将 HSL 值替换为 Demo 设计系统的暖色暗黑值：

| 用途 | 当前值 (HSL) | 新值 |
|---|---|---|
| `--background` | `0 0% 7.1%` | `24 6% 10%` (#1a1918) |
| `--foreground` | `0 0% 93%` | `36 23% 90%` (#ebe8e4) |
| `--primary` | `38 86% 64%` | `38 46% 62%` (#c9a96e) |
| `--secondary` | `0 0% 15.7%` | `20 6% 14%` (#272422) |
| `--muted` | `0 0% 15.7%` | `20 6% 14%` (#272422) |
| `--muted-foreground` | `0 0% 54.5%` | `24 7% 60%` (#9e9a96) |
| `--accent` | `0 0% 15.7%` | `20 7% 16%` (#2a2827) |
| `--border` | `0 0% 24.7%` | `20 6% 18%` (#2e2c2a) |
| `--sidebar-background` | `26 24% 12%` | `20 8% 13%` (#211f1e) |
| `--sidebar-foreground` | `0 0% 93%` | `36 23% 90%` (#ebe8e4) |
| `--sidebar-accent` | `26 15% 20%` | `20 7% 16%` (#2a2827) |
| `--sidebar-border` | `26 10% 18%` | `20 6% 18%` (#2e2c2a) |
| `--popover` | `0 0% 12.9%` | `20 6% 14%` (#272422) |
| `--ring` | `35 93% 77%` | `38 46% 62%` (#c9a96e) |

同时移除 `.scheme-neutral` 和 light mode 变量。

### Step 2: 添加 Inter 字体

在 `src/app.html` 的 `<head>` 中添加 Google Fonts `<link>`，在 `app.css` 中设置 `body { font-family: 'Inter', sans-serif; }`。

### Step 3: 添加自定义窗口标题栏

在 `+layout.svelte` 顶部添加 Tauri 自定义标题栏（`data-tauri-drag-region`）：
- 左侧：应用名 "ClawGO"
- 右侧：最小化 / 最大化 / 关闭按钮
- 需要在 `tauri.conf.json` 中设置 `decorations: false`

**涉及文件：**
- `src-tauri/tauri.conf.json` — 设置 `decorations: false`
- `src/routes/+layout.svelte` — 添加标题栏 HTML + CSS

### Step 4: 移除主题/色彩方案切换功能

从 `+layout.svelte` 中移除：
- `themeMode` / `colorScheme` / `systemDark` / `effectiveDark` 状态变量
- `cycleTheme()` / `cycleScheme()` 函数
- `getInitialTheme()` / `getInitialScheme()` 函数
- `$effect` 中 `localStorage.setItem("clawgo:theme")` 和 `localStorage.setItem("clawgo:colorScheme")`
- `matchMedia` 监听
- Icon Rail 底部的主题和色彩方案按钮
- `document.documentElement.classList.toggle("dark", ...)` 和 `document.documentElement.classList.toggle("scheme-neutral", ...)`
- HTML 中始终添加 `class="dark"`（固定暗色模式）

### Step 5: 重构 Icon Rail 样式

修改 Icon Rail 的 CSS，贴合 Demo 设计系统：
- 背景色：使用 `hsl(var(--sidebar-background))` + 右侧 `border-r`
- 激活态指示条保留，颜色改为 `var(--accent)` 金色
- 图标 hover 背景统一为 `var(--sidebar-accent)` 半透明
- 版本号 + 语言切换器保留

### Step 6: 重构 Content Panel 样式

侧边栏内容面板：
- Tab 栏底部 border 颜色改为 `var(--accent)` 金色
- 对话列表项样式微调（间距、圆角）
- 搜索输入框背景/边框统一

### Step 7: 重构顶部栏样式

非 Chat 页面顶部栏：
- 背景、边框颜色统一
- 面包屑文字颜色
- 按钮 hover 效果

---

## 涉及文件

| 文件 | 改动 |
|---|---|
| `src/app.html` | 添加 Inter 字体 `<link>` |
| `src/app.css` | 重写 CSS 变量为暖色暗黑固定值，移除 light/neutral 分支 |
| `src/routes/+layout.svelte` | 标题栏、移除主题切换、Icon Rail 样式、Content Panel 样式、顶部栏样式 |

## 验证

1. **Checklist 回归**：逐项检查上方所有功能入口是否保留（已移除的标记为 ~~删除线~~）
2. `npm run check` — Svelte 类型检查
3. `npm run build` — 构建验证
4. `npm run tauri dev` — 视觉对比 Demo 页面
5. 各页面切换确认侧边栏内容正确（Chat/Explorer/Memory/Plugins）
