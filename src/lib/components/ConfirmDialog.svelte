<script lang="ts">
  import Modal from './Modal.svelte';
  import { t } from '$lib/i18n/index.svelte';

  let {
    open = $bindable(false),
    title = '',
    message = '',
    confirmLabel = '',
    cancelLabel = '',
    variant = 'danger' as 'danger' | 'default',
    onConfirm,
    onCancel,
  }: {
    open?: boolean;
    title?: string;
    message?: string;
    confirmLabel?: string;
    cancelLabel?: string;
    variant?: 'danger' | 'default';
    onConfirm?: () => void | Promise<void>;
    onCancel?: () => void;
  } = $props();

  async function handleConfirm() {
    await onConfirm?.();
    open = false;
  }

  function handleCancel() {
    onCancel?.();
    open = false;
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape' && open) {
      e.preventDefault();
      handleCancel();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<Modal bind:open title={title || t('invest_confirm_title')}>
  <p class="text-sm text-muted-foreground mb-6">{message}</p>
  <div class="flex justify-end gap-3">
    <button
      class="rounded border border-border px-4 py-2 text-sm text-muted-foreground hover:bg-muted"
      onclick={handleCancel}
    >
      {cancelLabel || t('invest_cancel')}
    </button>
    <button
      class="rounded px-4 py-2 text-sm text-primary-foreground {variant === 'danger' ? 'bg-destructive hover:bg-destructive/90' : 'bg-primary hover:bg-primary/90'}"
      onclick={handleConfirm}
    >
      {confirmLabel || t('invest_confirm')}
    </button>
  </div>
</Modal>
