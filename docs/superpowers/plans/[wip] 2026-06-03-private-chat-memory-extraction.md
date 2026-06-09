# 私聊历史记忆提取实施方案

## 背景

当前记忆系统只在群聊（Group Chat）的 turn 执行成功后自动提取用户记忆。普通私聊（1-on-1 会话）只注入已有记忆，不提取新记忆。

用户希望私聊也能自动提取记忆，以便从所有对话中学习用户偏好和信息。

## 当前架构

### 记忆提取触发点
- **群聊**: `orchestrator.rs` 第 512-537 行，在 turn 执行成功后调用 `auto_extract_memories()`
- **私聊**: `session.rs` 第 796-813 行，只注入记忆，不提取

### 关键函数
- `auto_extract_memories(turns: &[String])` - 从对话文本中提取记忆
- `can_extract(group_chat_id: &str)` - 检查是否可以提取（防抖 + 每日上限）
- `record_extraction(group_chat_id: &str)` - 记录提取时间

### 数据结构
- `RunEvent` - 存储对话事件（User/Assistant 类型）
- `MemoryNode` - 存储提取的记忆节点

## 实施方案

### 方案 A: 在 session_actor 中添加记忆提取（推荐）

#### 1. 修改 SessionActor 结构体
```rust
struct SessionActor {
    // ... 现有字段 ...
    is_group_chat: bool,  // 新增：标识是否是群聊会话
}
```

#### 2. 修改 finalize_meta() 函数
在 `finalize_meta()` 中，当 session 状态为 `Completed` 且不是群聊会话时，触发记忆提取：

```rust
fn finalize_meta(&self, exit_code: Option<i32>) {
    // ... 现有逻辑 ...

    // 如果是私聊且成功完成，触发记忆提取
    if !self.is_group_chat && terminal_status == RunStatus::Completed {
        let run_id = self.run_id.clone();
        let emitter = Arc::clone(&self.emitter);
        tokio::spawn(async move {
            if let Err(e) = extract_memories_from_private_chat(&run_id, &emitter).await {
                log::warn!("[memory-extraction] private chat extraction failed: {}", e);
            }
        });
    }
}
```

#### 3. 添加 extract_memories_from_private_chat() 函数
```rust
async fn extract_memories_from_private_chat(
    run_id: &str,
    emitter: &BroadcastEmitter,
) -> Result<(), String> {
    // 1. 检查是否可以提取（使用 run_id 作为 group_chat_id 的替代）
    if !can_extract(run_id) {
        return Ok(());
    }

    // 2. 从 RunEvent 中读取对话内容
    let events = storage::events::list_events(run_id, 0);
    let turns: Vec<String> = events
        .iter()
        .filter(|e| matches!(e.event_type, RunEventType::User | RunEventType::Assistant))
        .filter_map(|e| {
            e.payload.get("text").and_then(|v| v.as_str()).map(|s| s.to_string())
        })
        .collect();

    if turns.is_empty() {
        return Ok(());
    }

    // 3. 调用 auto_extract_memories()
    let memories = auto_extract_memories(&turns).await;

    // 4. 记录提取时间
    if !memories.is_empty() {
        record_extraction(run_id);
        log::info!(
            "[memory-extraction] private chat extracted {} memories from run={}",
            memories.len(),
            run_id
        );
    }

    Ok(())
}
```

#### 4. 修改 spawn_actor() 函数
在 `spawn_actor()` 中添加 `is_group_chat` 参数：

```rust
pub fn spawn_actor(
    emitter: Arc<BroadcastEmitter>,
    sessions: ActorSessionMap,
    run_id: String,
    // ... 其他参数 ...
    is_group_chat: bool,  // 新增参数
) -> SessionActorHandle {
    let actor = SessionActor {
        // ... 其他字段 ...
        is_group_chat,
    };
    // ...
}
```

#### 5. 修改调用点
在 `session.rs` 中调用 `spawn_actor()` 时传入 `is_group_chat` 参数。

### 方案 B: 在 session.rs 中添加记忆提取（备选）

在 `start_session_impl()` 中，当 session 完成时触发记忆提取。但这需要监听 session 状态变化，实现更复杂。

## 实施步骤

1. **修改 SessionActor 结构体** - 添加 `is_group_chat` 字段
2. **修改 spawn_actor()** - 添加 `is_group_chat` 参数
3. **修改调用点** - 传入 `is_group_chat` 值
4. **添加 extract_memories_from_private_chat()** - 实现记忆提取逻辑
5. **修改 finalize_meta()** - 触发记忆提取
6. **测试** - 验证私聊记忆提取功能

## 注意事项

1. **防抖机制**: 使用 `run_id` 作为 `group_chat_id` 的替代，确保每个私聊会话只提取一次
2. **每日上限**: 复用现有的每日 50 次上限
3. **错误处理**: 记忆提取失败不应影响 session 正常结束
4. **性能考虑**: 记忆提取在后台异步执行，不阻塞 session 结束

## 预计工作量

- 代码修改: ~50 行
- 测试验证: ~30 分钟
- 总计: ~1-2 小时

## 风险评估

- **低风险**: 记忆提取是后台任务，失败不影响核心功能
- **中风险**: 需要修改 session_actor 核心代码，需要仔细测试
- **缓解措施**: 充分测试，添加错误日志，确保幂等性