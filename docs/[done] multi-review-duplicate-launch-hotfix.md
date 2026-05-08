# [done] multi-review duplicate launch hotfix (2026-05-08)

## Summary

- Symptom: triggering the `multi-review` skill once could surface two AskUserQuestion / permission-style interaction flows, making it look like the review launched twice.
- Scope: frontend interaction layer only; no backend event routing or skill content changes were made.
- Resolution: explicitly mark interactive buttons in AskUserQuestion / elicitation cards as `type="button"` to prevent accidental extra default button behavior.

## Investigation

- Store-side permission prompt handling already has regression coverage preventing duplicate synthetic tool cards when prompt metadata is incomplete.
- The relevant permission response path in `src/routes/chat/+page.svelte` has a single IPC send path (`api.respondPermission(...)`).
- That made the frontend interaction layer the highest-probability source of duplicate submission.

## Changes

### Updated files

- `src/lib/components/InlineToolCard.svelte`
  - Added explicit `type="button"` to AskUserQuestion interactive controls, including multi-question option buttons, submit buttons, and deny buttons.
- `src/lib/components/ElicitationDialog.svelte`
  - Added explicit `type="button"` to elicitation action buttons and URL-open button.
- `docs/changelog.md`
  - Added a changelog entry for the hotfix.

## Why this fix

Buttons without an explicit type can inherit default submit semantics depending on surrounding markup and browser behavior. In a permission / elicitation interaction card, that can create an extra action path from a single user click. Making every action button explicit removes that ambiguity with minimal blast radius.

## Verification

### Passed

- `npm test -- src/lib/stores/session-store.test.ts src/lib/utils/resolve-permission.test.ts`
  - Result: 282 tests passed.

### Known unrelated issues

- `npm run check`
  - Still fails due to pre-existing repository issues already noted in project docs / CLAUDE context, including `CodeEditor.svelte` parse/export errors and the pre-existing `MessageKey` typing error in `src/routes/rooms/+page.svelte`.

## Notes

- A manual retest from another session no longer reproduced the duplicate-launch symptom after this change.
- This hotfix is intentionally narrow and avoids touching skill orchestration or backend permission event reducers.
