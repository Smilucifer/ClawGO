<script lang="ts">
  let {
    width = 600,
    onResize = (_w: number) => {},
    minChatWidth = 400,
    minPreviewWidth = 320,
  }: {
    width: number;
    onResize?: (width: number) => void;
    minChatWidth?: number;
    minPreviewWidth?: number;
  } = $props();

  let dragging = $state(false);
  let startX = 0;
  let startWidth = 0;
  let containerEl: HTMLElement | undefined;

  function onPointerDown(e: PointerEvent) {
    e.preventDefault();
    dragging = true;
    startX = e.clientX;
    startWidth = width;
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    containerEl = (e.target as HTMLElement).parentElement ?? undefined;
  }

  function onPointerMove(e: PointerEvent) {
    if (!dragging) return;
    const dx = e.clientX - startX;
    const containerWidth = containerEl?.clientWidth ?? window.innerWidth;
    const maxPreview = containerWidth - minChatWidth;
    const newWidth = Math.min(maxPreview, Math.max(minPreviewWidth, startWidth + dx));
    onResize(newWidth);
  }

  function onPointerUp(_e: PointerEvent) {
    dragging = false;
  }

  function onDoubleClick() {
    const containerWidth = containerEl?.clientWidth ?? window.innerWidth;
    const half = Math.floor(containerWidth / 2);
    onResize(Math.max(minPreviewWidth, Math.min(half, containerWidth - minChatWidth)));
  }
</script>

<div
  class="relative w-1.5 cursor-col-resize flex-shrink-0 group hover:bg-primary/30 transition-colors {dragging ? 'bg-primary/50' : 'bg-border/50'}"
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={onPointerUp}
  onpointercancel={onPointerUp}
  ondblclick={onDoubleClick}
  role="separator"
  aria-orientation="vertical"
  aria-valuenow={width}
  tabindex="-1"
>
  <div class="absolute inset-y-0 -left-1 -right-1"></div>
  <div class="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 opacity-0 group-hover:opacity-100 transition-opacity">
    <div class="w-1 h-6 rounded-full bg-primary/60"></div>
  </div>
</div>
