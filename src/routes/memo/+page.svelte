<script lang="ts">
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import { t } from "$lib/i18n/index.svelte";

  onMount(() => {
    // Redirect to chat after a brief moment so the user sees the message
    const timer = setTimeout(() => goto("/chat", { replaceState: true }), 800);
    return () => clearTimeout(timer);
  });
</script>

<div class="flex h-full items-center justify-center bg-background">
  <div class="text-center">
    <p class="text-sm text-muted-foreground">{t("memo_redirectHint")}</p>
  </div>
</div>
