<script lang="ts">
  import { onMount } from 'svelte';

  import {
    getCurrentInsight,
    getDailyInsight,
    getHourlyInsights,
    type DailyInsight,
    type InsightRecord,
  } from '$lib/api';
  import DailySummarySection from '$lib/components/DailySummarySection.svelte';
  import HourlyDigestCard from '$lib/components/HourlyDigestCard.svelte';
  import RollingContextCard from '$lib/components/RollingContextCard.svelte';

  type RollingContextView = {
    currentFocus: string;
    activeProject: string | null;
    appsUsed: Array<{
      name: string;
      share: string;
    }>;
  };

  type HourlyDigestView = {
    id: number;
    label: string;
    dominantActivity: string;
    focusScoreLabel: string | null;
    projects: Array<{
      name: string;
      minutes: number;
      activities: string[];
    }>;
    topics: string[];
    keyMoments: string[];
    narrative: string | null;
    hourStart: string | null;
    hourEnd: string | null;
  };

  type DailySummaryView = {
    date: string;
    totalActiveHours: number | null;
    projectBreakdown: Array<{
      name: string;
      totalMinutes: number;
      activities: string[];
      keyAccomplishments: string[];
    }>;
    timeAllocation: Array<{
      label: string;
      value: string;
    }>;
    focusBlocks: Array<{
      start: string;
      end: string;
      durationMinutes: number;
      project: string;
      quality: string;
      tint: string;
    }>;
    openThreads: string[];
    narrative: string | null;
  };

  const FOCUS_TINTS: Record<string, string> = {
    deep: '#70ffe3',
    focused: '#70ffe3',
    moderate: '#ffb347',
    shallow: '#ff4ea6',
    fragmented: '#ff4ea6',
    distracted: '#ff4ea6',
  };

  let selectedDate = formatLocalDate(new Date());
  let loading = true;
  let errorMessage: string | null = null;
  let rollingContext: RollingContextView | null = null;
  let hourlyDigests: HourlyDigestView[] = [];
  let dailySummary: DailySummaryView | null = null;
  let requestVersion = 0;

  $: hasNoInsights = !loading && !rollingContext && hourlyDigests.length === 0 && !dailySummary;

  onMount(() => {
    void loadInsights(selectedDate);
  });

  function handleDateChange(event: Event): void {
    const input = event.currentTarget;
    if (!(input instanceof HTMLInputElement)) {
      return;
    }

    const nextDate = input.value.trim();
    if (!nextDate || nextDate === selectedDate) {
      return;
    }

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

    if (currentRequest !== requestVersion) {
      return;
    }

    const errors: string[] = [];

    if (currentResult.status === 'fulfilled') {
      rollingContext = normalizeRollingContext(currentResult.value);
    } else {
      rollingContext = null;
      errors.push('Unable to load rolling context.');
      console.error('Current insight request failed', currentResult.reason);
    }

    if (hourlyResult.status === 'fulfilled') {
      hourlyDigests = normalizeHourlyDigests(hourlyResult.value);
    } else {
      hourlyDigests = [];
      errors.push('Unable to load hourly digests.');
      console.error('Hourly insight request failed', hourlyResult.reason);
    }

    if (dailyResult.status === 'fulfilled') {
      dailySummary = normalizeDailySummary(dailyResult.value, date);
    } else {
      dailySummary = null;
      errors.push('Unable to load daily summary.');
      console.error('Daily insight request failed', dailyResult.reason);
    }

    loading = false;
    errorMessage = errors.length === 0 ? null : errors.join(' ');
  }

  function normalizeRollingContext(insight: InsightRecord | null): RollingContextView | null {
    if (!insight) {
      return null;
    }

    const data = asRecord(insight.data);
    if (!data) {
      return null;
    }

    const currentFocus = asString(data.current_focus) ?? asString(data.currentFocus);
    if (!currentFocus) {
      return null;
    }

    return {
      currentFocus,
      activeProject: asString(data.active_project) ?? asString(data.activeProject) ?? null,
      appsUsed: normalizeAppsUsed(data.apps_used ?? data.appsUsed),
    };
  }

  function normalizeAppsUsed(rawApps: unknown): Array<{ name: string; share: string }> {
    if (Array.isArray(rawApps)) {
      return rawApps
        .map((entry) => {
          const app = asRecord(entry);
          if (!app) {
            return null;
          }

          const name = asString(app.name) ?? asString(app.app_name) ?? asString(app.app);
          if (!name) {
            return null;
          }

          const share = asString(app.share) ?? asString(app.value) ?? asString(app.percentage) ?? 'active';
          return {
            name,
            share,
          };
        })
        .filter((entry): entry is { name: string; share: string } => entry !== null)
        .slice(0, 8);
    }

    const appMap = asRecord(rawApps);
    if (!appMap) {
      return [];
    }

    return Object.entries(appMap)
      .map(([name, value]) => {
        const asText = asString(value);
        if (asText) {
          return { name, share: asText };
        }

        const asNumeric = asNumber(value);
        if (asNumeric === null) {
          return null;
        }

        const normalized = asNumeric <= 1 ? asNumeric * 100 : asNumeric;
        return {
          name,
          share: `${Math.round(normalized)}%`,
        };
      })
      .filter((entry): entry is { name: string; share: string } => entry !== null)
      .slice(0, 8);
  }

  function normalizeHourlyDigests(insights: InsightRecord[]): HourlyDigestView[] {
    return insights
      .map((insight) => {
        const data = asRecord(insight.data);
        if (!data) {
          return null;
        }

        const hourStart = asString(data.hour_start) ?? insight.window_start ?? null;
        const hourEnd = asString(data.hour_end) ?? insight.window_end ?? null;
        const projects = normalizeHourlyProjects(data.projects);

        return {
          id: insight.id,
          label: formatHourRange(hourStart, hourEnd),
          dominantActivity: asString(data.dominant_activity) ?? 'No dominant activity recorded',
          focusScoreLabel: formatFocusScore(asNumber(data.focus_score)),
          projects,
          topics: readStringArray(data.topics),
          keyMoments: readStringArray(data.key_moments ?? data.keyMoments),
          narrative: asString(data.narrative) ?? asString(insight.narrative) ?? null,
          hourStart,
          hourEnd,
        };
      })
      .filter((digest): digest is HourlyDigestView => digest !== null)
      .sort((left, right) => {
        const leftTime = left.hourStart ? new Date(left.hourStart).getTime() : 0;
        const rightTime = right.hourStart ? new Date(right.hourStart).getTime() : 0;
        return rightTime - leftTime;
      });
  }

  function normalizeHourlyProjects(rawProjects: unknown): HourlyDigestView['projects'] {
    if (!Array.isArray(rawProjects)) {
      return [];
    }

    return rawProjects
      .map((entry) => {
        const project = asRecord(entry);
        if (!project) {
          return null;
        }

        return {
          name: asString(project.name) ?? 'Uncategorized',
          minutes: asNumber(project.minutes) ?? 0,
          activities: readStringArray(project.activities),
        };
      })
      .filter((project): project is HourlyDigestView['projects'][number] => project !== null);
  }

  function normalizeDailySummary(insight: DailyInsight | null, fallbackDate: string): DailySummaryView | null {
    if (!insight) {
      return null;
    }

    const data = asRecord(insight.data);
    if (!data) {
      return null;
    }

    const projectBreakdown = normalizeDailyProjects(data.projects);
    const timeAllocation = normalizeTimeAllocation(data.time_allocation ?? data.timeAllocation);
    const focusBlocks = normalizeFocusBlocks(data.focus_blocks ?? data.focusBlocks);
    const openThreads = readStringArray(data.open_threads ?? data.openThreads);
    const narrative = asString(data.narrative) ?? asString(insight.narrative) ?? null;
    const totalActiveHours = asNumber(data.total_active_hours ?? data.totalActiveHours);

    const hasSignal =
      projectBreakdown.length > 0 ||
      timeAllocation.length > 0 ||
      focusBlocks.length > 0 ||
      openThreads.length > 0 ||
      narrative !== null ||
      totalActiveHours !== null;

    if (!hasSignal) {
      return null;
    }

    return {
      date: asString(data.date) ?? fallbackDate,
      totalActiveHours,
      projectBreakdown,
      timeAllocation,
      focusBlocks,
      openThreads,
      narrative,
    };
  }

  function normalizeDailyProjects(rawProjects: unknown): DailySummaryView['projectBreakdown'] {
    if (!Array.isArray(rawProjects)) {
      return [];
    }

    return rawProjects
      .map((entry) => {
        const project = asRecord(entry);
        if (!project) {
          return null;
        }

        return {
          name: asString(project.name) ?? 'Uncategorized',
          totalMinutes: asNumber(project.total_minutes ?? project.minutes) ?? 0,
          activities: readStringArray(project.activities),
          keyAccomplishments: readStringArray(project.key_accomplishments ?? project.keyAccomplishments),
        };
      })
      .filter((project): project is DailySummaryView['projectBreakdown'][number] => project !== null)
      .sort((left, right) => right.totalMinutes - left.totalMinutes);
  }

  function normalizeTimeAllocation(rawAllocation: unknown): DailySummaryView['timeAllocation'] {
    const allocation = asRecord(rawAllocation);
    if (!allocation) {
      return [];
    }

    return Object.entries(allocation)
      .map(([label, value]) => {
        const text = asString(value);
        if (!text) {
          return null;
        }

        return {
          label,
          value: text,
        };
      })
      .filter((entry): entry is DailySummaryView['timeAllocation'][number] => entry !== null);
  }

  function normalizeFocusBlocks(rawBlocks: unknown): DailySummaryView['focusBlocks'] {
    if (!Array.isArray(rawBlocks)) {
      return [];
    }

    return rawBlocks
      .map((entry) => {
        const block = asRecord(entry);
        if (!block) {
          return null;
        }

        const quality = (asString(block.quality) ?? 'moderate').toLowerCase();
        return {
          start: asString(block.start) ?? '--:--',
          end: asString(block.end) ?? '--:--',
          durationMinutes: asNumber(block.duration_min ?? block.durationMinutes) ?? 0,
          project: asString(block.project) ?? 'Uncategorized',
          quality,
          tint: FOCUS_TINTS[quality] ?? '#ffb347',
        };
      })
      .filter((entry): entry is DailySummaryView['focusBlocks'][number] => entry !== null);
  }

  function readStringArray(value: unknown): string[] {
    if (!Array.isArray(value)) {
      return [];
    }

    return value
      .map((entry) => (typeof entry === 'string' ? entry.trim() : ''))
      .filter((entry) => entry.length > 0);
  }

  function asRecord(value: unknown): Record<string, unknown> | null {
    return typeof value === 'object' && value !== null ? (value as Record<string, unknown>) : null;
  }

  function asString(value: unknown): string | null {
    return typeof value === 'string' && value.trim().length > 0 ? value.trim() : null;
  }

  function asNumber(value: unknown): number | null {
    return typeof value === 'number' && Number.isFinite(value) ? value : null;
  }

  function formatFocusScore(score: number | null): string | null {
    if (score === null) {
      return null;
    }

    const normalized = score <= 1 ? score * 100 : score;
    return `${Math.round(normalized)}%`;
  }

  function formatHourRange(rawStart: string | null, rawEnd: string | null): string {
    if (!rawStart && !rawEnd) {
      return 'Unknown hour';
    }

    const formatter = new Intl.DateTimeFormat(undefined, {
      hour: 'numeric',
      minute: '2-digit',
    });

    const startDate = rawStart ? new Date(rawStart) : null;
    const endDate = rawEnd ? new Date(rawEnd) : null;

    if (startDate && Number.isFinite(startDate.getTime()) && endDate && Number.isFinite(endDate.getTime())) {
      return `${formatter.format(startDate)}–${formatter.format(endDate)}`;
    }

    if (startDate && Number.isFinite(startDate.getTime())) {
      return formatter.format(startDate);
    }

    if (endDate && Number.isFinite(endDate.getTime())) {
      return formatter.format(endDate);
    }

    return 'Unknown hour';
  }

  function formatLocalDate(value: Date): string {
    const year = value.getFullYear();
    const month = String(value.getMonth() + 1).padStart(2, '0');
    const day = String(value.getDate()).padStart(2, '0');
    return `${year}-${month}-${day}`;
  }
</script>

<svelte:head>
  <title>Screencap · Insights</title>
</svelte:head>

<section class="insights" aria-busy={loading}>
  <header class="insights__header">
    <p class="insights__eyebrow">Insights</p>
    <h2>Signal atlas</h2>
    <p class="insights__summary">
      Rolling context, hour-by-hour synthesis, and the daily picture in one place.
    </p>

    <label class="insights__date">
      <span>Date</span>
      <input type="date" value={selectedDate} on:change={handleDateChange} />
    </label>
  </header>

  {#if errorMessage}
    <p class="insights__error" role="status">{errorMessage}</p>
  {/if}

  <RollingContextCard {loading} context={rollingContext} />

  <section class="insights__section">
    <div class="insights__section-header">
      <h3>Hourly digests</h3>
      <p>{selectedDate}</p>
    </div>

    {#if loading}
      <div class="insights__skeleton-grid" aria-hidden="true">
        {#each Array.from({ length: 3 }, (_, index) => index) as skeleton (skeleton)}
          <article class="insights__skeleton"></article>
        {/each}
      </div>
    {:else if hourlyDigests.length === 0}
      <p class="insights__empty">No insights available for this day.</p>
    {:else}
      <div class="insights__hourly-grid">
        {#each hourlyDigests as digest (digest.id)}
          <HourlyDigestCard {digest} />
        {/each}
      </div>
    {/if}
  </section>

  <DailySummarySection {loading} summary={dailySummary} {selectedDate} />

  {#if hasNoInsights}
    <p class="insights__empty insights__empty--global">No insights available for this day.</p>
  {/if}
</section>

<style>
  .insights {
    height: 100%;
    overflow: auto;
    padding: clamp(1.1rem, 2.5vw, 1.9rem);
    display: grid;
    align-content: start;
    gap: 1rem;
    background:
      linear-gradient(160deg, rgb(25 31 47 / 95%), rgb(12 15 26 / 98%)),
      radial-gradient(circle at 86% 6%, rgb(112 255 227 / 18%), transparent 34%);
  }

  .insights__header {
    display: grid;
    gap: 0.55rem;
  }

  .insights__eyebrow {
    font-size: 0.68rem;
    text-transform: uppercase;
    letter-spacing: 0.22em;
    color: var(--pulse);
  }

  h2 {
    font-size: clamp(1.85rem, 4vw, 3rem);
  }

  .insights__summary {
    color: var(--paper-200);
    font-size: 0.9rem;
  }

  .insights__date {
    width: fit-content;
    display: grid;
    gap: 0.22rem;
    color: var(--paper-200);
    font-size: 0.74rem;
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }

  .insights__date input {
    border: 1px solid rgb(246 241 231 / 34%);
    border-radius: 0.66rem;
    background: rgb(8 12 18 / 66%);
    color: var(--paper-100);
    font: inherit;
    padding: 0.48rem 0.56rem;
  }

  .insights__error,
  .insights__empty {
    border: 1px solid rgb(255 179 71 / 34%);
    border-radius: 0.8rem;
    background: rgb(255 179 71 / 10%);
    padding: 0.7rem 0.86rem;
    color: var(--paper-200);
    font-size: 0.84rem;
  }

  .insights__empty {
    border-color: rgb(246 241 231 / 18%);
    background: rgb(8 12 18 / 62%);
  }

  .insights__empty--global {
    border-color: rgb(255 78 166 / 35%);
    background: rgb(255 78 166 / 10%);
  }

  .insights__section {
    display: grid;
    gap: 0.68rem;
  }

  .insights__section-header {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 0.7rem;
  }

  h3 {
    font-size: clamp(1.1rem, 2.4vw, 1.65rem);
  }

  .insights__section-header p {
    color: var(--paper-200);
    font-size: 0.72rem;
    letter-spacing: 0.11em;
    text-transform: uppercase;
  }

  .insights__hourly-grid,
  .insights__skeleton-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(16rem, 1fr));
    gap: 0.65rem;
  }

  .insights__skeleton {
    border-radius: 0.95rem;
    min-height: 8.8rem;
    background:
      linear-gradient(110deg, rgb(246 241 231 / 7%) 18%, rgb(246 241 231 / 17%) 32%, rgb(246 241 231 / 8%) 46%),
      rgb(9 13 22 / 76%);
    background-size: 240% 100%;
    animation: shimmer 1.4s linear infinite;
  }

  @keyframes shimmer {
    from {
      background-position: 200% 0;
    }

    to {
      background-position: -40% 0;
    }
  }
</style>
