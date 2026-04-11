<script lang="ts">
  import { onMount } from 'svelte';
  import { Chart, type ChartConfiguration, registerables } from 'chart.js';

  import {
    getCosts,
    getDailyInsightsRange,
    getProjectTimeAllocations,
    getStats,
    getTopicFrequencies,
    listCapturesInRange,
    type CaptureRecord,
    type CostBreakdown,
    type DailyInsight,
    type ProjectTimeAllocation,
    type SystemStats,
    type TopicFrequency,
  } from '$lib/api';

  Chart.register(...registerables);

  type DistributionPoint = {
    label: string;
    value: number;
  };

  type HeatmapCell = {
    date: string;
    value: number;
    level: 0 | 1 | 2 | 3 | 4;
    tooltip: string;
  };

  type CostPoint = {
    date: string;
    tokens: number;
    cents: number;
  };

  type CostView = {
    totalTokens: number;
    totalCents: number;
    extractionTokens: number;
    extractionCents: number;
    synthesisTokens: number;
    synthesisCents: number;
    perDayCents: number;
    monthCents: number;
    byDay: CostPoint[];
  };

  const DAY_MS = 86_400_000;

  const EMPTY_STATS: SystemStats = {
    capture_count: 0,
    captures_today: 0,
    storage_bytes: 0,
    uptime_secs: 0,
  };

  const EMPTY_COST_VIEW: CostView = {
    totalTokens: 0,
    totalCents: 0,
    extractionTokens: 0,
    extractionCents: 0,
    synthesisTokens: 0,
    synthesisCents: 0,
    perDayCents: 0,
    monthCents: 0,
    byDay: [],
  };

  const SHORT_DATE = new Intl.DateTimeFormat(undefined, {
    month: 'short',
    day: 'numeric',
  });

  const CURRENCY = new Intl.NumberFormat(undefined, {
    style: 'currency',
    currency: 'USD',
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  });

  let fromDate = formatLocalDate(new Date(Date.now() - 13 * DAY_MS));
  let toDate = formatLocalDate(new Date());

  let loading = true;
  let errorMessage: string | null = null;
  let requestVersion = 0;

  let stats: SystemStats = EMPTY_STATS;
  let projectSeries: DistributionPoint[] = [];
  let appSeries: DistributionPoint[] = [];
  let activitySeries: DistributionPoint[] = [];
  let heatmapSeries: HeatmapCell[] = [];
  let costView: CostView = EMPTY_COST_VIEW;
  let rangeCaptureCount = 0;

  let projectCanvas: HTMLCanvasElement | null = null;
  let appCanvas: HTMLCanvasElement | null = null;
  let activityCanvas: HTMLCanvasElement | null = null;

  let projectChart: Chart<'bar'> | null = null;
  let appChart: Chart<'doughnut'> | null = null;
  let activityChart: Chart<'bar'> | null = null;

  $: syncProjectChart(projectSeries, projectCanvas);
  $: syncAppChart(appSeries, appCanvas);
  $: syncActivityChart(activitySeries, activityCanvas);

  $: heatmapSlots = buildHeatmapSlots(heatmapSeries);

  onMount(() => {
    void loadBoard();

    return () => {
      destroyCharts();
    };
  });

  function applyDateRange(): void {
    void loadBoard();
  }

  function setQuickRange(days: number): void {
    const safeDays = Math.max(1, Math.trunc(days));
    toDate = formatLocalDate(new Date());
    fromDate = formatLocalDate(new Date(Date.now() - (safeDays - 1) * DAY_MS));
    void loadBoard();
  }

  async function loadBoard(): Promise<void> {
    const currentRequest = ++requestVersion;

    const validationError = validateDateRange(fromDate, toDate);
    if (validationError) {
      loading = false;
      errorMessage = validationError;
      return;
    }

    loading = true;
    errorMessage = null;

    const timestampRange = toTimestampRange(fromDate, toDate);

    const [
      statsResult,
      projectsResult,
      topicsResult,
      dailyInsightsResult,
      capturesResult,
      costsResult,
    ] = await Promise.allSettled([
      getStats(),
      getProjectTimeAllocations(timestampRange),
      getTopicFrequencies(timestampRange),
      getDailyInsightsRange({ from: fromDate, to: toDate }),
      listCapturesInRange(timestampRange, 500, 24),
      getCosts({ from: fromDate, to: toDate }),
    ]);

    if (currentRequest !== requestVersion) {
      return;
    }

    const errors: string[] = [];

    stats = statsResult.status === 'fulfilled' ? statsResult.value : EMPTY_STATS;
    if (statsResult.status === 'rejected') {
      errors.push('Stats unavailable.');
      console.error('Stats request failed', statsResult.reason);
    }

    const projectAllocations = projectsResult.status === 'fulfilled' ? projectsResult.value : [];
    if (projectsResult.status === 'rejected') {
      errors.push('Project insight data unavailable.');
      console.error('Project insight request failed', projectsResult.reason);
    }

    const topicFrequencies = topicsResult.status === 'fulfilled' ? topicsResult.value : [];
    if (topicsResult.status === 'rejected') {
      console.error('Topic insight request failed', topicsResult.reason);
    }

    const dailyInsights = dailyInsightsResult.status === 'fulfilled' ? dailyInsightsResult.value : [];
    if (dailyInsightsResult.status === 'rejected') {
      errors.push('Daily insight data unavailable.');
      console.error('Daily insight range request failed', dailyInsightsResult.reason);
    }

    const captures = capturesResult.status === 'fulfilled' ? capturesResult.value : [];
    if (capturesResult.status === 'rejected') {
      errors.push('Capture range data unavailable.');
      console.error('Capture range request failed', capturesResult.reason);
    }

    const costs = costsResult.status === 'fulfilled' ? costsResult.value : null;
    if (costsResult.status === 'rejected') {
      console.error('Cost request failed', costsResult.reason);
    }

    projectSeries = toProjectSeries(projectAllocations);
    appSeries = toAppSeries(captures);
    activitySeries = toActivitySeries(dailyInsights, captures, topicFrequencies);
    heatmapSeries = toHeatmapSeries(fromDate, toDate, dailyInsights, captures);
    costView = toCostView(costs, dailyInsights, toDate);

    rangeCaptureCount = captures.length;

    loading = false;
    errorMessage = errors.length > 0 ? errors.join(' ') : null;
  }

  function syncProjectChart(series: DistributionPoint[], canvas: HTMLCanvasElement | null): void {
    if (!canvas || series.length === 0) {
      projectChart?.destroy();
      projectChart = null;
      return;
    }

    const config: ChartConfiguration<'bar'> = {
      type: 'bar',
      data: {
        labels: series.map((entry) => entry.label),
        datasets: [
          {
            label: 'Captures',
            data: series.map((entry) => entry.value),
            backgroundColor: series.map(() => 'rgba(112, 255, 227, 0.72)'),
            borderColor: series.map(() => '#70ffe3'),
            borderWidth: 1.2,
            borderRadius: 7,
          },
        ],
      },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        animation: {
          duration: 280,
        },
        plugins: {
          legend: {
            display: false,
          },
          tooltip: {
            callbacks: {
              label(context) {
                const raw = Number(context.parsed.y ?? context.parsed.x ?? 0);
                return `${raw.toLocaleString()} captures`;
              },
            },
          },
        },
        scales: {
          x: {
            ticks: {
              color: '#f0e8db',
              maxRotation: 0,
              autoSkip: true,
              maxTicksLimit: 7,
            },
            grid: {
              color: 'rgba(246, 241, 231, 0.08)',
            },
          },
          y: {
            beginAtZero: true,
            ticks: {
              color: '#f0e8db',
              precision: 0,
            },
            grid: {
              color: 'rgba(246, 241, 231, 0.08)',
            },
          },
        },
      },
    };

    if (!projectChart) {
      projectChart = new Chart(canvas, config);
      return;
    }

    projectChart.data = config.data;
    projectChart.options = config.options ?? {};
    projectChart.update();
  }

  function syncAppChart(series: DistributionPoint[], canvas: HTMLCanvasElement | null): void {
    if (!canvas || series.length === 0) {
      appChart?.destroy();
      appChart = null;
      return;
    }

    const palette = [
      '#70ffe3',
      '#ff4ea6',
      '#ffb347',
      '#7ea6ff',
      '#be8bff',
      '#7ff8a4',
      '#ffd86a',
      '#86d6ff',
      '#f589ff',
      '#ffa16c',
    ];

    const config: ChartConfiguration<'doughnut'> = {
      type: 'doughnut',
      data: {
        labels: series.map((entry) => entry.label),
        datasets: [
          {
            data: series.map((entry) => entry.value),
            backgroundColor: series.map((_, index) => palette[index % palette.length]),
            borderColor: '#0b1018',
            borderWidth: 1.4,
          },
        ],
      },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        animation: {
          duration: 280,
        },
        plugins: {
          legend: {
            labels: {
              color: '#f0e8db',
              boxWidth: 10,
            },
            position: 'bottom',
          },
          tooltip: {
            callbacks: {
              label(context) {
                const value = Number(context.raw ?? 0);
                return `${context.label}: ${value.toLocaleString()} captures`;
              },
            },
          },
        },
      },
    };

    if (!appChart) {
      appChart = new Chart(canvas, config);
      return;
    }

    appChart.data = config.data;
    appChart.options = config.options ?? {};
    appChart.update();
  }

  function syncActivityChart(series: DistributionPoint[], canvas: HTMLCanvasElement | null): void {
    if (!canvas || series.length === 0) {
      activityChart?.destroy();
      activityChart = null;
      return;
    }

    const config: ChartConfiguration<'bar'> = {
      type: 'bar',
      data: {
        labels: series.map((entry) => entry.label),
        datasets: [
          {
            label: 'Occurrences',
            data: series.map((entry) => entry.value),
            backgroundColor: series.map((_, index) =>
              index % 2 === 0 ? 'rgba(255, 78, 166, 0.7)' : 'rgba(255, 179, 71, 0.7)'
            ),
            borderColor: series.map((_, index) => (index % 2 === 0 ? '#ff4ea6' : '#ffb347')),
            borderWidth: 1.2,
            borderRadius: 7,
          },
        ],
      },
      options: {
        indexAxis: 'y',
        responsive: true,
        maintainAspectRatio: false,
        animation: {
          duration: 280,
        },
        plugins: {
          legend: {
            display: false,
          },
          tooltip: {
            callbacks: {
              label(context) {
                const raw = Number(context.parsed.x ?? context.parsed.y ?? 0);
                return `${raw.toLocaleString()} events`;
              },
            },
          },
        },
        scales: {
          x: {
            beginAtZero: true,
            ticks: {
              color: '#f0e8db',
              precision: 0,
            },
            grid: {
              color: 'rgba(246, 241, 231, 0.08)',
            },
          },
          y: {
            ticks: {
              color: '#f0e8db',
            },
            grid: {
              color: 'rgba(246, 241, 231, 0.04)',
            },
          },
        },
      },
    };

    if (!activityChart) {
      activityChart = new Chart(canvas, config);
      return;
    }

    activityChart.data = config.data;
    activityChart.options = config.options ?? {};
    activityChart.update();
  }

  function destroyCharts(): void {
    projectChart?.destroy();
    appChart?.destroy();
    activityChart?.destroy();
    projectChart = null;
    appChart = null;
    activityChart = null;
  }

  function toProjectSeries(projects: ProjectTimeAllocation[]): DistributionPoint[] {
    return projects
      .map((entry) => ({
        label: entry.project?.trim() || 'Uncategorized',
        value: Math.max(0, Math.round(entry.capture_count)),
      }))
      .filter((entry) => entry.value > 0)
      .sort((left, right) => right.value - left.value)
      .slice(0, 10);
  }

  function toAppSeries(captures: CaptureRecord[]): DistributionPoint[] {
    const counts = new Map<string, number>();

    for (const capture of captures) {
      const key = capture.app_name?.trim() || 'Unlabeled app';
      counts.set(key, (counts.get(key) ?? 0) + 1);
    }

    return Array.from(counts.entries())
      .map(([label, value]) => ({ label, value }))
      .sort((left, right) => right.value - left.value)
      .slice(0, 8);
  }

  function toActivitySeries(
    insights: DailyInsight[],
    captures: CaptureRecord[],
    topics: TopicFrequency[]
  ): DistributionPoint[] {
    const counts = new Map<string, number>();

    for (const insight of insights) {
      const data = asRecord(insight.data);
      if (!data) {
        continue;
      }

      const rawProjects = data.projects;
      if (Array.isArray(rawProjects)) {
        for (const project of rawProjects) {
          const projectRecord = asRecord(project);
          if (!projectRecord) {
            continue;
          }

          for (const activity of readStringArray(projectRecord.activities)) {
            counts.set(activity, (counts.get(activity) ?? 0) + 1);
          }
        }
      }
    }

    if (counts.size === 0) {
      for (const capture of captures) {
        const activity =
          normalizeLabel(capture.primary_activity) ??
          normalizeLabel(capture.extraction_status) ??
          'captured';
        counts.set(activity, (counts.get(activity) ?? 0) + 1);
      }
    }

    if (counts.size === 0) {
      for (const topic of topics) {
        const label = normalizeLabel(topic.topic);
        if (!label) {
          continue;
        }

        counts.set(label, topic.capture_count);
      }
    }

    return Array.from(counts.entries())
      .map(([label, value]) => ({ label, value }))
      .sort((left, right) => right.value - left.value)
      .slice(0, 8);
  }

  function toHeatmapSeries(
    from: string,
    to: string,
    insights: DailyInsight[],
    captures: CaptureRecord[]
  ): HeatmapCell[] {
    const byDay = new Map<string, number>();

    for (const insight of insights) {
      const data = asRecord(insight.data);
      if (!data) {
        continue;
      }

      const date = normalizeLabel(asString(data.date)) ?? normalizeIsoDate(insight.window_start) ?? null;
      const totalActiveHours = asNumber(data.total_active_hours);

      if (!date || totalActiveHours === null) {
        continue;
      }

      byDay.set(date, Math.max(byDay.get(date) ?? 0, totalActiveHours));
    }

    if (byDay.size === 0) {
      const hourlyBuckets = new Map<string, Set<number>>();

      for (const capture of captures) {
        const capturedAt = new Date(capture.timestamp);
        if (!Number.isFinite(capturedAt.getTime())) {
          continue;
        }

        const date = formatLocalDate(capturedAt);
        const hour = capturedAt.getHours();

        const existing = hourlyBuckets.get(date) ?? new Set<number>();
        existing.add(hour);
        hourlyBuckets.set(date, existing);
      }

      for (const [date, hours] of hourlyBuckets.entries()) {
        byDay.set(date, hours.size);
      }
    }

    const days = enumerateDates(from, to);
    const maxValue = Math.max(1, ...days.map((date) => byDay.get(date) ?? 0));

    return days.map((date) => {
      const value = byDay.get(date) ?? 0;
      const level = value <= 0 ? 0 : (Math.min(4, Math.ceil((value / maxValue) * 4)) as HeatmapCell['level']);

      return {
        date,
        value,
        level,
        tooltip: `${date}: ${value.toFixed(1)} active hours`,
      };
    });
  }

  function toCostView(costs: CostBreakdown | null, insights: DailyInsight[], referenceDate: string): CostView {
    if (costs) {
      const byDay = costs.by_day
        .map((entry) => ({
          date: entry.date,
          tokens: Math.max(0, Math.round(entry.tokens_used)),
          cents: Number.isFinite(entry.reported_cost_cents) ? entry.reported_cost_cents : 0,
        }))
        .sort((left, right) => left.date.localeCompare(right.date));

      const dayCount = Math.max(byDay.length, 1);
      const monthKey = referenceDate.slice(0, 7);
      const monthCents = byDay
        .filter((entry) => entry.date.startsWith(monthKey))
        .reduce((total, entry) => total + entry.cents, 0);

      return {
        totalTokens: Math.max(0, Math.round(costs.total.tokens_used)),
        totalCents: Number.isFinite(costs.total.reported_cost_cents) ? costs.total.reported_cost_cents : 0,
        extractionTokens: Math.max(0, Math.round(costs.extraction.tokens_used)),
        extractionCents: Number.isFinite(costs.extraction.reported_cost_cents)
          ? costs.extraction.reported_cost_cents
          : 0,
        synthesisTokens: Math.max(0, Math.round(costs.synthesis.tokens_used)),
        synthesisCents: Number.isFinite(costs.synthesis.reported_cost_cents)
          ? costs.synthesis.reported_cost_cents
          : 0,
        perDayCents:
          (Number.isFinite(costs.total.reported_cost_cents) ? costs.total.reported_cost_cents : 0) / dayCount,
        monthCents,
        byDay,
      };
    }

    const byDayMap = new Map<string, CostPoint>();

    for (const insight of insights) {
      const data = asRecord(insight.data);
      const date =
        normalizeLabel(asString(data?.date)) ??
        normalizeIsoDate(insight.window_start) ??
        normalizeIsoDate(insight.window_end);

      if (!date) {
        continue;
      }

      const tokens = typeof insight.tokens_used === 'number' && Number.isFinite(insight.tokens_used) ? insight.tokens_used : 0;
      const cents = typeof insight.cost_cents === 'number' && Number.isFinite(insight.cost_cents) ? insight.cost_cents : 0;

      const existing = byDayMap.get(date) ?? { date, tokens: 0, cents: 0 };
      existing.tokens += Math.max(0, Math.round(tokens));
      existing.cents += cents;
      byDayMap.set(date, existing);
    }

    const byDay = Array.from(byDayMap.values()).sort((left, right) => left.date.localeCompare(right.date));
    const totalTokens = byDay.reduce((total, day) => total + day.tokens, 0);
    const totalCents = byDay.reduce((total, day) => total + day.cents, 0);
    const dayCount = Math.max(byDay.length, 1);
    const monthKey = referenceDate.slice(0, 7);
    const monthCents = byDay
      .filter((entry) => entry.date.startsWith(monthKey))
      .reduce((total, entry) => total + entry.cents, 0);

    return {
      totalTokens,
      totalCents,
      extractionTokens: 0,
      extractionCents: 0,
      synthesisTokens: totalTokens,
      synthesisCents: totalCents,
      perDayCents: totalCents / dayCount,
      monthCents,
      byDay,
    };
  }

  function buildHeatmapSlots(cells: HeatmapCell[]): Array<HeatmapCell | null> {
    if (cells.length === 0) {
      return [];
    }

    const firstDate = parseDateOnly(cells[0].date);
    const leading = firstDate ? firstDate.getDay() : 0;

    return [...Array.from({ length: leading }, () => null), ...cells];
  }

  function formatLocalDate(value: Date): string {
    const year = value.getFullYear();
    const month = String(value.getMonth() + 1).padStart(2, '0');
    const day = String(value.getDate()).padStart(2, '0');
    return `${year}-${month}-${day}`;
  }

  function validateDateRange(from: string, to: string): string | null {
    const parsedFrom = parseDateOnly(from);
    const parsedTo = parseDateOnly(to);

    if (!parsedFrom || !parsedTo) {
      return 'Choose a valid date range.';
    }

    if (parsedFrom.getTime() > parsedTo.getTime()) {
      return '`From` date must be before `To` date.';
    }

    return null;
  }

  function parseDateOnly(value: string): Date | null {
    const [yearRaw, monthRaw, dayRaw] = value.split('-').map((part) => Number(part));
    if (!Number.isFinite(yearRaw) || !Number.isFinite(monthRaw) || !Number.isFinite(dayRaw)) {
      return null;
    }

    const date = new Date(yearRaw, monthRaw - 1, dayRaw, 0, 0, 0, 0);
    return Number.isFinite(date.getTime()) ? date : null;
  }

  function toTimestampRange(from: string, to: string): { from: string; to: string } {
    const start = parseDateOnly(from) ?? new Date();
    const endStart = parseDateOnly(to) ?? new Date();
    const endExclusive = new Date(endStart.getTime() + DAY_MS);

    return {
      from: start.toISOString(),
      to: new Date(endExclusive.getTime() - 1).toISOString(),
    };
  }

  function enumerateDates(from: string, to: string): string[] {
    const start = parseDateOnly(from);
    const end = parseDateOnly(to);
    if (!start || !end) {
      return [];
    }

    const days: string[] = [];
    const cursor = new Date(start);

    while (cursor.getTime() <= end.getTime()) {
      days.push(formatLocalDate(cursor));
      cursor.setDate(cursor.getDate() + 1);
    }

    return days;
  }

  function normalizeLabel(value: unknown): string | null {
    if (typeof value !== 'string') {
      return null;
    }

    const trimmed = value.trim();
    return trimmed.length > 0 ? trimmed : null;
  }

  function normalizeIsoDate(value: unknown): string | null {
    if (typeof value !== 'string') {
      return null;
    }

    const parsed = new Date(value);
    if (!Number.isFinite(parsed.getTime())) {
      return null;
    }

    return formatLocalDate(parsed);
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

  function readStringArray(value: unknown): string[] {
    if (!Array.isArray(value)) {
      return [];
    }

    return value
      .map((entry) => (typeof entry === 'string' ? entry.trim() : ''))
      .filter((entry) => entry.length > 0);
  }

  function formatUptime(totalSeconds: number): string {
    const safeSeconds = Number.isFinite(totalSeconds) ? Math.max(0, Math.floor(totalSeconds)) : 0;
    const days = Math.floor(safeSeconds / 86_400);
    const hours = Math.floor((safeSeconds % 86_400) / 3_600);
    const minutes = Math.floor((safeSeconds % 3_600) / 60);

    if (days > 0) {
      return `${days}d ${hours}h ${minutes}m`;
    }

    if (hours > 0) {
      return `${hours}h ${minutes}m`;
    }

    return `${Math.max(minutes, 1)}m`;
  }

  function formatBytes(bytes: number): string {
    if (!Number.isFinite(bytes) || bytes <= 0) {
      return '0 MB';
    }

    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    const exponent = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
    const value = bytes / 1024 ** exponent;
    const precision = exponent >= 2 ? 1 : 0;
    return `${value.toFixed(precision)} ${units[exponent]}`;
  }

  function formatCost(cents: number): string {
    const safeCents = Number.isFinite(cents) ? cents : 0;
    return CURRENCY.format(safeCents / 100);
  }

  function formatDateLabel(rawDate: string): string {
    const parsed = parseDateOnly(rawDate);
    if (!parsed) {
      return rawDate;
    }

    return SHORT_DATE.format(parsed);
  }
</script>

<svelte:head>
  <title>Screencap · Stats</title>
</svelte:head>

<section class="stats" aria-busy={loading}>
  <header class="stats__header">
    <p class="stats__eyebrow">Telemetry Deck</p>
    <h2>Ops pulse matrix</h2>
    <p class="stats__summary">
      Project allocation, app usage, activity distribution, heatmap intensity, and cost telemetry in one
      range-controlled board.
    </p>

    <form class="stats__range" on:submit|preventDefault={applyDateRange}>
      <label>
        <span>From</span>
        <input type="date" bind:value={fromDate} />
      </label>
      <label>
        <span>To</span>
        <input type="date" bind:value={toDate} />
      </label>
      <button type="submit">Apply</button>
    </form>

    <div class="stats__quick-range">
      <button type="button" on:click={() => setQuickRange(7)}>7D</button>
      <button type="button" on:click={() => setQuickRange(30)}>30D</button>
      <button type="button" on:click={() => setQuickRange(90)}>90D</button>
    </div>
  </header>

  {#if errorMessage}
    <p class="stats__error" role="status">{errorMessage}</p>
  {/if}

  <section class="stats__summary-grid">
    <article class="stat-card">
      <p>Total captures</p>
      <strong>{stats.capture_count.toLocaleString()}</strong>
      <small>{rangeCaptureCount.toLocaleString()} in selected range</small>
    </article>
    <article class="stat-card">
      <p>Captured today</p>
      <strong>{stats.captures_today.toLocaleString()}</strong>
      <small>daemon uptime: {formatUptime(stats.uptime_secs)}</small>
    </article>
    <article class="stat-card">
      <p>Storage used</p>
      <strong>{formatBytes(stats.storage_bytes)}</strong>
      <small>local screenshot + metadata footprint</small>
    </article>
    <article class="stat-card">
      <p>Token volume</p>
      <strong>{costView.totalTokens.toLocaleString()}</strong>
      <small>{formatCost(costView.totalCents)} total reported spend</small>
    </article>
  </section>

  <section class="stats__chart-grid">
    <article class="panel-card">
      <header>
        <h3>Time per project</h3>
        <p>capture count proxy</p>
      </header>
      <div class="chart-shell">
        {#if loading}
          <p class="panel-card__state">Loading project allocation…</p>
        {:else if projectSeries.length === 0}
          <p class="panel-card__state">No project activity for this range.</p>
        {:else}
          <canvas bind:this={projectCanvas}></canvas>
        {/if}
      </div>
    </article>

    <article class="panel-card">
      <header>
        <h3>Time per app</h3>
        <p>capture distribution</p>
      </header>
      <div class="chart-shell">
        {#if loading}
          <p class="panel-card__state">Loading app usage…</p>
        {:else if appSeries.length === 0}
          <p class="panel-card__state">No app captures for this range.</p>
        {:else}
          <canvas bind:this={appCanvas}></canvas>
        {/if}
      </div>
    </article>

    <article class="panel-card">
      <header>
        <h3>Activity breakdown</h3>
        <p>daily activity signatures</p>
      </header>
      <div class="chart-shell chart-shell--wide">
        {#if loading}
          <p class="panel-card__state">Loading activity categories…</p>
        {:else if activitySeries.length === 0}
          <p class="panel-card__state">No activity labels available for this range.</p>
        {:else}
          <canvas bind:this={activityCanvas}></canvas>
        {/if}
      </div>
    </article>
  </section>

  <section class="panel-card panel-card--heatmap">
    <header>
      <h3>Daily active hours heatmap</h3>
      <p>GitHub-style cadence map</p>
    </header>

    {#if loading}
      <p class="panel-card__state">Rendering active-hours field…</p>
    {:else if heatmapSeries.length === 0}
      <p class="panel-card__state">No daily activity available for this range.</p>
    {:else}
      <div class="heatmap-wrap">
        <div class="heatmap-days" aria-hidden="true">
          <span>Sun</span>
          <span>Tue</span>
          <span>Thu</span>
          <span>Sat</span>
        </div>
        <div class="heatmap-grid" role="img" aria-label="Daily active hours heatmap">
          {#each heatmapSlots as slot, index (`${slot?.date ?? 'pad'}-${index}`)}
            {#if slot}
              <div class={`heatmap-cell level-${slot.level}`} title={slot.tooltip}></div>
            {:else}
              <div class="heatmap-cell heatmap-cell--blank" aria-hidden="true"></div>
            {/if}
          {/each}
        </div>
      </div>
    {/if}
  </section>

  <section class="stats__cost-grid">
    <article class="panel-card">
      <header>
        <h3>Cost tracking</h3>
        <p>reported model cost</p>
      </header>

      {#if loading}
        <p class="panel-card__state">Loading cost telemetry…</p>
      {:else}
        <dl class="cost-metrics">
          <div>
            <dt>Total cost</dt>
            <dd>{formatCost(costView.totalCents)}</dd>
          </div>
          <div>
            <dt>Avg / day</dt>
            <dd>{formatCost(costView.perDayCents)}</dd>
          </div>
          <div>
            <dt>Current month</dt>
            <dd>{formatCost(costView.monthCents)}</dd>
          </div>
          <div>
            <dt>Tokens used</dt>
            <dd>{costView.totalTokens.toLocaleString()}</dd>
          </div>
        </dl>

        <div class="cost-split">
          <article>
            <p>Extraction</p>
            <strong>{formatCost(costView.extractionCents)}</strong>
            <small>{costView.extractionTokens.toLocaleString()} tokens</small>
          </article>
          <article>
            <p>Synthesis</p>
            <strong>{formatCost(costView.synthesisCents)}</strong>
            <small>{costView.synthesisTokens.toLocaleString()} tokens</small>
          </article>
        </div>
      {/if}
    </article>

    <article class="panel-card">
      <header>
        <h3>Cost by day</h3>
        <p>rolling daily spend</p>
      </header>

      {#if loading}
        <p class="panel-card__state">Loading daily cost points…</p>
      {:else if costView.byDay.length === 0}
        <p class="panel-card__state">No reported cost points in this range.</p>
      {:else}
        <ul class="cost-day-list">
          {#each costView.byDay.slice(-8).reverse() as point (point.date)}
            <li>
              <span>{formatDateLabel(point.date)}</span>
              <strong>{formatCost(point.cents)}</strong>
              <small>{point.tokens.toLocaleString()} tokens</small>
            </li>
          {/each}
        </ul>
      {/if}
    </article>
  </section>
</section>

<style>
  .stats {
    height: 100%;
    overflow: auto;
    padding: clamp(1.1rem, 2.5vw, 2rem);
    display: grid;
    align-content: start;
    gap: 0.9rem;
    background:
      radial-gradient(circle at 10% 8%, rgb(112 255 227 / 18%), transparent 30%),
      radial-gradient(circle at 90% 14%, rgb(255 78 166 / 14%), transparent 34%),
      linear-gradient(150deg, rgb(23 28 42 / 96%), rgb(10 13 21 / 98%));
  }

  .stats__header {
    display: grid;
    gap: 0.55rem;
  }

  .stats__eyebrow {
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.22em;
    color: var(--pulse);
  }

  h2 {
    font-size: clamp(1.95rem, 4.2vw, 3.3rem);
  }

  .stats__summary {
    color: var(--paper-200);
    font-size: 0.88rem;
    max-width: 64ch;
  }

  .stats__range {
    width: fit-content;
    display: grid;
    grid-template-columns: repeat(3, auto);
    gap: 0.5rem;
    align-items: end;
  }

  .stats__range label {
    display: grid;
    gap: 0.2rem;
    font-size: 0.69rem;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--paper-200);
  }

  .stats__range input,
  .stats__range button,
  .stats__quick-range button {
    border: 1px solid rgb(246 241 231 / 32%);
    border-radius: 0.62rem;
    background: rgb(11 15 23 / 68%);
    color: var(--paper-100);
    font: inherit;
    padding: 0.45rem 0.56rem;
  }

  .stats__range button,
  .stats__quick-range button {
    cursor: pointer;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    font-size: 0.68rem;
    transition: transform 150ms ease, border-color 150ms ease, box-shadow 150ms ease;
  }

  .stats__range button:hover,
  .stats__range button:focus-visible,
  .stats__quick-range button:hover,
  .stats__quick-range button:focus-visible {
    transform: translate(0.14rem, -0.14rem);
    border-color: var(--pulse);
    box-shadow: 0.28rem 0.28rem 0 rgb(112 255 227 / 26%);
    outline: none;
  }

  .stats__quick-range {
    display: flex;
    flex-wrap: wrap;
    gap: 0.42rem;
  }

  .stats__error {
    border: 1px solid rgb(255 179 71 / 34%);
    border-radius: 0.8rem;
    background: rgb(255 179 71 / 12%);
    padding: 0.68rem 0.85rem;
    color: var(--paper-200);
    font-size: 0.84rem;
  }

  .stats__summary-grid {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 0.64rem;
  }

  .stat-card {
    border: 1px solid rgb(246 241 231 / 32%);
    border-radius: 0.9rem;
    background: linear-gradient(165deg, rgb(13 17 28 / 90%), rgb(10 13 20 / 64%));
    padding: 0.82rem;
    display: grid;
    gap: 0.34rem;
  }

  .stat-card p,
  .stat-card small {
    color: var(--paper-200);
    font-size: 0.68rem;
    text-transform: uppercase;
    letter-spacing: 0.11em;
  }

  .stat-card strong {
    font-family: var(--display-font);
    font-size: clamp(1.2rem, 2.4vw, 2rem);
    letter-spacing: 0.03em;
  }

  .stats__chart-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.68rem;
  }

  .panel-card {
    border: 1px solid rgb(246 241 231 / 30%);
    border-radius: 1rem;
    background: rgb(9 12 20 / 72%);
    padding: 0.86rem;
    display: grid;
    gap: 0.66rem;
    align-content: start;
  }

  .panel-card header {
    display: flex;
    justify-content: space-between;
    gap: 0.6rem;
    align-items: baseline;
  }

  h3 {
    font-size: clamp(1rem, 2.1vw, 1.45rem);
  }

  .panel-card header p {
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--paper-200);
  }

  .chart-shell {
    min-height: 15.4rem;
    position: relative;
  }

  .chart-shell--wide {
    min-height: 16.6rem;
  }

  .panel-card__state {
    border: 1px dashed rgb(246 241 231 / 24%);
    border-radius: 0.8rem;
    background: rgb(8 11 18 / 62%);
    color: var(--paper-200);
    font-size: 0.84rem;
    padding: 0.72rem 0.84rem;
  }

  .panel-card--heatmap {
    gap: 0.78rem;
  }

  .heatmap-wrap {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 0.56rem;
    align-items: start;
  }

  .heatmap-days {
    display: grid;
    grid-template-rows: repeat(4, 1fr);
    gap: 0.26rem;
    color: var(--paper-200);
    font-size: 0.64rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    padding-top: 0.06rem;
  }

  .heatmap-grid {
    display: grid;
    grid-auto-flow: column;
    grid-template-rows: repeat(7, 0.76rem);
    grid-auto-columns: 0.76rem;
    gap: 0.2rem;
    overflow-x: auto;
    padding-bottom: 0.15rem;
  }

  .heatmap-cell {
    border-radius: 0.14rem;
    border: 1px solid rgb(246 241 231 / 12%);
    background: rgb(11 14 22 / 84%);
  }

  .heatmap-cell.level-1 {
    background: rgb(112 255 227 / 28%);
    border-color: rgb(112 255 227 / 40%);
  }

  .heatmap-cell.level-2 {
    background: rgb(112 255 227 / 46%);
    border-color: rgb(112 255 227 / 58%);
  }

  .heatmap-cell.level-3 {
    background: rgb(255 179 71 / 52%);
    border-color: rgb(255 179 71 / 66%);
  }

  .heatmap-cell.level-4 {
    background: rgb(255 78 166 / 58%);
    border-color: rgb(255 78 166 / 70%);
  }

  .heatmap-cell--blank {
    opacity: 0.22;
  }

  .stats__cost-grid {
    display: grid;
    grid-template-columns: minmax(0, 1.35fr) minmax(0, 1fr);
    gap: 0.68rem;
  }

  .cost-metrics {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.45rem;
    margin: 0;
  }

  .cost-metrics div {
    border: 1px solid rgb(246 241 231 / 18%);
    border-radius: 0.68rem;
    background: rgb(13 16 27 / 58%);
    padding: 0.55rem;
    display: grid;
    gap: 0.2rem;
  }

  .cost-metrics dt {
    color: var(--paper-200);
    font-size: 0.65rem;
    text-transform: uppercase;
    letter-spacing: 0.1em;
  }

  .cost-metrics dd {
    margin: 0;
    font-family: var(--display-font);
    font-size: 1.08rem;
  }

  .cost-split {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.45rem;
  }

  .cost-split article {
    border: 1px solid rgb(246 241 231 / 18%);
    border-radius: 0.68rem;
    background: linear-gradient(160deg, rgb(112 255 227 / 12%), rgb(255 78 166 / 8%));
    padding: 0.56rem;
    display: grid;
    gap: 0.2rem;
  }

  .cost-split p,
  .cost-split small {
    color: var(--paper-200);
    font-size: 0.65rem;
    text-transform: uppercase;
    letter-spacing: 0.1em;
  }

  .cost-split strong {
    font-family: var(--display-font);
    font-size: 1.02rem;
  }

  .cost-day-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.38rem;
  }

  .cost-day-list li {
    border: 1px solid rgb(246 241 231 / 18%);
    border-radius: 0.68rem;
    background: rgb(12 16 25 / 56%);
    padding: 0.52rem 0.56rem;
    display: grid;
    grid-template-columns: auto auto 1fr;
    gap: 0.56rem;
    align-items: baseline;
  }

  .cost-day-list span,
  .cost-day-list small {
    color: var(--paper-200);
    font-size: 0.69rem;
    text-transform: uppercase;
    letter-spacing: 0.1em;
  }

  .cost-day-list strong {
    font-family: var(--display-font);
    font-size: 0.95rem;
  }

  @media (width <= 1080px) {
    .stats__summary-grid {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }

    .stats__cost-grid {
      grid-template-columns: 1fr;
    }
  }

  @media (width <= 880px) {
    .stats__chart-grid {
      grid-template-columns: 1fr;
    }

    .stats__range {
      grid-template-columns: 1fr;
      width: 100%;
    }
  }
</style>
