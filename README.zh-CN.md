# Claude Session Hub Remaster / Claude Session Hub 重制版

> 本文件是中文入口；英文说明见 [README.md](README.md)。

## 致谢

本项目基于 [AnyiWang/OpenCovibe](https://github.com/AnyiWang/OpenCovibe) 的本地优先桌面架构与 UI 基础继续演进，并吸收了 [TianLin0509/claude-session-hub](https://github.com/TianLin0509/claude-session-hub) 在会议室、备忘录、协作协议和多 CLI 工作流上的产品启发。感谢这两个项目对本 remaster 方向的帮助。

## 项目定位

Claude Session Hub Remaster 的目标不是重写 OpenCovibe，也不是直接搬运 Claude Session Hub。它以 OpenCovibe 当前的 Tauri + Svelte + Rust 架构为主体，逐步叠加 Claude Session Hub 中有差异化价值的协作能力。

当前架构原则：

- `Run` 继续作为最小执行单元。
- `Room` 作为协作编排层，建立在 `Run` 之上。
- Memo、Room、Roundtable、Driver/Copilot、Research、Arena Memory 等能力分阶段落地。
- 不在早期破坏现有 `/chat` 路径。

## 当前功能

### Memo

你可以在应用内维护全局备忘录和项目备忘录，用来保存长期偏好、项目约定、临时上下文和后续待办。项目备忘录会随当前项目目录切换，避免不同项目的上下文混在一起。

支持：

- 添加、编辑、复制、删除、清空 memo。
- Global / Project scope 切换。
- 重启后保留 memo 内容。
- 通过 Command Palette 打开 Memo 面板。

### Rooms、Roundtable 和 Driver/Copilot

Rooms 是多智能体协作的入口。你可以创建 Room、添加 Claude participant，并把 participant 关联到已有或新建的 Run。Room 删除不会删除对应 Run。Research Room 作为 Room kind 在同一入口中创建。

当前适合用来：

- 把相关 Run 聚合到一个 Room 中查看。
- 为 Room 保存局部 memo。
- 在 Roundtable 时间线里向多个活跃 participant 分发同一个问题。
- 使用 `@debate` 让 participant 基于上一轮公开回复互相比较观点。
- 使用 `@summary @name` 指定一个 participant 总结公开 Room 历史。
- 使用 `@name message` 发送私有回合，私有内容不会出现在公开时间线中。
- 创建 Driver Room，让一个 Driver 通过 `/review` 向一个或多个 Copilot 请求只读审查。
- Driver review 会生成 room-local `.arena/context.md`、`.arena/state.md` 和 `.arena/memory`，用于稳定引用 room / run 上下文。
- `.arena` 文件是本地运行上下文镜像，可能包含 run references、memo 和最近的公开 preview；不要把它当成对外分享材料。
- 创建 Research Room，把一个研究主题分发给多个活跃 participant。
- Research Room 会把本轮结果汇总到 room-local `research/artifact.json` 结构化产物，向 `research/artifacts.jsonl` 追加 artifact 历史，并把 `[fact]`、`[decision]`、`[lesson]` 标记行展示为 Arena Memory 候选。

### Windows Native Toolchain Support

在 Windows 上，如果你从普通桌面窗口启动应用，Claude / Codex 子进程通常拿不到 Visual Studio Developer Prompt 里的 `cl`、`link`、Windows SDK 等环境。当前版本会在明确需要 native toolchain 的项目中，自动为本地 CLI 子进程补充 MSVC developer environment。

模式：

- `auto`：默认，仅在保守 native project 信号下启用。
- `always`：强制为本地子进程启用。
- `off`：关闭自动注入。

你可以在 Settings 中切换模式，并查看当前项目的 MSVC 环境状态。状态提示会说明是否已注入、无需注入、已关闭或需要安装 Visual Studio C++ build tools；不会展示完整环境变量。

## 当前限制

- Roundtable、Driver/Copilot 和 Research 当前依赖活跃的本地 Claude participant；更完整的 Codex / Gemini / 多 CLI 能力矩阵仍在后续阶段。
- Driver/Copilot 目前是 MVP：Copilot 只读行为通过 review prompt 约束，危险操作审批和硬权限限制仍在后续阶段。
- Research Room 支持研究分发、artifact 历史归档和标记式 Arena Memory 候选抽取；候选提升为永久项目 Arena Memory 仍在后续阶段。
- 仍有部分上游基线检查需要后续清理。

## 后续计划

- Arena Memory 候选提升：项目事实、决策、经验沉淀。
- Multi-CLI capability matrix。

## 开发

```bash
npm install
npx svelte-kit sync
npm run dev
```

常用验证：

```bash
npm run lint
npm run build
npm run rust:check
npm run i18n:check
npm test -- src/lib/stores/memo-store.test.ts
cargo test --manifest-path src-tauri/Cargo.toml storage::memos::tests:: -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml commands::memos::tests:: -- --nocapture
```

## 许可证

本 remaster 仍应遵守上游项目及其依赖的许可证要求。合并上游代码或迁移 Claude Session Hub 设计时，应保留必要的 attribution 与 license 说明。
