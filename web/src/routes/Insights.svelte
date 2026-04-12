<script lang="ts">
  import { onMount } from 'svelte';

  import {
    getCurrentInsight,
    getDailyInsight,
    getHourlyInsights,
    type DailyInsight,
    type InsightRecord,
  } from '$lib/api';

  type RollingContextView = {
    currentFocus: string;
    activeProject: string | null;
    appsUsed: Array<{ name: string; share: string }>;
  };

  type HourlyDigestView = {
    id: number;
    label: string;
    dominantActivity: string;
    focusScoreLabel: string | null;
    projects: Array<{ name: string; minutes: number; activities: string[] }>;
    topics: string[];
    keyMoments: string[];
    narrative: string | null;
    hourStart: string | null;
    hourEnd: string | null;
  };

  type DailySummaryView = {
    date: string;
    totalActiveHours: number | null;
    projectBreakdown: Array<{ name: string; totalMinutes: number; activities: string[]; keyAccomplishments: string[] }>;
    timeAllocation: Array<{ label: string; value: string }>;
    focusBlocks: Array<{ start: string; end: string; durationMinutes: number; project: string; quality: string; tint: string }>;
    openThreads: string[];
    narrative: string | null;
  };

  const FOCUS_TINTS: Record<string, string> = {
    deep: 'bg-emerald-500', focused: 'bg-emerald-500',
    moderate: 'bg-amber-500', shallow: 'bg-orange-500',
    fragmented: 'bg-red-500', distracted: 'bg-red-500',
  };

  const DATE_DISPLAY = new Intl.DateTimeFormat(undefined, { weekday: 'long', month: 'long', day: 'numeric' });

  let selectedDate = formatLocalDate(new Date());
  let loading = true;
  let errorMessage: string | null = null;
  let rollingContext: RollingContextView | null = null;
  let hourlyDigests: HourlyDigestView[] = [];
  let dailySummary: DailySummaryView | null = null;
  let requestVersion = 0;

  $: hasNoInsights = !loading && !rollingContext && hourlyDigests.length === 0 && !dailySummary;
  $: displayDate = (() => {
    const [y, m, d] = selectedDate.split('-').map(Number);
    return DATE_DISPLAY.format(new Date(y, m - 1, d));
  })();

  onMount(() => { void loadInsights(selectedDate); });

  function handleDateChange(event: Event): void {
    const input = event.currentTarget;
    if (!(input instanceof HTMLInputElement)) return;
    const nextDate = input.value.trim();
    if (!nextDate || nextDate === selectedDate) return;
    selectedDate = nextDate;
    void loadInsights(nextDate);
  }

  async function loadInsights(date: string): Promise<void> {
    const currentRequest = ++requestVersion;
    loading = true;
    errorMessage = null;

    const [currentResult, hourlyResult, dailyResult] = await Promise.allSettled([
      getCurrentInsight(),
      getHourlyInsights(date),
      getDailyInsight(date),
    ]);

    if (currentRequest !== requestVersion) return;

    const errors: string[] = [];

    if (currentResult.status === 'fulfilled') {
      rollingContext = normalizeRollingContext(currentResult.value);
    } else {
      rollingContext = null;
      errors.push('Unable to load rolling context.');
    }

    if (hourlyResult.status === 'fulfilled') {
      hourlyDigests = normalizeHourlyDigests(hourlyResult.value);
    } else {
      hourlyDigests = [];
      errors.push('Unable to load hourly digests.');
    }

    if (dailyResult.status === 'fulfilled') {
      dailySummary = normalizeDailySummary(dailyResult.value, date);
    } else {
      dailySummary = null;
      errors.push('Unable to load daily summary.');
    }

    loading = false;
    errorMessage = errors.length === 0 ? null : errors.join(' ');
  }

  function normalizeRollingContext(insight: InsightRecord | null): RollingContextView | null {
    if (!insight) return null;
    const data = asRecord(insight.data);
    if (!data) return null;
    const currentFocus = asString(data.current_focus) ?? asString(data.currentFocus);
    if (!currentFocus) return null;
    return {
      currentFocus,
      activeProject: asString(data.active_project) ?? asString(data.activeProject) ?? null,
      appsUsed: normalizeAppsUsed(data.apps_used ?? data.appsUsed),
    };
  }

  function normalizeAppsUsed(rawApps: unknown): Array<{ name: string; share: string }> {
    if (Array.isArray(rawApps)) {
      return rawApps.map((entry) => {
        const app = asRecord(entry);
        if (!app) return null;
        const name = asString(app.name) ?? asString(app.app_name) ?? asString(app.app);
        if (!name) return null;
        const share = asString(app.share) ?? asString(app.value) ?? asString(app.percentage) ?? 'active';
        return { name, share };
      }).filter((e): e is { name: string; share: string } => e !== null).slice(0, 8);
    }
    const appMap = asRecord(rawApps);
    if (!appMap) return [];
    return Object.entries(appMap).map(([name, value]) => {
      const asText = asString(value);
      if (asText) return { name, share: asText };
      const asNum = asNumber(value);
      if (asNum === null) return null;
      const normalized = asNum <= 1 ? asNum * 100 : asNum;
      return { name, share: `${Math.round(normalized)}%` };
    }).filter((e): e is { name: string; share: string } => e !== null).slice(0, 8);
  }

  function normalizeHourlyDigests(insights: InsightRecord[]): HourlyDigestView[] {
    return insights.map((insight) => {
      const data = asRecord(insight.data);
      if (!data) return null;
      const hourStart = asString(data.hour_start) ?? insight.window_start ?? null;
      const hourEnd = asString(data.hour_end) ?? insight.window_end ?? null;
      return {
        id: insight.id,
        label: formatHourRange(hourStart, hourEnd),
        dominantActivity: asString(data.dominant_activity) ?? 'No dominant activity recorded',
        focusScoreLabel: formatFocusScore(asNumber(data.focus_score)),
        projects: normalizeHourlyProjects(data.projects),
        topics: readStringArray(data.topics),
        keyMoments: readStringArray(data.key_moments ?? data.keyMoments),
        narrative: asString(data.narrative) ?? asString(insight.narrative) ?? null,
        hourStart, hourEnd,
      };
    }).filter((d): d is HourlyDigestView => d !== null)
      .sort((a, b) => {
        const at = a.hourStart ? new Date(a.hourStart).getTime() : 0;
        const bt = b.hourStart ? new Date(b.hourStart).getTime() : 0;
        return bt - at;
      });
  }

  function normalizeHourlyProjects(raw: unknown): HourlyDigestView['projects'] {
    if (!Array.isArray(raw)) return [];
    return raw.map((entry) => {
      const p = asRecord(entry);
      if (!p) return null;
      return { name: asString(p.name) ?? 'Uncategorized', minutes: asNumber(p.minutes) ?? 0, activities: readStringArray(p.activities) };
    }).filter((p): p is HourlyDigestView['projects'][number] => p !== null);
  }

  function normalizeDailySummary(insight: DailyInsight | null, fallbackDate: string): DailySummaryView | null {
    if (!insight) return null;
    const data = asRecord(insight.data);
    if (!data) return null;
    const projectBreakdown = normalizeDailyProjects(data.projects);
    const timeAllocation = normalizeTimeAllocation(data.time_allocation ?? data.timeAllocation);
    const focusBlocks = normalizeFocusBlocks(data.focus_blocks ?? data.focusBlocks);
    const openThreads = readStringArray(data.open_threads ?? data.openThreads);
    const narrative = asString(data.narrative) ?? asString(insight.narrative) ?? null;
    const totalActiveHours = asNumber(data.total_active_hours ?? data.totalActiveHours);
    const hasSignal = projectBreakdown.length > 0 || timeAllocation.length > 0 || focusBlocks.length > 0 || openThreads.length > 0 || narrative !== null || totalActiveHours !== null;
    if (!hasSignal) return null;
    return { date: asString(data.date) ?? fallbackDate, totalActiveHours, projectBreakdown, timeAllocation, focusBlocks, openThreads, narrative };
  }

  function normalizeDailyProjects(raw: unknown): DailySummaryView['projectBreakdown'] {
    if (!Array.isArray(raw)) return [];
    return raw.map((entry) => {
      const p = asRecord(entry);
      if (!p) return null;
      return {
        name: asString(p.name) ?? 'Uncategorized',
        totalMinutes: asNumber(p.total_minutes ?? p.minutes) ?? 0,
        activities: readStringArray(p.activities),
        keyAccomplishments: readStringArray(p.key_accomplishments ?? p.keyAccomplishments),
      };
    }).filter((p): p is DailySummaryView['projectBreakdown'][number] => p !== null)
      .sort((a, b) => b.totalMinutes - a.totalMinutes);
  }

  function normalizeTimeAllocation(raw: unknown): DailySummaryView['timeAllocation'] {
    const alloc = asRecord(raw);
    if (!alloc) return [];
    return Object.entries(alloc).map(([label, value]) => {
      const text = asString(value);
      return text ? { label, value: text } : null;
    }).filter((e): e is DailySummaryView['timeAllocation'][number] => e !== null);
  }

  function normalizeFocusBlocks(raw: unknown): DailySummaryView['focusBlocks'] {
    if (!Array.isArray(raw)) return [];
    return raw.map((entry) => {
      const b = asRecord(entry);
      if (!b) return null;
      const quality = (asString(b.quality) ?? 'moderate').toLowerCase();
      return {
        start: asString(b.start) ?? '--:--', end: asString(b.end) ?? '--:--',
        durationMinutes: asNumber(b.duration_min ?? b.durationMinutes) ?? 0,
        project: asString(b.project) ?? 'Uncategorized', quality,
        tint: FOCUS_TINTS[quality] ?? 'bg-amber-500',
      };
    }).filter((e): e is DailySummaryView['focusBlocks'][number] => e !== null);
  }

  function readStringArray(value: unknown): string[] {
    if (!Array.isArray(value)) return [];
    return value
      .map((e) => (typeof e === 'string' ? e.trim() : ''))
      .filter((e) => e.length > 0);
  }

  function asRecord(value: unknown): Record<string, unknown> | null {
    return typeof value === 'object' && value !== null
      ? (value as Record<string, unknown>)
      : null;
  }

  function asString(value: unknown): string | null {
    if (typeof value !== 'string') return null;
    const trimmed = value.trim();
    return trimmed.length > 0 ? trimmed : null;
  }

  function asNumber(value: unknown): number | null {
    return typeof value === 'number' && Number.isFinite(value) ? value : null;
  }

  function formatFocusScore(score: number | null): string | null {
    if (score === null) return null;
    const normalized = score <= 1 ? score * 100 : score;
    return `${Math.round(normalized)}%`;
  }

  function formatHourRange(start: string | null, end: string | null): string {
    if (!start && !end) return 'Unknown hour';

    const fmt = new Intl.DateTimeFormat(undefined, { hour: 'numeric', minute: '2-digit' });
    const startDate = start ? new Date(start) : null;
    const endDate = end ? new Date(end) : null;
    const validStart = startDate && Number.isFinite(startDate.getTime());
    const validEnd = endDate && Number.isFinite(endDate.getTime());

    if (validStart && validEnd) return `${fmt.format(startDate)} – ${fmt.format(endDate)}`;
    if (validStart) return fmt.format(startDate);
    if (validEnd) return fmt.format(endDate);
    return 'Unknown hour';
  }

  function formatLocalDate(value: Date): string {
    const y = value.getFullYear();
    const m = String(value.getMonth() + 1).padStart(2, '0');
    const d = String(value.getDate()).padStart(2, '0');
    return `${y}-${m}-${d}`;
  }

  function formatMinutes(m: number): string {
    const hours = Math.floor(m / 60);
    const mins = m % 60;
    return hours > 0 ? `${hours}h ${mins}m` : `${mins}m`;
  }
</script>

<div class="space-y-8" aria-busy={loading}>
  <!-- Page Header -->
  <div class="flex items-end justify-between">
    <div>
      <h1 class="text-[2.25rem] font-semibold tracking-tight text-on-surface">Insights</h1>
      <p class="text-secondary text-sm">{displayDate}</p>
    </div>
    <div class="flex items-center gap-2 px-4 py-2 bg-surface-container-high rounded-lg text-on-surface cursor-pointer hover:bg-surface-container-highest transition-colors text-sm">
      <span class="material-symbols-outlined text-sm">calendar_today</span>
      <input type="date" class="bg-transparent border-none p-0 text-sm font-medium focus:ring-0 cursor-pointer" value={selectedDate} on:change={handleDateChange} />
    </div>
  </div>

  {#if errorMessage}
    <div class="bg-amber-50 dark:bg-amber-950/50 text-amber-800 dark:text-amber-200 rounded-2xl px-6 py-4 text-sm" role="status">{errorMessage}</div>
  {/if}

  {#if loading}
    <!-- Skeleton -->
    <div class="grid grid-cols-12 gap-6">
      <div class="col-span-12 lg:col-span-8 bg-surface-container-lowest rounded-[24px] p-8 animate-pulse h-48"></div>
      <div class="col-span-12 lg:col-span-4 bg-primary-container rounded-[24px] p-8 animate-pulse h-48"></div>
      <div class="col-span-12 lg:col-span-4 bg-surface-container-low rounded-[24px] p-6 animate-pulse h-64"></div>
      <div class="col-span-12 lg:col-span-8 bg-surface-container-lowest rounded-[24px] p-6 animate-pulse h-64"></div>
    </div>
  {:else if hasNoInsights}
    <div class="bg-surface-container-lowest rounded-[24px] p-12 text-center">
      <span class="material-symbols-outlined text-5xl text-on-surface-variant/40 mb-4">analytics</span>
      <h3 class="text-lg font-semibold text-on-surface mb-2">No insights for this day</h3>
      <p class="text-sm text-secondary">Insights are generated from extracted captures. Make sure the extraction pipeline is running.</p>
    </div>
  {:else}
    <div class="grid grid-cols-12 gap-6">
      <!-- Daily Digest -->
      {#if dailySummary?.narrative}
        <div class="col-span-12 lg:col-span-8 bg-surface-container-lowest rounded-[24px] p-8 relative overflow-hidden group">
          <div class="absolute top-0 right-0 p-8 opacity-10 group-hover:opacity-20 transition-opacity">
            <span class="material-symbols-outlined text-[120px]">auto_awesome</span>
          </div>
          <div class="relative z-10">
            <div class="flex items-center gap-2 mb-6">
              <span class="material-symbols-outlined text-primary">summarize</span>
              <h2 class="text-lg font-semibold">Daily Digest</h2>
            </div>
            <p class="text-xl leading-relaxed text-on-surface-variant font-medium max-w-2xl">{dailySummary.narrative}</p>
            <div class="mt-8 flex gap-4 flex-wrap">
              {#if dailySummary.totalActiveHours}
                <div class="bg-surface-container-low px-4 py-2 rounded-full flex items-center gap-2">
                  <span class="material-symbols-outlined text-[16px] text-primary">timer</span>
                  <span class="text-xs font-semibold text-secondary">{dailySummary.totalActiveHours.toFixed(1)}h Recorded</span>
                </div>
              {/if}
              {#if dailySummary.projectBreakdown.length > 0}
                <div class="bg-surface-container-low px-4 py-2 rounded-full flex items-center gap-2">
                  <span class="material-symbols-outlined text-[16px] text-primary">folder</span>
                  <span class="text-xs font-semibold text-secondary">{dailySummary.projectBreakdown.length} project{dailySummary.projectBreakdown.length === 1 ? '' : 's'}</span>
                </div>
              {/if}
            </div>
          </div>
        </div>
      {:else}
        <div class="col-span-12 lg:col-span-8 bg-surface-container-lowest rounded-[24px] p-8 flex items-center justify-center">
          <p class="text-secondary text-sm">No daily summary available yet.</p>
        </div>
      {/if}

      <!-- Rolling Context / Focus Card -->
      {#if rollingContext}
        <div class="col-span-12 lg:col-span-4 bg-primary-container text-white rounded-[24px] p-8 flex flex-col justify-between shadow-xl shadow-primary/10">
          <div class="flex justify-between items-start">
            <div class="bg-white/20 p-2 rounded-xl backdrop-blur-sm">
              <span class="material-symbols-outlined">view_in_ar</span>
            </div>
            <span class="text-xs font-bold bg-white/20 px-2 py-1 rounded-md">LIVE FOCUS</span>
          </div>
          <div class="mt-6">
            <div class="text-2xl font-bold mb-1">{rollingContext.currentFocus}</div>
            <div class="text-white/80 text-sm font-medium">
              {rollingContext.activeProject ?? 'No active project'}
            </div>
            {#if rollingContext.appsUsed.length > 0}
              <div class="mt-4 flex flex-wrap gap-2">
                {#each rollingContext.appsUsed.slice(0, 4) as app}
                  <span class="text-[10px] bg-white/15 px-2 py-1 rounded-md font-medium">{app.name}: {app.share}</span>
                {/each}
              </div>
            {/if}
          </div>
        </div>
      {:else}
        <div class="col-span-12 lg:col-span-4 bg-surface-container-low rounded-[24px] p-8 flex flex-col items-center justify-center text-center">
          <span class="material-symbols-outlined text-3xl text-on-surface-variant/40 mb-2">sensors_off</span>
          <p class="text-sm text-secondary">No rolling context right now</p>
        </div>
      {/if}

      <!-- Focus Blocks -->
      {#if dailySummary && dailySummary.focusBlocks.length > 0}
        <div class="col-span-12 lg:col-span-4 bg-surface-container-low rounded-[24px] p-6">
          <div class="flex items-center justify-between mb-6">
            <h2 class="text-sm font-bold text-secondary uppercase tracking-widest">Focus Blocks</h2>
          </div>
          <div class="space-y-4">
            {#each dailySummary.focusBlocks as block}
              <div class="bg-surface-container-lowest p-4 rounded-2xl flex items-start gap-4">
                <div class="text-[10px] font-bold text-primary mt-1 w-20 shrink-0">{block.start} – {block.end}</div>
                <div>
                  <div class="text-sm font-semibold mb-1">{block.project}</div>
                  <div class="flex items-center gap-2">
                    <div class="w-2 h-2 rounded-full {block.tint}"></div>
                    <p class="text-[11px] text-secondary capitalize">{block.quality} · {formatMinutes(block.durationMinutes)}</p>
                  </div>
                </div>
              </div>
            {/each}
          </div>
        </div>
      {/if}

      <!-- Project Breakdown -->
      {#if dailySummary && dailySummary.projectBreakdown.length > 0}
        <div class="col-span-12 lg:col-span-8 grid grid-cols-1 md:grid-cols-2 gap-6">
          {#each dailySummary.projectBreakdown as project, i}
            <div class="bg-surface-container-lowest rounded-[24px] p-6 border-l-4 {i === 0 ? 'border-primary' : 'border-tertiary-container'} shadow-sm">
              <div class="flex justify-between items-start mb-4">
                <div>
                  <h3 class="font-bold text-on-surface">{project.name}</h3>
                  <span class="text-[10px] text-secondary font-medium uppercase">
                    {project.activities.length > 0 ? project.activities[0] : 'Project'}
                  </span>
                </div>
                <div class="text-right">
                  <div class="text-lg font-bold {i === 0 ? 'text-primary' : 'text-tertiary-container'}">{formatMinutes(project.totalMinutes)}</div>
                </div>
              </div>
              {#if project.keyAccomplishments.length > 0}
                <ul class="space-y-3">
                  {#each project.keyAccomplishments.slice(0, 3) as item}
                    <li class="flex items-center gap-3">
                      <span class="material-symbols-outlined text-[16px] text-emerald-500" style="font-variation-settings: 'FILL' 1;">check_circle</span>
                      <span class="text-xs text-on-surface-variant">{item}</span>
                    </li>
                  {/each}
                </ul>
              {/if}
            </div>
          {/each}
        </div>
      {/if}

      <!-- Open Threads -->
      {#if dailySummary && dailySummary.openThreads.length > 0}
        <div class="col-span-12 bg-surface-container-high rounded-[24px] p-6">
          <div class="flex items-center justify-between mb-4">
            <div class="flex items-center gap-2">
              <span class="material-symbols-outlined text-purple-600">model_training</span>
              <h2 class="text-sm font-bold text-on-surface uppercase tracking-widest">Unfinished Threads</h2>
            </div>
            <span class="text-[10px] font-bold text-secondary">{dailySummary.openThreads.length} PENDING ITEMS</span>
          </div>
          <div class="space-y-3">
            {#each dailySummary.openThreads as thread}
              <div class="flex items-center gap-4 bg-surface-container-lowest/50 p-4 rounded-xl">
                <span class="material-symbols-outlined text-amber-500">chat_bubble</span>
                <span class="text-xs font-bold text-on-surface">{thread}</span>
              </div>
            {/each}
          </div>
        </div>
      {/if}

      <!-- Hourly Digests -->
      {#if hourlyDigests.length > 0}
        <div class="col-span-12">
          <h2 class="text-lg font-bold mb-4">Hourly Digests</h2>
          <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
            {#each hourlyDigests as digest (digest.id)}
              <div class="bg-surface-container-lowest rounded-[24px] p-6">
                <div class="flex items-center justify-between mb-3">
                  <span class="text-[10px] font-bold text-primary uppercase tracking-wider">{digest.label}</span>
                  {#if digest.focusScoreLabel}
                    <span class="text-xs font-bold text-emerald-600 dark:text-emerald-400 bg-emerald-50 dark:bg-emerald-950 px-2 py-0.5 rounded-md">{digest.focusScoreLabel}</span>
                  {/if}
                </div>
                <p class="text-sm font-semibold text-on-surface mb-2">{digest.dominantActivity}</p>
                {#if digest.narrative}
                  <p class="text-xs text-on-surface-variant leading-relaxed mb-3">{digest.narrative}</p>
                {/if}
                {#if digest.topics.length > 0}
                  <div class="flex flex-wrap gap-1.5">
                    {#each digest.topics.slice(0, 4) as topic}
                      <span class="px-2 py-0.5 bg-surface-container-low text-on-surface-variant rounded-full text-[10px] font-medium">{topic}</span>
                    {/each}
                  </div>
                {/if}
              </div>
            {/each}
          </div>
        </div>
      {/if}
    </div>
  {/if}
</div>
