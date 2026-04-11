<script lang="ts">
  import { onMount } from 'svelte';

  import DailySummaryCard from './DailySummaryCard.svelte';
  import RollingContextCard from './RollingContextCard.svelte';
  import type {
    DailySummaryView,
    FocusBlockView,
    InsightEnvelope,
    ProjectBreakdownView,
    RollingContextView,
  } from './types';

  const DAILY_ENDPOINT = '/api/insights/daily';
  const ROLLING_ENDPOINT = '/api/insights/rolling';
  const LEGACY_ROLLING_ENDPOINT = '/api/insights/current';

  const FOCUS_QUALITY_SCORES: Record<string, number> = {
    'deep-focus': 0.92,
    deep: 0.92,
    focused: 0.88,
    moderate: 0.66,
    'moderate-focus': 0.66,
    shallow: 0.34,
    fragmented: 0.3,
    distracted: 0.24,
  };

  const FOCUS_TINTS: Record<string, string> = {
    'deep-focus': '#70ffe3',
    deep: '#70ffe3',
    focused: '#70ffe3',
    moderate: '#ffb347',
    'moderate-focus': '#ffb347',
    shallow: '#ff4ea6',
    fragmented: '#ff4ea6',
    distracted: '#ff4ea6',
  };

  let selectedDate = formatDateInput(new Date());
  let loading = true;
  let errorMessage = '';
  let rollingContext: RollingContextView | null = null;
  let dailySummary: DailySummaryView | null = null;
  let requestVersion = 0;

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
    void loadInsights(selectedDate);
  }

  async function loadInsights(date: string): Promise<void> {
    const currentRequest = ++requestVersion;
    loading = true;
    errorMessage = '';

    const [dailyResult, rollingResult] = await Promise.allSettled([
      fetchDailyInsight(date),
      fetchRollingInsight(),
    ]);

    if (currentRequest !== requestVersion) {
      return;
    }

    const errors: string[] = [];

    if (dailyResult.status === 'fulfilled') {
      dailySummary = normalizeDailySummary(dailyResult.value);
    } else {
      dailySummary = null;
      errors.push('Could not load daily summary.');
      console.error('Failed to load daily insights', dailyResult.reason);
    }

    if (rollingResult.status === 'fulfilled') {
      rollingContext = normalizeRollingContext(rollingResult.value);
    } else {
      rollingContext = null;
      errors.push('Could not load rolling context.');
      console.error('Failed to load rolling insights', rollingResult.reason);
    }

    errorMessage = errors.join(' ');
    loading = false;
  }

  async function fetchDailyInsight(date: string): Promise<InsightEnvelope | null> {
    const params = new URLSearchParams({ date });
    return fetchInsightEnvelope(`${DAILY_ENDPOINT}?${params.toString()}`);
  }

  async function fetchRollingInsight(): Promise<InsightEnvelope | null> {
    const rolling = await fetchInsightEnvelope(ROLLING_ENDPOINT);
    if (rolling) {
      return rolling;
    }

    return fetchInsightEnvelope(LEGACY_ROLLING_ENDPOINT);
  }

  async function fetchInsightEnvelope(path: string): Promise<InsightEnvelope | null> {
    const response = await fetch(path, {
      headers: {
        Accept: 'application/json',
      },
    });

    if (response.status === 404) {
      return null;
    }

    if (!response.ok) {
      throw new Error(`request to ${path} failed (${response.status})`);
    }

    const payload: unknown = await response.json();
    if (!isInsightEnvelope(payload)) {
      return null;
    }

    return payload;
  }

  function normalizeRollingContext(insight: InsightEnvelope | null): RollingContextView | null {
    if (!insight) {
      return null;
    }

    const data = asRecord(insight.data);
    if (!data) {
      return null;
    }

    const currentFocus =
      asString(data.current_focus) ??
      asString(data.currentFocus) ??
      asString(data.summary) ??
      asString(insight.narrative);

    if (!currentFocus) {
      return null;
    }

    const activeProject = asString(data.active_project) ?? asString(data.activeProject);
    const summary = asString(data.summary) ?? asString(insight.narrative);
    const mood = asString(data.mood);

    const appsUsageRaw = data.apps_used ?? data.appsUsed;
    const appsUsed = normalizeAppsUsed(appsUsageRaw);

    return {
      currentFocus,
      activeProject,
      summary,
      mood,
      appsUsed,
    };
  }

  function normalizeAppsUsed(value: unknown): { name: string; share: string }[] {
    if (Array.isArray(value)) {
      return value
        .map((entry) => {
          const record = asRecord(entry);
          if (!record) {
            return null;
          }

          const name = asString(record.name) ?? asString(record.app) ?? asString(record.app_name);
          if (!name) {
            return null;
          }

          const share = asString(record.share) ?? asString(record.percentage) ?? asString(record.value) ?? 'active';
          return {
            name,
            share,
          };
        })
        .filter((entry): entry is { name: string; share: string } => entry !== null);
    }

    const map = asRecord(value);
    if (!map) {
      return [];
    }

    return Object.entries(map)
      .map(([name, rawShare]) => {
        if (typeof rawShare === 'number' && Number.isFinite(rawShare)) {
          const normalized = rawShare <= 1 ? rawShare * 100 : rawShare;
          return {
            name,
            share: `${Math.round(normalized)}%`,
          };
        }

        const text = asString(rawShare);
        if (!text) {
          return null;
        }

        return {
          name,
          share: text,
        };
      })
      .filter((entry): entry is { name: string; share: string } => entry !== null)
      .slice(0, 6);
  }

  function normalizeDailySummary(insight: InsightEnvelope | null): DailySummaryView | null {
    if (!insight) {
      return null;
    }

    const data = asRecord(insight.data);
    if (!data) {
      return null;
    }

    const projects = parseProjects(data);
    const totalMinutes = deriveTotalMinutes(data, projects);
    const focusBlocks = parseFocusBlocks(data);
    const focusScore = deriveFocusScore(data, focusBlocks);
    const keyMoments = parseKeyMoments(data, projects);
    const openThreads = readStringArray(data.open_threads ?? data.openThreads);

    const narrative =
      asString(data.narrative) ?? asString(data.summary) ?? asString(insight.narrative) ?? null;

    const hasSignal =
      totalMinutes > 0 ||
      projects.length > 0 ||
      focusBlocks.length > 0 ||
      keyMoments.length > 0 ||
      openThreads.length > 0 ||
      Boolean(narrative);

    if (!hasSignal) {
      return null;
    }

    return {
      totalMinutes,
      totalLabel: formatDuration(totalMinutes),
      focusScore,
      focusScoreLabel: focusScore === null ? '—' : `${Math.round(focusScore * 100)}%`,
      narrative,
      keyMoments,
      openThreads,
      projects,
      focusBlocks,
    };
  }

  function parseProjects(data: Record<string, unknown>): ProjectBreakdownView[] {
    const entries = new Map<string, { minutes: number; accomplishments: Set<string> }>();

    const rawProjects = Array.isArray(data.projects) ? data.projects : [];
    for (const rawProject of rawProjects) {
      const project = asRecord(rawProject);
      if (!project) {
        continue;
      }

      const name =
        asString(project.name) ?? asString(project.project) ?? asString(project.label) ?? 'Uncategorized';

      const minutes =
        parseMinutes(project.total_minutes) ??
        parseMinutes(project.minutes) ??
        parseHours(project.total_hours) ??
        parseHours(project.hours) ??
        0;

      const accomplishments = readStringArray(
        project.key_accomplishments ?? project.keyMoments ?? project.accomplishments
      );

      mergeProject(entries, name, minutes, accomplishments);
    }

    const projectBreakdown = asRecord(data.project_breakdown ?? data.projectBreakdown);
    if (projectBreakdown) {
      for (const [name, rawValue] of Object.entries(projectBreakdown)) {
        const minutes = parseMinutes(rawValue) ?? parseDurationString(rawValue) ?? 0;
        mergeProject(entries, normalizeLabel(name), minutes, []);
      }
    }

    const allocation = asRecord(data.time_allocation ?? data.timeAllocation);
    if (allocation) {
      for (const [name, rawValue] of Object.entries(allocation)) {
        const minutes = parseDurationString(rawValue) ?? parseMinutes(rawValue) ?? 0;
        if (minutes > 0) {
          mergeProject(entries, normalizeLabel(name), minutes, []);
        }
      }
    }

    return Array.from(entries.entries())
      .map(([name, value]) => ({
        name,
        minutes: value.minutes,
        durationLabel: formatDuration(value.minutes),
        accomplishments: Array.from(value.accomplishments),
      }))
      .filter((project) => project.minutes > 0 || project.accomplishments.length > 0)
      .sort((left, right) => right.minutes - left.minutes)
      .slice(0, 8);
  }

  function mergeProject(
    bucket: Map<string, { minutes: number; accomplishments: Set<string> }>,
    name: string,
    minutes: number,
    accomplishments: string[]
  ): void {
    const safeName = name.trim() || 'Uncategorized';
    const clampedMinutes = Math.max(0, Math.round(minutes));

    const existing = bucket.get(safeName);
    if (!existing) {
      bucket.set(safeName, {
        minutes: clampedMinutes,
        accomplishments: new Set(accomplishments.filter(Boolean)),
      });
      return;
    }

    existing.minutes += clampedMinutes;
    for (const accomplishment of accomplishments) {
      if (accomplishment) {
        existing.accomplishments.add(accomplishment);
      }
    }
  }

  function deriveTotalMinutes(data: Record<string, unknown>, projects: ProjectBreakdownView[]): number {
    const fromHours = parseHours(data.total_active_hours ?? data.totalActiveHours);
    if (fromHours !== null) {
      return fromHours;
    }

    const fromMinutes = parseMinutes(data.total_active_minutes ?? data.totalActiveMinutes);
    if (fromMinutes !== null) {
      return fromMinutes;
    }

    return projects.reduce((sum, project) => sum + project.minutes, 0);
  }

  function parseFocusBlocks(data: Record<string, unknown>): FocusBlockView[] {
    const blocks = Array.isArray(data.focus_blocks) ? data.focus_blocks : [];

    return blocks
      .map((rawBlock) => {
        const block = asRecord(rawBlock);
        if (!block) {
          return null;
        }

        const minutes = parseMinutes(block.duration_min) ?? parseMinutes(block.minutes) ?? 0;
        if (minutes <= 0) {
          return null;
        }

        const quality = asString(block.quality) ?? 'focused';
        const normalizedQuality = quality.toLowerCase();

        const project =
          asString(block.project) ?? asString(block.name) ?? asString(block.focus) ?? 'Focus block';

        const start = asString(block.start) ?? '';
        const end = asString(block.end) ?? '';

        return {
          project,
          minutes,
          label: start && end ? `${start}–${end}` : formatDuration(minutes),
          quality,
          tint: FOCUS_TINTS[normalizedQuality] ?? '#70ffe3',
        } satisfies FocusBlockView;
      })
      .filter((entry): entry is FocusBlockView => entry !== null)
      .slice(0, 10);
  }

  function deriveFocusScore(data: Record<string, unknown>, blocks: FocusBlockView[]): number | null {
    const directScore = asNumber(data.focus_score ?? data.focusScore);
    if (directScore !== null) {
      if (directScore > 1) {
        return Math.min(1, directScore / 100);
      }

      return Math.max(0, Math.min(1, directScore));
    }

    if (blocks.length === 0) {
      return null;
    }

    const weighted = blocks.reduce(
      (accumulator, block) => {
        const key = block.quality.toLowerCase();
        const score = FOCUS_QUALITY_SCORES[key] ?? 0.58;

        return {
          scoreSum: accumulator.scoreSum + score * block.minutes,
          minuteSum: accumulator.minuteSum + block.minutes,
        };
      },
      { scoreSum: 0, minuteSum: 0 }
    );

    if (weighted.minuteSum <= 0) {
      return null;
    }

    return weighted.scoreSum / weighted.minuteSum;
  }

  function parseKeyMoments(data: Record<string, unknown>, projects: ProjectBreakdownView[]): string[] {
    const direct = readStringArray(data.key_moments ?? data.keyMoments);
    if (direct.length > 0) {
      return direct.slice(0, 6);
    }

    const fromProjects = projects.flatMap((project) => project.accomplishments);
    return Array.from(new Set(fromProjects)).slice(0, 6);
  }

  function parseDurationString(value: unknown): number | null {
    if (typeof value !== 'string') {
      return null;
    }

    const trimmed = value.trim();
    if (!trimmed) {
      return null;
    }

    const hoursMatch = /(-?\d+(?:\.\d+)?)\s*h/i.exec(trimmed);
    const minutesMatch = /(-?\d+(?:\.\d+)?)\s*m/i.exec(trimmed);

    let totalMinutes = 0;

    if (hoursMatch) {
      totalMinutes += Number(hoursMatch[1]) * 60;
    }

    if (minutesMatch) {
      totalMinutes += Number(minutesMatch[1]);
    }

    if (totalMinutes > 0) {
      return Math.round(totalMinutes);
    }

    const maybeNumeric = Number(trimmed);
    if (Number.isFinite(maybeNumeric) && maybeNumeric > 0) {
      return Math.round(maybeNumeric);
    }

    return null;
  }

  function parseHours(value: unknown): number | null {
    const numeric = asNumber(value);
    if (numeric === null) {
      return null;
    }

    if (numeric <= 0) {
      return 0;
    }

    return Math.round(numeric * 60);
  }

  function parseMinutes(value: unknown): number | null {
    const numeric = asNumber(value);
    if (numeric === null) {
      return null;
    }

    if (numeric <= 0) {
      return 0;
    }

    return Math.round(numeric);
  }

  function formatDuration(totalMinutes: number): string {
    const safeMinutes = Math.max(0, Math.round(totalMinutes));
    const hours = Math.floor(safeMinutes / 60);
    const minutes = safeMinutes % 60;

    if (hours === 0) {
      return `${Math.max(minutes, 1)}m`;
    }

    if (minutes === 0) {
      return `${hours}h`;
    }

    return `${hours}h ${minutes}m`;
  }

  function normalizeLabel(raw: string): string {
    const cleaned = raw
      .replace(/[_-]+/g, ' ')
      .trim()
      .replace(/\s+/g, ' ');

    if (!cleaned) {
      return 'Uncategorized';
    }

    return cleaned.charAt(0).toUpperCase() + cleaned.slice(1);
  }

  function readStringArray(value: unknown): string[] {
    if (!Array.isArray(value)) {
      return [];
    }

    return value
      .map((item) => (typeof item === 'string' ? item.trim() : ''))
      .filter((item) => item.length > 0);
  }

  function asRecord(value: unknown): Record<string, unknown> | null {
    if (typeof value !== 'object' || value === null) {
      return null;
    }

    return value as Record<string, unknown>;
  }

  function asString(value: unknown): string | null {
    if (typeof value !== 'string') {
      return null;
    }

    const trimmed = value.trim();
    return trimmed.length > 0 ? trimmed : null;
  }

  function asNumber(value: unknown): number | null {
    if (typeof value === 'number' && Number.isFinite(value)) {
      return value;
    }

    if (typeof value === 'string') {
      const parsed = Number(value.trim());
      if (Number.isFinite(parsed)) {
        return parsed;
      }
    }

    return null;
  }

  function isInsightEnvelope(value: unknown): value is InsightEnvelope {
    const record = asRecord(value);
    if (!record) {
      return false;
    }

    return (
      typeof record.id === 'number' &&
      typeof record.insight_type === 'string' &&
      asRecord(record.data) !== null
    );
  }

  function formatDateInput(date: Date): string {
    const year = date.getFullYear();
    const month = String(date.getMonth() + 1).padStart(2, '0');
    const day = String(date.getDate()).padStart(2, '0');
    return `${year}-${month}-${day}`;
  }
</script>

<section class="panel" aria-busy={loading}>
  <header class="panel__header">
    <p class="panel__section">Insights</p>
    <h2>Daily cognition board</h2>
    <p class="panel__summary">
      Synthesized activity for any day plus rolling context from the latest capture window.
    </p>
  </header>

  <form class="date-picker" on:submit|preventDefault>
    <label for="insights-date">Day</label>
    <input
      id="insights-date"
      name="insights-date"
      type="date"
      max={formatDateInput(new Date())}
      value={selectedDate}
      on:change={handleDateChange}
    />
  </form>

  {#if errorMessage}
    <p class="panel__error" role="alert">{errorMessage}</p>
  {/if}

  <div class="insights-grid">
    <RollingContextCard context={rollingContext} {loading} />
    <DailySummaryCard summary={dailySummary} selectedDate={selectedDate} {loading} />
  </div>
</section>

<style>
  .panel {
    height: 100%;
    padding: clamp(1.2rem, 2.6vw, 2rem);
    display: grid;
    align-content: start;
    gap: 1rem;
    overflow: auto;
    background:
      linear-gradient(150deg, rgb(26 17 35 / 96%), rgb(12 16 24 / 98%)),
      radial-gradient(circle at 18% 5%, rgb(255 78 166 / 16%), transparent 32%),
      radial-gradient(circle at 88% 86%, rgb(112 255 227 / 10%), transparent 34%);
  }

  .panel__header {
    display: grid;
    gap: 0.52rem;
  }

  .panel__section {
    margin: 0;
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.22em;
    color: var(--pulse);
  }

  h2 {
    font-size: clamp(1.95rem, 4.2vw, 3.25rem);
  }

  .panel__summary {
    margin: 0;
    color: var(--paper-200);
    font-size: 0.88rem;
  }

  .date-picker {
    width: fit-content;
    border: 1px solid rgb(246 241 231 / 34%);
    border-radius: 0.95rem;
    padding: 0.65rem 0.78rem;
    display: grid;
    gap: 0.35rem;
    background: rgb(8 11 18 / 72%);
  }

  .date-picker label {
    font-size: 0.68rem;
    text-transform: uppercase;
    letter-spacing: 0.16em;
    color: var(--paper-200);
  }

  .date-picker input {
    border: 1px solid rgb(112 255 227 / 42%);
    border-radius: 0.62rem;
    padding: 0.43rem 0.58rem;
    background: rgb(11 14 22 / 92%);
    color: var(--paper-100);
    font-family: var(--display-font);
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  .date-picker input:focus-visible {
    outline: 2px solid var(--surge);
    outline-offset: 2px;
  }

  .panel__error {
    margin: 0;
    color: #ff9494;
    border: 1px solid rgb(255 148 148 / 55%);
    border-radius: 0.7rem;
    padding: 0.55rem 0.76rem;
    background: rgb(65 19 24 / 52%);
    font-size: 0.84rem;
  }

  .insights-grid {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1.15fr);
    gap: 0.86rem;
  }

  @media (width <= 980px) {
    .insights-grid {
      grid-template-columns: 1fr;
    }

    .date-picker {
      width: 100%;
    }
  }
</style>
