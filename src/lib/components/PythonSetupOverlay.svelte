<script lang="ts">
  import { onMount } from "svelte";
  import { getTransport } from "$lib/transport";

  interface SetupProgress {
    stage: "starting" | "verifying" | "ready" | "error";
    message: string;
    error?: string | null;
  }

  let visible = $state(false);
  let stage = $state<string>("");
  let message = $state<string>("");
  let errorMsg = $state<string | null>(null);
  let fadingOut = $state(false);

  function handleProgress(p: SetupProgress) {
    stage = p.stage;
    message = p.message;
    errorMsg = p.error ?? null;

    if (p.stage === "ready") {
      fadingOut = true;
      setTimeout(() => {
        visible = false;
        fadingOut = false;
      }, 600);
    } else {
      // "starting", "verifying", "error" all show the overlay
      visible = true;
      fadingOut = false;
    }
  }

  onMount(() => {
    const transport = getTransport();
    let unlisten: (() => void) | undefined;

    // Listen for real-time progress events
    transport
      .listen("python://setup-progress", (payload: unknown) => {
        handleProgress(payload as SetupProgress);
      })
      .then((fn) => {
        unlisten = fn;
      });

    // Poll current state on mount to handle race condition
    // (backend may have emitted before frontend mounted)
    transport.invoke("get_python_status", {}).then((status: unknown) => {
      const s = status as { ready: boolean; progress?: SetupProgress };
      if (s.progress) {
        handleProgress(s.progress);
      } else if (!s.ready) {
        // Python is still initializing but no progress event yet — show overlay
        visible = true;
        stage = "starting";
        message = "正在初始化 Python 环境...";
      }
    }).catch(() => {
      // Ignore poll errors — overlay stays hidden
    });

    return () => {
      unlisten?.();
    };
  });

  function retry() {
    // Emit retry event — backend will re-run bootstrap
    getTransport().invoke("get_python_status", {});
  }
</script>

{#if visible}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="python-overlay"
    class:fade-out={fadingOut}
    onkeydown={(e) => e.key === "Escape" && undefined}
  >
    <div class="overlay-card">
      <!-- Logo -->
      <div class="logo-container">
        <img src="/logo.png?v=2" alt="ClawGO" class="logo-img" />
        <span class="logo-text">Claw GO</span>
      </div>

      <!-- Status -->
      <div class="status-area">
        {#if stage === "error"}
          <div class="status-icon error">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <circle cx="12" cy="12" r="10" />
              <line x1="15" y1="9" x2="9" y2="15" />
              <line x1="9" y1="9" x2="15" y2="15" />
            </svg>
          </div>
          <p class="status-text error">{message}</p>
          {#if errorMsg}
            <p class="error-detail">{errorMsg}</p>
          {/if}
          <button class="retry-btn" onclick={retry}> 重试 </button>
        {:else}
          <div class="spinner"></div>
          <p class="status-text">{message || "正在初始化..."}</p>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .python-overlay {
    position: fixed;
    inset: 0;
    z-index: 9999;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.85);
    backdrop-filter: blur(12px);
    animation: fadeIn 0.3s ease-out;
  }

  .python-overlay.fade-out {
    animation: fadeOut 0.6s ease-in forwards;
  }

  .overlay-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2rem;
    padding: 3rem;
  }

  .logo-container {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.75rem;
  }

  .logo-img {
    width: 64px;
    height: 64px;
    border-radius: 16px;
    box-shadow: 0 0 40px rgba(255, 255, 255, 0.1);
  }

  .logo-text {
    font-size: 1.5rem;
    font-weight: 700;
    letter-spacing: 0.05em;
    color: #fff;
    text-shadow: 0 0 20px rgba(255, 255, 255, 0.2);
  }

  .status-area {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.75rem;
  }

  .spinner {
    width: 24px;
    height: 24px;
    border: 2.5px solid rgba(255, 255, 255, 0.2);
    border-top-color: rgba(255, 255, 255, 0.8);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  .status-text {
    font-size: 0.875rem;
    color: rgba(255, 255, 255, 0.7);
    text-align: center;
  }

  .status-text.error {
    color: #f87171;
  }

  .status-icon.error {
    width: 32px;
    height: 32px;
    color: #f87171;
  }

  .error-detail {
    font-size: 0.75rem;
    color: rgba(255, 255, 255, 0.4);
    max-width: 400px;
    text-align: center;
    word-break: break-all;
  }

  .retry-btn {
    margin-top: 0.5rem;
    padding: 0.5rem 1.5rem;
    font-size: 0.875rem;
    border-radius: 0.5rem;
    background: rgba(255, 255, 255, 0.1);
    color: #fff;
    border: 1px solid rgba(255, 255, 255, 0.2);
    cursor: pointer;
    transition: background 0.2s;
  }

  .retry-btn:hover {
    background: rgba(255, 255, 255, 0.2);
  }

  @keyframes fadeIn {
    from { opacity: 0; }
    to { opacity: 1; }
  }

  @keyframes fadeOut {
    from { opacity: 1; }
    to { opacity: 0; }
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
