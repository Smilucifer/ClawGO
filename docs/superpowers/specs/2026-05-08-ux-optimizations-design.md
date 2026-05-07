# UX Optimizations Design Spec

**Date:** 2026-05-08
**Scope:** 4 independent UX improvements

---

## 1. Chat Sidebar Preview — Show Latest Message

### Problem

`summarize_events()` in `src-tauri/src/storage/runs.rs` only parses the last 5 lines of `events.jsonl`. When a session ends with tool events (`ToolStart`, `ToolEnd`, `RunState`, `UsageUpdate`), the latest user/assistant message is pushed out of the 5-line window. The preview falls back to `run.prompt` (the earliest text), which is misleading.

The single-run `get_run()` command iterates all events and always finds the latest message — the two paths are inconsistent.

### Solution

Change `summarize_events()` to scan lines **in reverse** (from the end) and stop at the first `user_message` (bus event) or `user`/`assistant` (legacy event). This guarantees finding the latest message efficiently without scanning the entire file.

### Changes

| File | Change |
|------|--------|
| `src-tauri/src/storage/runs.rs` `summarize_events()` | Replace forward iteration of last-5-lines with reverse iteration; break on first message event found |

### Behavior

- `last_preview` now always reflects the most recent user or assistant message text, regardless of trailing tool events.
- `last_ts` and `msg_count` logic remain unchanged (still iterate all lines for count; last timestamp from final line).

---

## 2. Version Check — Fix GitHub URL

### Problem

`check_for_updates` in `src-tauri/src/commands/updates.rs` queries `https://api.github.com/repos/AnyiWang/OpenCovibe/releases/latest` (the original upstream repo). The user's repo is `Smilucifer/claude-session-hub-remaster`, which has its own releases (e.g., `v1.1.2`). The update check points to the wrong source.

### Solution

Change the `GITHUB_API_URL` constant to point to the user's repository.

### Changes

| File | Change |
|------|--------|
| `src-tauri/src/commands/updates.rs` line 7 | Change URL to `https://api.github.com/repos/Smilucifer/claude-session-hub-remaster/releases/latest` |

### Note

The `UpdateBanner.svelte` component, `checkForUpdates()` API wrapper, and `UpdateInfo` type all remain unchanged — only the backend URL changes.

---

## 3. Provider Switch — Auto-Update Default Model

### Problem

`handleProviderChange()` in `src/routes/chat/+page.svelte` (lines 237-271) has a conditional guard `else if (!store.model && ...)` that skips the model reset when `store.model` is already populated. This means:

1. User switches from Claude to DeepSeek → model correctly becomes `deepseek-v4-pro`
2. User switches back to Claude → model stays `deepseek-v4-pro` (guard fails because model is truthy)
3. User sends message → `startSession()` passes the wrong model to the Anthropic backend → error

The reference implementation `handlePlatformChange()` (lines 2302-2341) always overwrites `store.model` and works correctly.

### Solution

Rewrite the model assignment in `handleProviderChange()` to always overwrite `store.model`, using the same priority as `handlePlatformChange()`:

1. `cred.models[0]` (user-configured models from credentials)
2. `provider.defaultModel` (from `PHASE7_PROVIDERS`)
3. `getCliCurrentModel()` (for Anthropic/Claude)
4. `""` (empty fallback)

Also persist `active_platform_id` to settings, matching `handlePlatformChange()` behavior.

### Changes

| File | Change |
|------|--------|
| `src/routes/chat/+page.svelte` `handleProviderChange()` | Remove `!store.model` guard; always set `store.model` based on provider type; persist `active_platform_id` |

### Logic (pseudocode)

```
function handleProviderChange(providerId):
    if store.run: return
    provider = getPhase7Provider(providerId)
    store.agent = provider.executionAgent
    store.platformId = provider.platformId ?? (provider.id === "claude" ? "anthropic" : null)

    // Permission mode (unchanged)
    ...

    // Model assignment — always overwrite
    if provider.mode === "claude_compatible_api":
        cred = findCredential(settings.platform_credentials, provider.platformId)
        store.model = cred?.models?.[0] ?? provider.defaultModel ?? ""
    else if provider.executionAgent !== "claude":
        store.model = ""  // Codex etc. don't use a model field
    else:
        // Anthropic/Claude
        store.model = getCliCurrentModel() || settings?.default_model || ""

    // Persist platform selection
    api.updateUserSettings({ active_platform_id: store.platformId })

    // Async load (unchanged)
    loadAgentSettingsFor(provider)
```

---

## 4. Room Creation — Command Quick-Reference Banner

### Problem

After creating a new roundtable room, the UI shows an empty room with no guidance. Users don't know the available commands: `@debate`, `@summary @Name`, `/dm @Name msg`, `@Name msg`.

### Solution

Add a dismissible banner at the top of the room detail area showing a command quick-reference. The banner:

- Appears after room creation (when room is a roundtable and has no turns yet)
- Lists the 4 command patterns with brief descriptions
- Has a close (X) button
- Dismissal is session-only (uses `$state`, not persisted)
- Uses existing `animate-toast-in` / `animate-toast-out` CSS animations from `app.css`

### Changes

| File | Change |
|------|--------|
| `src/routes/rooms/+page.svelte` | Add `showCommandHint` state; add banner UI above room detail; set `showCommandHint = true` after `handleCreateRoundtable()` |
| `messages/en.json` | Add i18n keys for banner title, command descriptions, dismiss |
| `messages/zh-CN.json` | Add corresponding Chinese translations |

### Banner Content

```
Command Quick Reference:
  @debate [topic]     — All participants debate
  @summary @Name      — Named participant summarizes
  /dm @Name message   — Private turn (hidden from others)
  @Name message       — Public turn to one participant
                    [X]
```

### i18n Keys (draft)

```json
{
  "room_commandHint_title": "Command Quick Reference",
  "room_commandHint_debate": "@debate [topic] — All participants debate",
  "room_commandHint_summary": "@summary @Name — Named participant summarizes",
  "room_commandHint_dm": "/dm @Name message — Private turn",
  "room_commandHint_target": "@Name message — Public turn to one participant",
  "room_commandHint_dismiss": "Dismiss"
}
```

---

## Implementation Order

These 4 changes are independent and can be implemented in any order. Suggested order by complexity (simplest first):

1. **Version check URL fix** — 1 line change
2. **Chat preview reverse scan** — localized Rust change
3. **Provider model auto-switch** — frontend logic fix
4. **Room command banner** — new UI + i18n

## Testing

- **Preview**: Create a run, send messages, verify sidebar shows latest message text (not initial prompt).
- **Version check**: Verify `check_for_updates` returns the correct latest version from the user's repo.
- **Provider switch**: Switch between Claude → DeepSeek → Claude and verify model resets correctly each time.
- **Room banner**: Create a new roundtable room, verify banner appears; close it, verify it doesn't reappear in the same session.
