# 记忆系统重构：从角色中心到用户中心

## 背景

阅读了 PilotDeck 的 EdgeClaw Memory (ClawXMemory) 系统源码后，对比 ClawGO 当前的 Character Memory System，发现了两套系统在根本设计理念上的差异，以及 ClawGO 可以吸收的关键设计模式。

## 当前状态

### ClawGO Character Memory System (Phase 10+)

```
┌─────────────────────────────────────────────────────┐
│                   Per-Character 架构                  │
│                                                      │
│  Character A          Character B       Character C  │
│  ┌──────────┐        ┌──────────┐      ┌──────────┐ │
│  │ LanceDB  │        │ LanceDB  │      │ LanceDB  │ │
│  │ 向量索引  │        │ 向量索引  │      │ 向量索引  │ │
│  ├──────────┤        ├──────────┤      ├──────────┤ │
│  │ petgraph │        │ petgraph │      │ petgraph │ │
│  │ 记忆图谱  │        │ 记忆图谱  │      │ 记忆图谱  │ │
│  ├──────────┤        ├──────────┤      ├──────────┤ │
│  │ JSONL    │        │ JSONL    │      │ JSONL    │ │
│  │ 记忆日志  │        │ 记忆日志  │      │ 记忆日志  │ │
│  └──────────┘        └──────────┘      └──────────┘ │
│                                                      │
│  记忆类型: fact / preference / skill / rule           │
│           relationship / experience                   │
│  注入格式: [Character Memory] 列表 → system prompt     │
└─────────────────────────────────────────────────────┘
```

**问题**：
1. 记忆系统回答的是"AI 角色知道什么"，而不是"用户是谁"
2. 每个角色有独立的 LanceDB + 图谱 + JSONL，N 个角色 = N 倍存储
3. 角色的"个性"应该由 `AiCharacter.role_instruction` 定义，记忆不应越权
4. 没有用户画像、没有协作规则（feedback）
5. 记忆存储在二进制向量索引中，不可见、不可编辑
6. 去重仅依赖嵌入余弦相似度，无法合并语义相同但表达不同的记忆

### PilotDeck ClawXMemory 架构（参考目标）

```
┌──────────────────────────────────────────────────────┐
│              用户中心 / 白盒记忆                        │
│                                                       │
│  memory/                                              │
│  ├── MEMORY.md              ← 索引清单（人类可读）       │
│  ├── global/                                          │
│  │   ├── UserIdentity/user-profile.md  ← 用户画像      │
│  │   └── UserIdentityNotes/           ← 用户笔记       │
│  ├── Project/                  ← 项目记忆文件           │
│  │   ├── architecture.md                              │
│  │   └── api-design.md                                │
│  ├── Feedback/                 ← 协作规则文件           │
│  │   └── no-mock-database.md                          │
│  └── project.meta.md           ← 项目元数据             │
│                                                       │
│  三阶段管道: Index → Dream → Retrieval                 │
│  存储: SQLite (L0会话+Trace) + Markdown (记忆本体)      │
│  注入格式: <memory-context> → user message attachment  │
└──────────────────────────────────────────────────────┘
```

## 设计方案

### 核心理念变更

| 维度 | 当前 (ClawGO) | 目标 |
|------|-------------|------|
| 记忆归属 | Per AI 角色 | 用户中心 |
| 记忆类型 | fact/skill/rule/preference/relationship/experience | user / feedback / project |
| 存储介质 | LanceDB 二进制 + JSONL | Markdown 文件（白盒） |
| 向量索引 | 主存储 | 辅助检索层 |
| 图谱 | 角色内记忆关系（Louvain 社区检测） | 简化或移除 |
| 去重 | 嵌入余弦相似度 | LLM 驱动的 Dream 整合 |
| 注入范围 | 按角色筛选 | 私聊 + 群聊统一注入 |
| 用户画像 | 无 | user-profile.md |
| 协作规则 | 无 | feedback/*.md |
| 回滚能力 | 无 | Dream 快照 |

### 数据结构

#### 记忆文件格式（统一）

```markdown
---
name: user-is-senior-go-developer
description: 用户有十年 Go 开发经验，但 React 新手
type: user
scope: global
updatedAt: 2026-05-28T10:30:00Z
---

## 身份背景
- 资深 Go 后端开发者（10年经验）
- React 前端新手，需要对照后端概念解释
```

```markdown
---
name: no-mock-database-in-tests
description: 集成测试必须使用真实数据库
type: feedback
scope: project
projectId: clawgo-main
updatedAt: 2026-05-28T10:30:00Z
---

## Rule
集成测试必须对接真实数据库，不允许 mock。

## Why
上次 mock 测试通过但生产迁移失败，造成数据丢失。

## How To Apply
所有 `*_test.rs` 中涉及数据库的测试用例必须连接真实测试库。
```

#### 目录结构

```
~/.claw-go/memory/
├── MEMORY.md                    # 人类可读索引
├── global/
│   ├── UserIdentity/
│   │   └── user-profile.md      # LLM 定期重写的用户画像
│   └── UserIdentityNotes/       # 单次提取的用户笔记（Dream 时合并到 profile）
├── Project/                     # 项目级记忆
│   ├── architecture.md
│   └── constraints.md
├── Feedback/                    # 协作规则
│   └── no-mock-db.md
└── project.meta.md              # 项目元数据
```

### 模块变更

#### 删除/大幅简化

| 模块 | 原因 |
|------|------|
| `memory_graph.rs` | Louvain 社区检测失去意义（不再有 per-character 记忆图谱） |
| `vectorstore.rs` 中 per-character 索引管理 | 简化为全局索引 |
| 前端 `CharacterMemoryPanel.svelte` | 替换为用户记忆面板 |
| `EmbeddingConfig.chat_api_key` 分离 | 简化为统一配置 |

#### 保留并改造

| 模块 | 改造方向 |
|------|---------|
| `memory_extraction.rs` | prompt 从"提取关于角色的记忆"改为"提取关于用户的事实/偏好/规则" |
| `memory_injection.rs` | 检索范围从 per-character 改为全局 user scope，注入格式改为 `<memory-context>` |
| `context.rs` | 注入逻辑不变 |
| `vectorstore.rs` | LanceDB 保留作为 ANN 检索加速层，不作为主存储 |

#### 新增

| 模块 | 用途 |
|------|------|
| `memory_file_store.rs` | Markdown 文件读写 + frontmatter 解析 + MEMORY.md 索引维护 |
| `memory_dream.rs` | 周期性 LLM 驱动的记忆整合（合并重复、重写用户画像） |
| 前端 `UserMemoryPanel.svelte` | 用户记忆管理面板（替代 CharacterMemoryPanel） |

### 实施阶段

#### Phase 1 — 用户画像 + 协作规则（最小改动）
- 在群聊级别增加 `USER.md` 和 `FEEDBACK.md`
- 修改 `memory_extraction.rs` 的 prompt，从"提取角色的记忆"改为"提取用户信息"
- 注入到所有参与者的 system prompt
- **不改存储层**，先用 JSONL 验证效果

#### Phase 2 — 白盒 Markdown 存储
- 实现 `memory_file_store.rs`：frontmatter 读写 + MEMORY.md 索引
- 写入 LanceDB 的同时生成 markdown 文件
- 前端增加记忆文件浏览面板

#### Phase 3 — Dream 整合
- 实现周期性 LLM 驱动的记忆整合
- 合并重复、重写用户画像
- 快照备份机制

#### Phase 4 — 清理
- 移除 per-character 的 LanceDB 索引
- 移除 `memory_graph.rs` 的 Louvain 社区检测
- 简化前端审核面板
- 统一 `EmbeddingConfig`

## 预期收益

1. **存储复杂度**：N 个角色 × (LanceDB + 图谱 + JSONL) → 1 套 markdown 文件
2. **注入统一**：不再按角色筛选，同一份记忆上下文注入到所有对话
3. **可调试性**：所有记忆以可读 markdown 存在，用户可直接查看/编辑
4. **角色定义回归原位**：`AiCharacter.role_instruction` 控制角色行为，记忆不做越权的事
5. **安全性**：Dream 快照支持回滚，不怕 LLM 提取错误

## 风险

- 从 per-character 迁移到全局 user scope 时，已有的记忆数据需要迁移
- Dream 的 LLM 调用增加成本（可设置间隔控制）
- 去掉图谱后，记忆间的关联关系需要用其他方式表达（markdown 内的 `[[wiki-link]]`）
