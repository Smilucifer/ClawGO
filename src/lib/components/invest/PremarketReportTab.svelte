<script lang="ts">
  /**
   * 盘前观察 tab — reads the latest premarket report JSON via Tauri commands
   * and renders the four-section long image (`#report-canvas`, width var --report-w).
   *
   * Generation state lives in premarket-store (survives tab switches); toolbar shows
   * elapsed time. 导出 PNG/PDF (html2canvas + jsPDF) 走 Tauri save() + write_binary_export.
   * Collapsible settings panel edits the 4 weights + 3 thresholds via
   * `save_premarket_config_cmd`.
   *
   * Class names / tokens are copied 1:1 from
   * `docs/ui-demo/premarket-report-demo.html`.
   */
  import { onMount } from 'svelte';
  import html2canvas from 'html2canvas';
  import { jsPDF } from 'jspdf';
  import { getTransport } from '$lib/transport';
  import { t } from '$lib/i18n/index.svelte';
  import { premarketStore } from '$lib/stores/premarket-store.svelte';

  // ── Types (mirrors src-tauri/src/invest/premarket/*) ─────────────
  type Grade = 'S' | 'A' | 'B' | 'C';
  interface FactorBreakdown {
    sentiment: number;
    capital: number;
    technical: number;
    catalyst: number;
    sector_strength: number;
  }
  interface SymbolScore {
    symbol: string;
    name: string;
    total: number;
    grade: Grade;
    factors: FactorBreakdown;
    missingFactors: string[];
  }
  interface AiSector {
    name: string;
    tag: string; // 新闻强 / 催化强 / 情绪强 / 分歧大 / 风险预警
    count: number;
    note: string;
  }
  interface AiCommentary {
    sectors: AiSector[];
    tone: string;
  }
  interface MacroSnapshot {
    shCompositeClose: number | null;
    shCompositeVol20: number | null;
    northboundNet: number | null;
    vix: number | null;
    gold: number | null;
    advanceCount: number | null;
    declineCount: number | null;
    twoMarketVolume: number | null;
    limitUpCount: number | null;
    limitDownCount: number | null;
    upOver3pctCount: number | null;
    flatCount: number | null;
  }
  interface PremarketConfig {
    weight_sentiment: number;
    weight_capital: number;
    weight_technical: number;
    weight_catalyst: number;
    weight_sector: number;
    weight_source: 'auto' | 'manual';
    threshold_s: number;
    threshold_a: number;
    threshold_b: number;
  }
  // 02 段：板块资金流入榜 + 拥挤度雷达 —— 与后端 SectorFlowEntry 对齐
  type CrowdLevel = 'healthy' | 'warm' | 'hot';
  interface SectorFlowEntry {
    name: string;
    netInflow: number; // 亿
    changePct: number | null;
    totalTurnover: number | null;
    advanceCount: number | null;
    declineCount: number | null;
    leadStock: string | null;
    leadChangePct: number | null;
    crowdLevel: CrowdLevel;
    crowdInputs: { turnoverPct: number; volumeShare: number; divergence: number };
    barWidth: number; // 0-100，按 Top1 归一
    source: string;
  }
  // 03 段：主线（行业主题）排序 —— 与后端 ThemeEntry 对齐
  interface ThemeEntry {
    rank: number;
    name: string;
    memberCount: number;
    sentiment: number; // 0-100
    capital: number; // 0-100
    catalyst: number; // 0-100
    technical: number; // 0-100
    total: number; // 0-100
    grade: Grade;
    reason: string;
  }
  interface ReportPayload {
    date: string;
    markdown: string | null;
    json: {
      date: string;
      macro: MacroSnapshot | null;
      scores: SymbolScore[];
      config: PremarketConfig;
      aiCommentary: AiCommentary | null;
      sectorFlows?: SectorFlowEntry[] | null;
      themes?: ThemeEntry[] | null;
    } | null;
  }

  const invoke = <T,>(cmd: string, args?: Record<string, unknown>) =>
    getTransport().invoke<T>(cmd, args);

  // ── State ─────────────────────────────────────────────────────────
  let report = $state<ReportPayload | null>(null);
  let latestDate = $state<string | null>(null);
  const generating = $derived(premarketStore.generating);
  let nowTick = $state(Date.now());
  const elapsedSec = $derived(
    generating
      ? Math.floor((nowTick - premarketStore.startedAt) / 1000)
      : Math.round(premarketStore.lastElapsedMs / 1000),
  );
  let loading = $state(false);
  let errorMsg = $state<string | null>(null);
  let exportingPng = $state(false);
  let exportingPdf = $state(false);

  // Settings panel
  let settingsOpen = $state(false);
  let cfg = $state<PremarketConfig>({
    weight_sentiment: 0.25,
    weight_capital: 0.25,
    weight_technical: 0.20,
    weight_catalyst: 0.15,
    weight_sector: 0.15,
    weight_source: 'auto',
    threshold_s: 78,
    threshold_a: 62,
    threshold_b: 45,
  });
  let cfgLoaded = $state(false);
  let cfgSaving = $state(false);
  let cfgSaveMsg = $state<string | null>(null);

  const weightSum = $derived(
    cfg.weight_sentiment + cfg.weight_capital + cfg.weight_technical + cfg.weight_catalyst + cfg.weight_sector,
  );
  const weightSumOk = $derived(Math.abs(weightSum - 1.0) <= 0.001);
  const thresholdsOk = $derived(cfg.threshold_s > cfg.threshold_a && cfg.threshold_a > cfg.threshold_b);
  const cfgValid = $derived(weightSumOk && thresholdsOk);

  // ── Derived views for the 4 sections ─────────────────────────────
  const scores = $derived<SymbolScore[]>(report?.json?.scores ?? []);
  const macro = $derived<MacroSnapshot | null>(report?.json?.macro ?? null);
  const commentary = $derived<AiCommentary | null>(report?.json?.aiCommentary ?? null);

  const hasRisk = $derived(
    !!commentary && commentary.sectors.some((s) => s.tag.includes('风险')),
  );
  const wallClass = $derived.by(() => {
    if (!commentary) return 'wall-3col';
    const n = commentary.sectors.length;
    if (hasRisk) return 'wall-3col';
    if (n <= 4) return `wall-n${n}`;
    if (n === 5) return 'wall-1plus4';
    return 'wall-3col';
  });

  // Grouped by grade — cap 5 per bucket (server assigns grade by rank).
  const grouped = $derived.by(() => {
    const buckets: Record<Grade, SymbolScore[]> = { S: [], A: [], B: [], C: [] };
    for (const s of scores) buckets[s.grade].push(s);
    (['S', 'A', 'B', 'C'] as const).forEach((g) =>
      buckets[g].sort((a, b) => b.total - a.total),
    );
    return {
      S: buckets.S.slice(0, 5),
      A: buckets.A.slice(0, 5),
      B: buckets.B.slice(0, 5),
      C: buckets.C.slice(0, 5),
    };
  });

  // Simple sector "main-lines" ordering from aiCommentary (fallback if empty).
  const mainLines = $derived<AiSector[]>(commentary?.sectors ?? []);

  // 02 段真实数据（后端 Top10，按 net_inflow 降序）
  const sectorFlows = $derived<SectorFlowEntry[]>(report?.json?.sectorFlows ?? []);
  // 03 段真实数据（后端 Top5，按 total 降序，rank 已回填）
  const themes = $derived<ThemeEntry[]>(report?.json?.themes ?? []);

  // ── Actions ───────────────────────────────────────────────────────
  async function loadConfig() {
    try {
      const c = await invoke<PremarketConfig>('get_premarket_config_cmd');
      cfg = { ...c };
      cfgLoaded = true;
    } catch (e) {
      console.error('[premarket] loadConfig:', e);
    }
  }

  async function loadLatest() {
    loading = true;
    errorMsg = null;
    try {
      const dates = await invoke<string[]>('list_premarket_reports', { limit: 1 });
      if (dates.length === 0) {
        report = null;
        latestDate = null;
        return;
      }
      latestDate = dates[0];
      report = await invoke<ReportPayload>('read_premarket_report', { date: dates[0] });
    } catch (e) {
      console.error('[premarket] loadLatest:', e);
      errorMsg = String(e);
    } finally {
      loading = false;
    }
  }

  async function generate() {
    errorMsg = null;
    await premarketStore.generate();
  }

  // 秒表:生成中每秒推进 nowTick(驱动 elapsedSec 重算)
  $effect(() => {
    if (!premarketStore.generating) return;
    const h = setInterval(() => { nowTick = Date.now(); }, 1000);
    return () => clearInterval(h);
  });

  // 生成完成(completionSeq 变化)→ 刷新报告 + 回填错误
  let lastSeq = premarketStore.completionSeq;
  $effect(() => {
    const seq = premarketStore.completionSeq;
    if (seq !== lastSeq) {
      lastSeq = seq;
      if (premarketStore.lastError) errorMsg = premarketStore.lastError;
      else loadLatest();
    }
  });

  // 公共导出流程:截图 report-canvas → 转 base64 → Tauri save() 落盘。
  // toBase64 负责把 canvas 转成对应格式的 base64(去掉 data-URI 头)。
  async function captureAndSave(ext: 'png' | 'pdf', toBase64: (c: HTMLCanvasElement) => string) {
    const el = document.getElementById('report-canvas');
    if (!el) return;
    const canvas = await html2canvas(el, {
      scale: 2,
      backgroundColor: getComputedStyle(el).backgroundColor || '#1a1918',
      useCORS: true,
    });
    const base64 = toBase64(canvas);
    const { save } = await import('@tauri-apps/plugin-dialog');
    const path = await save({
      defaultPath: `premarket_${report?.date ?? latestDate ?? 'report'}.${ext}`,
      filters: [{ name: ext.toUpperCase(), extensions: [ext] }],
    });
    if (path) await invoke<void>('write_binary_export', { path, base64 });
  }

  async function exportPng() {
    exportingPng = true;
    try {
      await captureAndSave('png', (c) => c.toDataURL('image/png').split(',')[1]);
    } catch (e) {
      console.error('[premarket] exportPng:', e);
      errorMsg = String(e);
    } finally {
      exportingPng = false;
    }
  }

  async function exportPdf() {
    exportingPdf = true;
    try {
      await captureAndSave('pdf', (c) => {
        const pdf = new jsPDF({
          unit: 'px',
          format: [c.width, c.height],
          orientation: c.width >= c.height ? 'landscape' : 'portrait',
        });
        pdf.addImage(c.toDataURL('image/png'), 'PNG', 0, 0, c.width, c.height);
        return pdf.output('datauristring').split(',')[1];
      });
    } catch (e) {
      console.error('[premarket] exportPdf:', e);
      errorMsg = String(e);
    } finally {
      exportingPdf = false;
    }
  }

  async function saveConfig() {
    if (!cfgValid) return;
    cfgSaving = true;
    cfgSaveMsg = null;
    try {
      await invoke<void>('save_premarket_config_cmd', { config: cfg });
      cfgSaveMsg = t('invest_premarket_cfg_saved');
    } catch (e) {
      cfgSaveMsg = String(e);
    } finally {
      cfgSaving = false;
    }
  }

  onMount(() => {
    loadConfig();
    loadLatest();
  });

  // ── Helpers ───────────────────────────────────────────────────────
  function fmtNum(v: number | null | undefined, digits = 2): string {
    if (v === null || v === undefined || !Number.isFinite(v)) return '—';
    return v.toFixed(digits);
  }
  function fmtInt(v: number | null | undefined): string {
    if (v === null || v === undefined || !Number.isFinite(v)) return '—';
    return String(Math.round(v));
  }
  function evalClass(tag: string): string {
    // Demo tags: 新闻强 / 催化强 / 情绪强 / 分歧大 / 风险预警
    if (tag.includes('新闻')) return 'news';
    if (tag.includes('催化')) return 'cata';
    if (tag.includes('情绪')) return 'mood';
    if (tag.includes('分歧')) return 'split';
    if (tag.includes('风险')) return 'risk';
    return 'mood';
  }
  function gradeVar(g: Grade): string {
    return { S: 'var(--grade-s)', A: 'var(--grade-a)', B: 'var(--grade-b)', C: 'var(--grade-c)' }[g];
  }
  function factorPct(v: number): number {
    // Factor scores are 0-100 raw; clamp for bar.
    return Math.max(0, Math.min(100, v));
  }

  const factorLabelSentiment = $derived(t('invest_premarket_factor_sentiment'));
  const factorLabelCapital = $derived(t('invest_premarket_factor_capital'));
  const factorLabelCatalyst = $derived(t('invest_premarket_factor_catalyst'));
</script>

<div class="premarket-tab" data-invest-scope>
  <!-- Toolbar -->
  <div class="toolbar">
    <button class="btn primary" onclick={generate} disabled={generating || loading}>
      {generating
        ? `${t('invest_premarket_elapsed_running')} ${elapsedSec}s`
        : t('invest_premarket_generate_now')}
    </button>
    {#if !generating && premarketStore.lastElapsedMs > 0}
      <span class="elapsed-note">{t('invest_premarket_elapsed')} {elapsedSec}s</span>
    {/if}
    <button class="btn" onclick={exportPng} disabled={!report || exportingPng}>
      {exportingPng ? '...' : t('invest_premarket_export_png')}
    </button>
    <button class="btn" onclick={exportPdf} disabled={!report || exportingPdf}>
      {exportingPdf ? '...' : t('invest_premarket_export_pdf')}
    </button>
    <button class="btn subtle" onclick={() => (settingsOpen = !settingsOpen)}>
      {settingsOpen ? t('invest_premarket_settings_hide') : t('invest_premarket_settings_show')}
    </button>
  </div>

  <!-- Settings panel (collapsible) -->
  {#if settingsOpen}
    <div class="settings-panel">
      <div class="settings-title">{t('invest_premarket_settings_title')}</div>
      <div class="settings-desc">{t('invest_premarket_settings_desc')}</div>

      <div class="settings-grid">
        <label class="settings-item">
          <span>{t('invest_premarket_factor_sentiment')} · w</span>
          <input type="number" step="0.01" min="0" max="1" bind:value={cfg.weight_sentiment} />
        </label>
        <label class="settings-item">
          <span>{t('invest_premarket_factor_capital')} · w</span>
          <input type="number" step="0.01" min="0" max="1" bind:value={cfg.weight_capital} />
        </label>
        <label class="settings-item">
          <span>{t('invest_premarket_factor_technical')} · w</span>
          <input type="number" step="0.01" min="0" max="1" bind:value={cfg.weight_technical} />
        </label>
        <label class="settings-item">
          <span>{t('invest_premarket_factor_catalyst')} · w</span>
          <input type="number" step="0.01" min="0" max="1" bind:value={cfg.weight_catalyst} />
        </label>
        <label class="settings-item">
          <span>{t('invest_premarket_factor_sector')} · w</span>
          <input
            type="number"
            step="0.01"
            min="0"
            max="1"
            bind:value={cfg.weight_sector}
            disabled={cfg.weight_source === 'auto'}
          />
        </label>
        <label class="settings-item">
          <span>{t('invest_premarket_weight_source')}</span>
          <select bind:value={cfg.weight_source}>
            <option value="auto">{t('invest_premarket_weight_source_auto')}</option>
            <option value="manual">{t('invest_premarket_weight_source_manual')}</option>
          </select>
        </label>
        <label class="settings-item">
          <span>{t('invest_premarket_threshold_s')}</span>
          <input type="number" step="0.5" bind:value={cfg.threshold_s} />
        </label>
        <label class="settings-item">
          <span>{t('invest_premarket_threshold_a')}</span>
          <input type="number" step="0.5" bind:value={cfg.threshold_a} />
        </label>
        <label class="settings-item">
          <span>{t('invest_premarket_threshold_b')}</span>
          <input type="number" step="0.5" bind:value={cfg.threshold_b} />
        </label>
      </div>

      <div class="settings-hint">
        <span class:bad={!weightSumOk} class:ok={weightSumOk}>
          {t('invest_premarket_weight_sum')}: {weightSum.toFixed(3)} {weightSumOk ? '✓' : '≠ 1.000'}
        </span>
        <span class:bad={!thresholdsOk} class:ok={thresholdsOk}>
          {t('invest_premarket_threshold_order')}: S &gt; A &gt; B {thresholdsOk ? '✓' : '✗'}
        </span>
      </div>

      <div class="settings-actions">
        <button
          class="btn primary"
          disabled={!cfgValid || cfgSaving || !cfgLoaded}
          onclick={saveConfig}
        >
          {cfgSaving ? '...' : t('invest_premarket_settings_save')}
        </button>
        {#if cfgSaveMsg}
          <span class="save-msg">{cfgSaveMsg}</span>
        {/if}
      </div>
    </div>
  {/if}

  {#if errorMsg}
    <div class="err-strip">{errorMsg}</div>
  {/if}

  <!-- Report canvas (width var --report-w, export target) -->
  {#if !report}
    <div class="empty">
      {loading ? t('invest_loading') : t('invest_premarket_empty')}
    </div>
  {:else}
    <div id="report-canvas">
      <!-- 报告头 -->
      <div class="report-head">
        <div>
          <div class="title">{t('invest_premarket_title')}</div>
          <div class="subtitle">{t('invest_premarket_subtitle')}</div>
        </div>
        <div class="head-spacer"></div>
        <div class="head-right">
          <span class="session-badge">{t('invest_premarket_session_badge')}</span>
          <span class="next-day">{t('invest_premarket_next_day')} {report.date}</span>
        </div>
      </div>

      <!-- 01 舆论/新闻先验 -->
      <div class="section">
        <div class="section-head">
          <span class="section-no">01</span>
          <span class="section-title">{t('invest_premarket_sec1_title')}</span>
          <span class="section-tag">{t('invest_premarket_sec1_tag')}</span>
        </div>

        {#if commentary && commentary.sectors.length > 0}
          <div class="theme-wall {wallClass}">
            {#each commentary.sectors as sec, i}
              <div
                class="theme-tag-card"
                class:ttc-first={i === 0 && wallClass === 'wall-1plus4'}
                style={sec.tag.includes('风险') ? 'grid-column: 1 / -1;' : ''}
              >
                <div class="ttc-head">
                  <span class="ttc-name">{sec.name}</span>
                  <span class="eval-tag {evalClass(sec.tag)}">{sec.tag}</span>
                  <span class="ttc-count">{sec.count} {t('invest_premarket_news_count_unit')}</span>
                </div>
                <div class="ttc-desc">{sec.note}</div>
              </div>
            {/each}
          </div>
          <div class="ai-note">
            <span class="ai-tag">AI</span>{commentary.tone}
          </div>
        {:else}
          <div class="ai-note placeholder">
            <span class="ai-tag">AI</span>{t('invest_premarket_ai_missing')}
          </div>
        {/if}
      </div>

      <!-- 02 资金与宏观 -->
      <div class="section">
        <div class="section-head">
          <span class="section-no">02</span>
          <span class="section-title">{t('invest_premarket_sec2_title')}</span>
          <span class="section-tag">{t('invest_premarket_sec2_tag')}</span>
        </div>

        <div class="macro-sub">{t('invest_premarket_macro_indicators')}</div>
        <div class="macro-grid">
          <div class="macro-cell">
            <span class="macro-label">{t('invest_premarket_macro_sh_composite')}</span>
            <span class="macro-value">{fmtNum(macro?.shCompositeClose, 2)}</span>
          </div>
          <div class="macro-cell">
            <span class="macro-label">{t('invest_premarket_macro_northbound')}</span>
            <span
              class="macro-value"
              class:up={(macro?.northboundNet ?? 0) > 0}
              class:down={(macro?.northboundNet ?? 0) < 0}
            >
              {fmtNum(macro?.northboundNet, 2)}
            </span>
          </div>
          <div class="macro-cell">
            <span class="macro-label">{t('invest_premarket_macro_volume')}</span>
            <span class="macro-value">{fmtInt(macro?.twoMarketVolume)}</span>
          </div>
          <div class="macro-cell">
            <span class="macro-label">{t('invest_premarket_macro_vix')}</span>
            <span
              class="macro-value"
              class:up={(macro?.vix ?? 0) > 25}
              class:down={(macro?.vix ?? 0) > 0 && (macro?.vix ?? 0) < 15}
            >{fmtNum(macro?.vix, 2)}</span>
          </div>
          <div class="macro-cell">
            <span class="macro-label">{t('invest_premarket_macro_vol20')}</span>
            <span class="macro-value">{fmtNum(macro?.shCompositeVol20, 2)}</span>
          </div>
        </div>

        <div class="macro-sub">{t('invest_premarket_market_breadth')}</div>
        <div class="macro-grid">
          <div class="macro-cell">
            <span class="macro-label">{t('invest_premarket_breadth_apd')}</span>
            <span class="apd">
              <span class="u">{fmtInt(macro?.advanceCount)}</span><span class="sep">-</span><span class="p">{fmtInt(macro?.flatCount)}</span><span class="sep">-</span><span class="d">{fmtInt(macro?.declineCount)}</span>
            </span>
          </div>
          <div class="macro-cell">
            <span class="macro-label">{t('invest_premarket_breadth_limit_up')}</span>
            <span class="macro-value up">{fmtInt(macro?.limitUpCount)}</span>
          </div>
          <div class="macro-cell">
            <span class="macro-label">{t('invest_premarket_breadth_limit_down')}</span>
            <span class="macro-value down">{fmtInt(macro?.limitDownCount)}</span>
          </div>
          <div class="macro-cell">
            <span class="macro-label">{t('invest_premarket_breadth_up3')}</span>
            <span class="macro-value">{fmtInt(macro?.upOver3pctCount)}</span>
          </div>
          <div class="macro-cell">
            <span class="macro-label">{t('invest_premarket_breadth_mood')}</span>
            <span class="macro-value">
              {#if macro && macro.advanceCount != null && macro.declineCount != null && (macro.advanceCount + macro.declineCount) > 0}
                {((macro.advanceCount / (macro.advanceCount + macro.declineCount + (macro.flatCount ?? 0))) * 100).toFixed(1)}%
              {:else}—{/if}
            </span>
          </div>
        </div>

        <div class="money-divider"></div>
        {#if macro}
          {@const northUp = (macro.northboundNet ?? 0) > 0}
          {@const limitUpRich = (macro.limitUpCount ?? 0) >= 40}
          {@const hot = northUp && limitUpRich}
          <div class="money-strip">
            <span class="me-title">{t('invest_premarket_money_effect')}</span>
            <span class="me-badge {hot ? 'hot' : 'active'}">
              {hot ? t('invest_premarket_money_hot') : t('invest_premarket_money_active')}
            </span>
            <span class="reason">
              {t('invest_premarket_money_reason_north')}: {fmtNum(macro.northboundNet, 1)} ·
              {t('invest_premarket_money_reason_limits')}: {fmtInt(macro.limitUpCount)} / {fmtInt(macro.limitDownCount)}
            </span>
          </div>
        {:else}
          <div class="money-strip placeholder">
            <span class="me-title">{t('invest_premarket_money_effect')}</span>
            <span class="reason">{t('invest_premarket_macro_missing')}</span>
          </div>
        {/if}

        <!-- 板块资金流入榜 · 拥挤度雷达 -->
        <div class="macro-sub">{t('invest_premarket_sector_flow_title')}</div>
        {#if sectorFlows.length > 0}
          <div class="sector-flow">
            {#each sectorFlows as sf}
              {@const isIn = sf.netInflow >= 0}
              {@const dir = isIn ? 'in' : 'out'}
              <div class="sector-row">
                <span class="sf-name">{sf.name}</span>
                <span class="sf-flow {dir}">
                  {isIn ? '+' : ''}{sf.netInflow.toFixed(1)}{t('invest_premarket_unit_yi')}
                </span>
                <div class="sf-bar-track">
                  <div class="sf-bar-fill {dir}" style="width:{Math.max(2, sf.barWidth)}%"></div>
                </div>
                <span class="crowd-badge {sf.crowdLevel}">
                  {sf.crowdLevel === 'hot'
                    ? t('invest_premarket_crowd_hot')
                    : sf.crowdLevel === 'warm'
                      ? t('invest_premarket_crowd_warm')
                      : t('invest_premarket_crowd_healthy')}
                </span>
              </div>
            {/each}
          </div>
          <div class="radar-hint">
            <span>{t('invest_premarket_radar_formula')}</span>
            <span class="legend"><span class="dot dot-healthy"></span>{t('invest_premarket_crowd_healthy')}</span>
            <span class="legend"><span class="dot dot-warm"></span>{t('invest_premarket_crowd_warm')}</span>
            <span class="legend"><span class="dot dot-hot"></span>{t('invest_premarket_crowd_hot_full')}</span>
          </div>
        {:else}
          <div class="empty-inline">{t('invest_premarket_sector_flow_missing')}</div>
        {/if}
      </div>

      <!-- 03 主线排序 -->
      <div class="section">
        <div class="section-head">
          <span class="section-no">03</span>
          <span class="section-title">{t('invest_premarket_sec3_title')}</span>
          <span class="section-tag">{t('invest_premarket_sec3_tag')}</span>
        </div>

        {#if themes.length > 0}
          {#each themes as th}
            <div class="theme-row">
              <span class="theme-rank">{th.rank}</span>
              <div class="theme-main">
                <span class="theme-name">{th.name}</span>
                <span class="theme-reason">{th.reason}</span>
              </div>
              <div class="theme-bars">
                <div class="mini-bar">
                  <span class="mb-label">{t('invest_premarket_bar_news')}</span>
                  <div class="mb-track"><div class="mb-fill" style="width:{factorPct(th.sentiment)}%"></div></div>
                </div>
                <div class="mini-bar">
                  <span class="mb-label">{t('invest_premarket_bar_capital')}</span>
                  <div class="mb-track"><div class="mb-fill" style="width:{factorPct(th.capital)}%"></div></div>
                </div>
                <div class="mini-bar">
                  <span class="mb-label">{t('invest_premarket_bar_catalyst')}</span>
                  <div class="mb-track"><div class="mb-fill" style="width:{factorPct(th.catalyst)}%"></div></div>
                </div>
              </div>
              <span class="theme-score" style="color:{gradeVar(th.grade)}">{Math.round(th.total)}</span>
            </div>
          {/each}
        {:else if mainLines.length > 0}
          <!-- Fallback: 后端主线未产出（观察池无行业映射），用 AI 舆情板块占位 -->
          {#each mainLines.slice(0, 5) as line, i}
            <div class="theme-row">
              <span class="theme-rank">{i + 1}</span>
              <div class="theme-main">
                <span class="theme-name">{line.name}</span>
                <span class="theme-reason">{line.note}</span>
              </div>
              <span class="theme-score" style="color:var(--text-tertiary)">—</span>
            </div>
          {/each}
        {:else}
          <div class="empty-inline">{t('invest_premarket_mainlines_missing')}</div>
        {/if}
      </div>

      <!-- 04 SABC 观察池 -->
      <div class="section">
        <div class="section-head">
          <span class="section-no">04</span>
          <span class="section-title">{t('invest_premarket_sec4_title')}</span>
          <span class="section-tag">{t('invest_premarket_sec4_tag')}</span>
        </div>

        <div class="pool-grid">
          {#each [
            { grade: 'S' as Grade, label: t('invest_premarket_pool_s_label'), sub: t('invest_premarket_pool_s_sub'), rows: grouped.S },
            { grade: 'A' as Grade, label: t('invest_premarket_pool_a_label'), sub: t('invest_premarket_pool_a_sub'), rows: grouped.A },
            { grade: 'B' as Grade, label: t('invest_premarket_pool_b_label'), sub: t('invest_premarket_pool_b_sub'), rows: grouped.B },
            { grade: 'C' as Grade, label: t('invest_premarket_pool_c_label'), sub: t('invest_premarket_pool_c_sub'), rows: grouped.C },
          ] as bucket}
            <div class="pool-box grade-{bucket.grade.toLowerCase()}">
              <div class="pool-head">
                <span class="grade-badge">{bucket.grade}</span>
                <span class="pool-label">{bucket.label}</span>
                <span class="pool-sub">{bucket.sub}</span>
              </div>
              <div class="pool-body">
                {#if bucket.rows.length === 0}
                  <div class="pool-empty">{t('invest_premarket_pool_empty')}</div>
                {:else}
                  {#each bucket.rows as s}
                    <div class="stock-row">
                      <span class="stk-name">{s.name || s.symbol}</span>
                      <span class="stk-code">{s.symbol}</span>
                      <span class="stk-spacer"></span>
                      <span class="stk-tags">
                        {#if s.factors.capital >= 60}
                          <span class="stk-tag money">{t('invest_premarket_tag_money')}</span>
                        {/if}
                        {#if s.factors.sentiment >= 60}
                          <span class="stk-tag mood">{t('invest_premarket_tag_mood')}</span>
                        {/if}
                        {#if s.factors.catalyst >= 60}
                          <span class="stk-tag">{t('invest_premarket_tag_catalyst')}</span>
                        {/if}
                        {#if s.factors.technical >= 60}
                          <span class="stk-tag">{t('invest_premarket_tag_tech')}</span>
                        {/if}
                        {#if s.factors.sector_strength >= 60}
                          <span class="stk-tag">{t('invest_premarket_tag_sector')}</span>
                        {/if}
                      </span>
                      <span class="stk-score">{s.total.toFixed(0)}</span>
                    </div>
                  {/each}
                {/if}
              </div>
            </div>
          {/each}
        </div>
      </div>

      <!-- footer -->
      <div class="report-foot">
        <span class="disclaimer">
          {t('invest_premarket_disclaimer_prefix')}
          <span class="brand">openInvest</span>
          {t('invest_premarket_disclaimer_suffix')}
        </span>
      </div>
    </div>
  {/if}
</div>

<style>
  /* Local visual language mirrors docs/ui-demo/premarket-report-demo.html.
     All tokens (--bg-*, --text-*, --accent, --up/--down/--flat, --grade-*)
     come from the [data-invest-scope] scope in src/app.css. */

  .premarket-tab {
    --report-w: 1080px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-3);
    padding-bottom: var(--space-4);
  }

  .toolbar {
    display: flex;
    gap: var(--space-2);
    width: var(--report-w);
  }
  .btn {
    flex: 1;
    font-size: 13px;
    font-weight: 600;
    padding: 9px var(--space-4);
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background: var(--bg-card);
    color: var(--text-primary);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    transition: border-color .12s ease, opacity .12s ease;
  }
  .btn.primary { background: var(--accent); color: var(--bg-base); border-color: var(--accent); }
  .btn.subtle  { flex: 0 0 auto; padding: 9px 14px; }
  .btn:hover:not(:disabled) { border-color: var(--accent); }
  .btn:disabled { opacity: .5; cursor: not-allowed; }
  .elapsed-note {
    align-self: center;
    font-size: 11px;
    color: var(--text-tertiary);
    font-family: var(--font-mono);
    white-space: nowrap;
  }

  .settings-panel {
    width: var(--report-w);
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--border);
    background: var(--bg-card);
    border-radius: var(--radius-lg);
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .settings-title { font-size: 13px; font-weight: 700; color: var(--text-primary); }
  .settings-desc  { font-size: 11px; color: var(--text-tertiary); }
  .settings-grid  { display: grid; grid-template-columns: repeat(4, 1fr); gap: var(--space-2); }
  .settings-item  { display: flex; flex-direction: column; gap: 4px; font-size: 11px; color: var(--text-secondary); }
  .settings-item input {
    background: var(--bg-hover); border: 1px solid var(--border);
    border-radius: var(--radius-sm); color: var(--text-primary);
    padding: 6px 8px; font-family: var(--font-mono); font-size: 12px;
  }
  .settings-hint  { display: flex; gap: var(--space-3); font-size: 11px; font-family: var(--font-mono); }
  .settings-hint .ok  { color: var(--color-success); }
  .settings-hint .bad { color: var(--color-error); }
  .settings-actions { display: flex; align-items: center; gap: var(--space-3); }
  .save-msg { font-size: 11px; color: var(--text-secondary); }

  .err-strip {
    width: var(--report-w);
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--color-error);
    border-radius: var(--radius-sm);
    color: var(--color-error);
    background: rgba(168, 122, 122, 0.10);
    font-size: 12px;
  }

  .empty {
    width: var(--report-w);
    padding: 40px var(--space-4);
    text-align: center;
    color: var(--text-tertiary);
    border: 1px dashed var(--border);
    border-radius: var(--radius-lg);
    background: var(--bg-card);
    font-size: 13px;
  }

  /* ── Report canvas (exact copy of demo) ─────────────────── */
  #report-canvas {
    width: var(--report-w);
    padding: var(--space-4);
    background: var(--bg-base);
    border-radius: var(--radius-lg);
    border: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .report-head {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    padding: var(--space-4);
    border-radius: var(--radius-lg);
    border: 1px solid var(--border);
    background: linear-gradient(135deg, rgba(201, 169, 110, 0.10), var(--bg-card));
  }
  .report-head .title { font-size: 20px; font-weight: 700; letter-spacing: 0.02em; }
  .report-head .subtitle { font-size: 11px; color: var(--text-secondary); margin-top: 6px; line-height: 1.6; }
  .head-spacer { flex: 1; }
  .session-badge {
    font-size: 11px; font-weight: 600; color: var(--accent);
    background: var(--accent-muted); border: 1px solid var(--border);
    padding: 4px 12px; border-radius: 999px; white-space: nowrap;
  }
  .head-right { display: flex; flex-direction: column; align-items: flex-end; gap: 6px; }
  .next-day { font-size: 11px; color: var(--text-tertiary); font-family: var(--font-mono); }

  .section {
    padding: var(--space-4);
    border-radius: var(--radius-lg);
    border: 1px solid var(--border);
    background: var(--bg-card);
  }
  .section-head { display: flex; align-items: baseline; gap: var(--space-2); margin-bottom: var(--space-3); }
  .section-no { font-size: 11px; font-weight: 700; color: var(--accent); font-family: var(--font-mono); letter-spacing: 0.04em; }
  .section-title { font-size: 14px; font-weight: 600; }
  .section-tag { font-size: 10px; color: var(--text-tertiary); margin-left: auto; }

  /* 01 舆情标签墙 */
  .theme-wall { display: grid; gap: var(--space-2); grid-template-columns: repeat(4, 1fr); }
  .theme-wall.wall-n1 { grid-template-columns: 1fr; }
  .theme-wall.wall-n2 { grid-template-columns: repeat(2, 1fr); }
  .theme-wall.wall-n3 { grid-template-columns: repeat(3, 1fr); }
  .theme-wall.wall-n4 { grid-template-columns: repeat(4, 1fr); }
  .theme-wall.wall-1plus4 { grid-template-columns: repeat(4, 1fr); }
  .theme-wall.wall-3col { grid-template-columns: repeat(3, 1fr); }
  .theme-tag-card.ttc-first { grid-column: 1 / -1; }
  .theme-tag-card {
    display: flex; flex-direction: column; gap: var(--space-2);
    padding: var(--space-3);
    border-radius: var(--radius-sm);
    background: var(--bg-hover);
    border: 1px solid var(--border);
  }
  .ttc-head { display: flex; align-items: center; gap: 6px; }
  .ttc-name { font-size: 13px; font-weight: 600; }
  .ttc-count { font-size: 9px; color: var(--text-tertiary); font-family: var(--font-mono); margin-left: auto; }
  .eval-tag { font-size: 10px; font-weight: 700; padding: 2px 8px; border-radius: 4px; white-space: nowrap; }
  .eval-tag.news  { color: var(--accent);  background: var(--accent-muted); }
  .eval-tag.cata  { color: var(--up);      background: rgba(192, 82, 74, 0.18); }
  .eval-tag.mood  { color: var(--grade-b); background: rgba(124, 148, 168, 0.16); }
  .eval-tag.split { color: var(--down);    background: rgba(78, 154, 95, 0.16); }
  .eval-tag.risk  { color: var(--text-primary); background: rgba(168, 122, 122, 0.30); border: 1px solid var(--down); }
  .ttc-desc { font-size: 11px; color: var(--text-secondary); line-height: 1.5; }
  .ai-note {
    margin-top: var(--space-3);
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-sm);
    background: var(--accent-subtle);
    border: 1px solid var(--border);
    font-size: 12px; color: var(--text-secondary); line-height: 1.6;
  }
  .ai-note.placeholder { font-style: italic; color: var(--text-tertiary); }
  .ai-note .ai-tag { font-size: 9px; font-weight: 700; color: var(--accent); text-transform: uppercase; letter-spacing: 0.05em; margin-right: 6px; }

  /* 02 宏观 & 广度 */
  .macro-sub { font-size: 10px; font-weight: 600; color: var(--text-tertiary); letter-spacing: 0.04em; margin: var(--space-3) 0 var(--space-1); }
  .macro-sub:first-of-type { margin-top: 0; }
  .macro-grid { display: grid; grid-template-columns: repeat(5, 1fr); gap: var(--space-2); text-align: center; }
  .macro-cell { display: flex; flex-direction: column; gap: 2px; padding: var(--space-1) 0; }
  .macro-label { font-size: 10px; color: var(--text-tertiary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .macro-value { font-size: 13px; font-weight: 600; font-family: var(--font-mono); color: var(--text-primary); }
  .macro-value.up { color: var(--up); }
  .macro-value.down { color: var(--down); }
  .apd { display: inline-flex; gap: 2px; justify-content: center; font-family: var(--font-mono); font-size: 13px; font-weight: 600; }
  .apd .u { color: var(--up); }
  .apd .p { color: var(--flat); }
  .apd .d { color: var(--down); }
  .apd .sep { color: var(--text-tertiary); font-weight: 400; }
  .money-divider { height: 1px; background: var(--border); margin: var(--space-3) 0; }
  .money-strip {
    display: flex; align-items: center; gap: var(--space-3);
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-sm);
    background: var(--accent-subtle);
    border: 1px solid var(--border);
  }
  .money-strip.placeholder { background: var(--bg-hover); }
  .me-title { font-size: 10px; color: var(--text-tertiary); text-transform: uppercase; letter-spacing: 0.04em; white-space: nowrap; }
  .me-badge { font-size: 12px; font-weight: 700; padding: 2px 10px; border-radius: var(--radius-sm); white-space: nowrap; }
  .me-badge.hot    { color: var(--up); background: rgba(192, 82, 74, 0.18); }
  .me-badge.active { color: var(--accent); background: var(--accent-muted); }
  .money-strip .reason { font-size: 12px; color: var(--text-secondary); line-height: 1.5; }

  /* 02 板块资金流入榜 + 拥挤度雷达 */
  .sector-flow { display: flex; flex-direction: column; gap: var(--space-1); }
  .sector-row {
    display: flex; align-items: center; gap: var(--space-3);
    padding: 6px var(--space-3);
    border-radius: var(--radius-sm);
    background: var(--bg-hover);
    border: 1px solid var(--border);
  }
  .sf-name { font-size: 12px; font-weight: 500; min-width: 96px; }
  .sf-flow { font-size: 12px; font-weight: 700; font-family: var(--font-mono); min-width: 72px; }
  .sf-flow.in  { color: var(--up); }
  .sf-flow.out { color: var(--down); }
  .sf-bar-track { flex: 1; height: 6px; border-radius: 3px; background: var(--border); overflow: hidden; }
  .sf-bar-fill { height: 100%; border-radius: 3px; }
  .sf-bar-fill.in  { background: var(--up); }
  .sf-bar-fill.out { background: var(--down); }
  .crowd-badge {
    font-size: 10px; font-weight: 700;
    padding: 2px 8px; border-radius: 4px;
    white-space: nowrap; min-width: 58px; text-align: center;
  }
  .crowd-badge.healthy { color: var(--down);   background: rgba(78, 154, 95, 0.16); }
  .crowd-badge.warm    { color: var(--accent); background: var(--accent-muted); }
  .crowd-badge.hot     { color: var(--up);     background: rgba(192, 82, 74, 0.18); }
  .radar-hint {
    display: flex; align-items: center; gap: var(--space-2);
    margin-top: var(--space-2);
    font-size: 10px; color: var(--text-tertiary);
    flex-wrap: wrap;
  }
  .radar-hint .legend { display: inline-flex; align-items: center; gap: 4px; }
  .radar-hint .dot { width: 8px; height: 8px; border-radius: 2px; display: inline-block; }
  .radar-hint .dot-healthy { background: var(--down); }
  .radar-hint .dot-warm    { background: var(--accent); }
  .radar-hint .dot-hot     { background: var(--up); }

  /* 03 主线排序 */
  .theme-row {
    display: flex; align-items: center; gap: var(--space-3);
    padding: 10px var(--space-3);
    border-radius: var(--radius-sm);
    background: var(--bg-hover);
    border: 1px solid var(--border);
    margin-bottom: var(--space-2);
  }
  .theme-rank { font-size: 15px; font-weight: 700; font-family: var(--font-mono); color: var(--accent); min-width: 22px; text-align: center; }
  .theme-main { flex: 1; display: flex; flex-direction: column; gap: 3px; }
  .theme-name { font-size: 13px; font-weight: 600; }
  .theme-reason { font-size: 11px; color: var(--text-secondary); line-height: 1.5; }
  .theme-bars { display: flex; gap: var(--space-2); }
  .mini-bar { display: flex; flex-direction: column; gap: 2px; align-items: center; min-width: 44px; }
  .mini-bar .mb-label { font-size: 9px; color: var(--text-tertiary); }
  .mini-bar .mb-track { width: 40px; height: 4px; border-radius: 2px; background: var(--border); overflow: hidden; }
  .mini-bar .mb-fill { height: 100%; background: var(--accent); border-radius: 2px; }
  .theme-score { font-size: 15px; font-weight: 700; font-family: var(--font-mono); min-width: 40px; text-align: right; }
  .empty-inline { font-size: 12px; color: var(--text-tertiary); text-align: center; padding: var(--space-3) 0; }

  /* 04 SABC pool */
  .pool-grid { display: grid; grid-template-columns: repeat(2, 1fr); gap: var(--space-3); }
  .pool-box { border: 1px solid var(--border); border-radius: var(--radius-lg); overflow: hidden; }
  .pool-head { display: flex; align-items: center; gap: var(--space-2); padding: var(--space-2) var(--space-3); border-bottom: 1px solid var(--border); }
  .grade-badge {
    font-size: 12px; font-weight: 800; width: 22px; height: 22px;
    border-radius: 5px; display: flex; align-items: center; justify-content: center;
  }
  .grade-s .grade-badge { color: var(--bg-base); background: var(--grade-s); }
  .grade-a .grade-badge { color: var(--bg-base); background: var(--grade-a); }
  .grade-b .grade-badge { color: var(--bg-base); background: var(--grade-b); }
  .grade-c .grade-badge { color: var(--bg-base); background: var(--grade-c); }
  .grade-s .pool-head { background: rgba(201, 169, 110, 0.10); }
  .grade-a .pool-head { background: rgba(138, 154, 118, 0.10); }
  .grade-b .pool-head { background: rgba(124, 148, 168, 0.10); }
  .grade-c .pool-head { background: rgba(158, 154, 150, 0.08); }
  .pool-label { font-size: 12px; font-weight: 600; }
  .pool-sub { font-size: 10px; color: var(--text-tertiary); margin-left: auto; }
  .pool-body { padding: var(--space-1) var(--space-2); }
  .pool-empty { font-size: 11px; color: var(--text-tertiary); padding: 8px var(--space-2); }
  .stock-row { display: flex; align-items: center; gap: var(--space-2); padding: 5px var(--space-2); border-radius: 4px; }
  .stock-row + .stock-row { border-top: 1px solid var(--border); }
  .stk-name { font-size: 12px; font-weight: 500; min-width: 60px; }
  .stk-code { font-size: 10px; color: var(--text-tertiary); font-family: var(--font-mono); }
  .stk-spacer { flex: 1; }
  .stk-tags { display: flex; gap: 3px; }
  .stk-tag {
    font-size: 9px; padding: 1px 5px; border-radius: 3px;
    background: var(--bg-hover); color: var(--text-secondary);
    border: 1px solid var(--border);
  }
  .stk-tag.money { color: var(--up); }
  .stk-tag.mood { color: var(--accent); }
  .stk-score { font-size: 12px; font-weight: 700; font-family: var(--font-mono); min-width: 30px; text-align: right; color: var(--text-primary); }

  .report-foot {
    display: flex; align-items: center; gap: var(--space-2);
    padding: var(--space-3) var(--space-4);
    border-radius: var(--radius-lg);
    border: 1px solid var(--border);
    background: var(--bg-card);
    font-size: 11px; color: var(--text-tertiary); line-height: 1.6;
  }
  .report-foot .disclaimer { flex: 1; }
  .report-foot .brand { font-weight: 700; color: var(--accent); }
</style>
