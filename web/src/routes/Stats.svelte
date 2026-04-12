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

  function chartTextColor(): string {
    if (typeof document === 'undefined') return '#64748b';
    return document.documentElement.classList.contains('dark') ? '#94a3b8' : '#64748b';
  }

  function chartGridColor(): string {
    if (typeof document === 'undefined') return 'rgba(0,0,0,0.06)';
    return document.documentElement.classList.contains('dark') ? 'rgba(255,255,255,0.08)' : 'rgba(0,0,0,0.06)';
  }

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

    const txtColor = chartTextColor();
    const gridColor = chartGridColor();

    const config: ChartConfiguration<'bar'> = {
      type: 'bar',
      data: {
        labels: series.map((entry) => entry.label),
        datasets: [
          {
            label: 'Captures',
            data: series.map((entry) => entry.value),
            backgroundColor: 'rgba(0, 88, 188, 0.65)',
            borderColor: '#0058bc',
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
              color: txtColor,
              maxRotation: 0,
              autoSkip: true,
              maxTicksLimit: 7,
            },
            grid: {
              color: gridColor,
            },
          },
          y: {
            beginAtZero: true,
            ticks: {
              color: txtColor,
              precision: 0,
            },
            grid: {
              color: gridColor,
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
      '#0058bc',
      '#8a2bb9',
      '#e67700',
      '#0d9488',
      '#6366f1',
      '#059669',
      '#d97706',
      '#2563eb',
      '#c026d3',
      '#ea580c',
    ];

    const txtColor = chartTextColor();

    const config: ChartConfiguration<'doughnut'> = {
      type: 'doughnut',
      data: {
        labels: series.map((entry) => entry.label),
        datasets: [
          {
            data: series.map((entry) => entry.value),
            backgroundColor: series.map((_, index) => palette[index % palette.length]),
            borderColor: 'transparent',
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
              color: txtColor,
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

    const txtColor = chartTextColor();
    const gridColor = chartGridColor();

    const config: ChartConfiguration<'bar'> = {
      type: 'bar',
      data: {
        labels: series.map((entry) => entry.label),
        datasets: [
          {
            label: 'Occurrences',
            data: series.map((entry) => entry.value),
            backgroundColor: series.map((_, index) =>
              index % 2 === 0 ? 'rgba(138, 43, 185, 0.65)' : 'rgba(230, 119, 0, 0.65)'
            ),
            borderColor: series.map((_, index) => (index % 2 === 0 ? '#8a2bb9' : '#e67700')),
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
              color: txtColor,
              precision: 0,
            },
            grid: {
              color: gridColor,
            },
          },
          y: {
            ticks: {
              color: txtColor,
            },
            grid: {
              color: gridColor,
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
          asString(capture.primary_activity) ??
          asString(capture.extraction_status) ??
          'captured';
        counts.set(activity, (counts.get(activity) ?? 0) + 1);
      }
    }

    if (counts.size === 0) {
      for (const topic of topics) {
        const label = asString(topic.topic);
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

      const date = asString(data.date) ?? normalizeIsoDate(insight.window_start) ?? null;
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
        asString(data?.date) ??
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

<section class="min-h-full overflow-auto bg-background p-6 font-sans text-on-surface md:p-8 lg:p-10" aria-busy={loading}>
  <header class="mb-8 space-y-6">
    <div>
      <p class="mb-1 text-[0.7rem] font-semibold uppercase tracking-[0.22em] text-on-surface-variant">Workspace Performance</p>
      <h1 class="text-[2.25rem] font-semibold leading-tight tracking-tight text-on-surface">Stats</h1>
      <p class="mt-2 max-w-[64ch] text-sm text-on-surface-variant">
        Project allocation, app usage, activity distribution, heatmap intensity, and cost telemetry in one range-controlled
        board.
      </p>
    </div>

    <div class="flex flex-col gap-4 lg:flex-row lg:flex-wrap lg:items-end lg:justify-between">
      <div class="flex flex-wrap items-center gap-2">
        <button
          type="button"
          class="rounded-full border border-outline-variant bg-surface-container-low px-4 py-2 text-[0.68rem] font-medium uppercase tracking-[0.12em] text-on-surface-variant transition hover:border-primary/40 hover:bg-surface-container-high"
          on:click={() => setQuickRange(7)}
        >
          7D
        </button>
        <button
          type="button"
          class="rounded-full border border-outline-variant bg-surface-container-low px-4 py-2 text-[0.68rem] font-medium uppercase tracking-[0.12em] text-on-surface-variant transition hover:border-primary/40 hover:bg-surface-container-high"
          on:click={() => setQuickRange(30)}
        >
          30D
        </button>
        <button
          type="button"
          class="rounded-full border border-outline-variant bg-surface-container-low px-4 py-2 text-[0.68rem] font-medium uppercase tracking-[0.12em] text-on-surface-variant transition hover:border-primary/40 hover:bg-surface-container-high"
          on:click={() => setQuickRange(10000)}
        >
          All time
        </button>
      </div>

      <form class="flex flex-wrap items-end gap-3" on:submit|preventDefault={applyDateRange}>
        <label class="grid gap-1 text-[0.65rem] font-medium uppercase tracking-[0.12em] text-on-surface-variant">
          <span class="inline-flex items-center gap-1">
            <span class="material-symbols-outlined text-[16px] text-on-surface-variant/80">calendar_today</span>
            From
          </span>
          <input
            type="date"
            bind:value={fromDate}
            class="rounded-2xl border border-outline-variant bg-surface-container-lowest px-3 py-2 text-sm text-on-surface shadow-sm outline-none transition focus:border-primary focus:ring-2 focus:ring-primary/20"
          />
        </label>
        <label class="grid gap-1 text-[0.65rem] font-medium uppercase tracking-[0.12em] text-on-surface-variant">
          <span class="inline-flex items-center gap-1">
            <span class="material-symbols-outlined text-[16px] text-on-surface-variant/80">event</span>
            To
          </span>
          <input
            type="date"
            bind:value={toDate}
            class="rounded-2xl border border-outline-variant bg-surface-container-lowest px-3 py-2 text-sm text-on-surface shadow-sm outline-none transition focus:border-primary focus:ring-2 focus:ring-primary/20"
          />
        </label>
        <button
          type="submit"
          class="rounded-full bg-primary px-5 py-2.5 text-[0.7rem] font-semibold uppercase tracking-[0.1em] text-on-primary shadow-sm transition hover:bg-primary-container"
        >
          Apply
        </button>
      </form>
    </div>
  </header>

  {#if errorMessage}
    <p class="mb-6 rounded-2xl border border-amber-200 dark:border-amber-800 bg-amber-50 dark:bg-amber-950/50 px-4 py-3 text-sm text-amber-900 dark:text-amber-200" role="status">{errorMessage}</p>
  {/if}

  <section class="mb-8 grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
    <article class="flex flex-col gap-2 rounded-[24px] border border-outline/10 bg-surface-container-lowest p-5 shadow-sm">
      <div class="flex items-center gap-2 text-[0.65rem] font-semibold uppercase tracking-[0.12em] text-on-surface-variant">
        <span class="material-symbols-outlined text-[18px] text-primary">photo_library</span>
        Total captures
      </div>
      <p class="font-sans text-2xl font-semibold tracking-tight text-on-surface md:text-3xl">{stats.capture_count.toLocaleString()}</p>
      <p class="text-[0.7rem] uppercase tracking-[0.08em] text-on-surface-variant">{rangeCaptureCount.toLocaleString()} in selected range</p>
    </article>
    <article class="flex flex-col gap-2 rounded-[24px] border border-outline/10 bg-surface-container-lowest p-5 shadow-sm">
      <div class="flex items-center gap-2 text-[0.65rem] font-semibold uppercase tracking-[0.12em] text-on-surface-variant">
        <span class="material-symbols-outlined text-[18px] text-primary">today</span>
        Today
      </div>
      <p class="font-sans text-2xl font-semibold tracking-tight text-on-surface md:text-3xl">{stats.captures_today.toLocaleString()}</p>
      <p class="text-[0.7rem] uppercase tracking-[0.08em] text-on-surface-variant">uptime {formatUptime(stats.uptime_secs)}</p>
    </article>
    <article class="flex flex-col gap-2 rounded-[24px] border border-outline/10 bg-surface-container-lowest p-5 shadow-sm">
      <div class="flex items-center gap-2 text-[0.65rem] font-semibold uppercase tracking-[0.12em] text-on-surface-variant">
        <span class="material-symbols-outlined text-[18px] text-primary">hard_drive</span>
        Storage
      </div>
      <p class="font-sans text-2xl font-semibold tracking-tight text-on-surface md:text-3xl">{formatBytes(stats.storage_bytes)}</p>
      <p class="text-[0.7rem] uppercase tracking-[0.08em] text-on-surface-variant">local footprint</p>
    </article>
    <article class="flex flex-col gap-2 rounded-[24px] border border-outline/10 bg-surface-container-lowest p-5 shadow-sm">
      <div class="flex items-center gap-2 text-[0.65rem] font-semibold uppercase tracking-[0.12em] text-on-surface-variant">
        <span class="material-symbols-outlined text-[18px] text-primary">token</span>
        Tokens
      </div>
      <p class="font-sans text-2xl font-semibold tracking-tight text-on-surface md:text-3xl">{costView.totalTokens.toLocaleString()}</p>
      <p class="text-[0.7rem] uppercase tracking-[0.08em] text-on-surface-variant">{formatCost(costView.totalCents)} total spend</p>
    </article>
  </section>

  <section class="mb-8 grid grid-cols-1 gap-6 xl:grid-cols-2">
    <article class="flex flex-col gap-4 rounded-[24px] bg-surface-container-lowest p-6 shadow-sm ring-1 ring-outline/10">
      <header class="flex flex-wrap items-baseline justify-between gap-2">
        <div class="flex items-center gap-2">
          <span class="material-symbols-outlined text-[22px] text-on-surface-variant">bar_chart</span>
          <div>
            <h3 class="text-lg font-semibold text-on-surface">Time per project</h3>
            <p class="text-[0.65rem] font-semibold uppercase tracking-[0.14em] text-on-surface-variant">capture count proxy</p>
          </div>
        </div>
      </header>
      <div class="relative min-h-[15.4rem] overflow-hidden rounded-2xl bg-surface-container-low p-4">
        {#if loading}
          <p class="rounded-xl border border-dashed border-outline/30 bg-surface-container px-4 py-3 text-sm text-on-surface-variant">Loading project allocation…</p>
        {:else if projectSeries.length === 0}
          <p class="rounded-xl border border-dashed border-outline/30 bg-surface-container px-4 py-3 text-sm text-on-surface-variant">No project activity for this range.</p>
        {:else}
          <canvas class="max-h-[15rem] w-full" bind:this={projectCanvas}></canvas>
        {/if}
      </div>
    </article>

    <article class="flex flex-col gap-4 rounded-[24px] bg-surface-container-lowest p-6 shadow-sm ring-1 ring-outline/10">
      <header class="flex flex-wrap items-baseline justify-between gap-2">
        <div class="flex items-center gap-2">
          <span class="material-symbols-outlined text-[22px] text-on-surface-variant">donut_large</span>
          <div>
            <h3 class="text-lg font-semibold text-on-surface">Time per app</h3>
            <p class="text-[0.65rem] font-semibold uppercase tracking-[0.14em] text-on-surface-variant">capture distribution</p>
          </div>
        </div>
      </header>
      <div class="relative min-h-[15.4rem] overflow-hidden rounded-2xl bg-surface-container-low p-4">
        {#if loading}
          <p class="rounded-xl border border-dashed border-outline/30 bg-surface-container px-4 py-3 text-sm text-on-surface-variant">Loading app usage…</p>
        {:else if appSeries.length === 0}
          <p class="rounded-xl border border-dashed border-outline/30 bg-surface-container px-4 py-3 text-sm text-on-surface-variant">No app captures for this range.</p>
        {:else}
          <canvas class="max-h-[15rem] w-full" bind:this={appCanvas}></canvas>
        {/if}
      </div>
    </article>

    <article class="flex flex-col gap-4 rounded-[24px] bg-surface-container-lowest p-6 shadow-sm ring-1 ring-outline/10 xl:col-span-2">
      <header class="flex flex-wrap items-baseline justify-between gap-2">
        <div class="flex items-center gap-2">
          <span class="material-symbols-outlined text-[22px] text-on-surface-variant">analytics</span>
          <div>
            <h3 class="text-lg font-semibold text-on-surface">Activity breakdown</h3>
            <p class="text-[0.65rem] font-semibold uppercase tracking-[0.14em] text-on-surface-variant">daily activity signatures</p>
          </div>
        </div>
      </header>
      <div class="relative min-h-[16.6rem] overflow-hidden rounded-2xl bg-surface-container-low p-4">
        {#if loading}
          <p class="rounded-xl border border-dashed border-outline/30 bg-surface-container px-4 py-3 text-sm text-on-surface-variant">Loading activity categories…</p>
        {:else if activitySeries.length === 0}
          <p class="rounded-xl border border-dashed border-outline/30 bg-surface-container px-4 py-3 text-sm text-on-surface-variant">No activity labels available for this range.</p>
        {:else}
          <canvas class="max-h-[16rem] w-full" bind:this={activityCanvas}></canvas>
        {/if}
      </div>
    </article>
  </section>

  <section class="mb-8 rounded-[24px] bg-surface-container-lowest p-6 shadow-sm ring-1 ring-outline/10">
    <header class="mb-4 flex flex-wrap items-baseline justify-between gap-2">
      <div class="flex items-center gap-2">
        <span class="material-symbols-outlined text-[22px] text-on-surface-variant">grid_view</span>
        <div>
          <h3 class="text-lg font-semibold text-on-surface">Daily active hours heatmap</h3>
          <p class="text-[0.65rem] font-semibold uppercase tracking-[0.14em] text-on-surface-variant">cadence map</p>
        </div>
      </div>
    </header>

    {#if loading}
      <p class="rounded-xl border border-dashed border-outline/30 bg-surface-container px-4 py-3 text-sm text-on-surface-variant">Rendering active-hours field…</p>
    {:else if heatmapSeries.length === 0}
      <p class="rounded-xl border border-dashed border-outline/30 bg-surface-container px-4 py-3 text-sm text-on-surface-variant">No daily activity available for this range.</p>
    {:else}
      <div class="grid grid-cols-[auto_1fr] items-start gap-3">
        <div class="grid grid-rows-7 gap-1 pt-0.5 text-[0.65rem] font-medium uppercase tracking-wide text-on-surface-variant" aria-hidden="true">
          <span>S</span>
          <span>M</span>
          <span>T</span>
          <span>W</span>
          <span>T</span>
          <span>F</span>
          <span>S</span>
        </div>
        <div
          class="grid grid-flow-col grid-rows-7 gap-1 overflow-x-auto pb-1 [grid-auto-columns:0.75rem]"
          role="img"
          aria-label="Daily active hours heatmap"
        >
          {#each heatmapSlots as slot, index (`${slot?.date ?? 'pad'}-${index}`)}
            {#if slot}
              <div
                class="h-3 w-3 shrink-0 rounded-sm border border-outline-variant/40 {slot.level === 0
                  ? 'bg-surface-container-high'
                  : slot.level === 1
                    ? 'bg-emerald-200 dark:bg-emerald-900'
                    : slot.level === 2
                      ? 'bg-emerald-400 dark:bg-emerald-600'
                      : slot.level === 3
                        ? 'bg-amber-400 dark:bg-amber-600'
                        : 'bg-rose-400 dark:bg-rose-600'}"
                title={slot.tooltip}
              ></div>
            {:else}
              <div class="h-3 w-3 shrink-0 rounded-sm border border-outline-variant/20 bg-surface-container-low/50" aria-hidden="true"></div>
            {/if}
          {/each}
        </div>
      </div>
    {/if}
  </section>

  <section class="grid grid-cols-1 gap-6 lg:grid-cols-2">
    <article class="flex flex-col gap-4 rounded-[24px] bg-surface-container-lowest p-6 shadow-sm ring-1 ring-outline/10">
      <header class="flex flex-wrap items-baseline justify-between gap-2">
        <div class="flex items-center gap-2">
          <span class="material-symbols-outlined text-[22px] text-on-surface-variant">payments</span>
          <div>
            <h3 class="text-lg font-semibold text-on-surface">Cost tracking</h3>
            <p class="text-[0.65rem] font-semibold uppercase tracking-[0.14em] text-on-surface-variant">reported model cost</p>
          </div>
        </div>
      </header>

      {#if loading}
        <p class="rounded-xl border border-dashed border-outline/30 bg-surface-container px-4 py-3 text-sm text-on-surface-variant">Loading cost telemetry…</p>
      {:else}
        <dl class="grid grid-cols-2 gap-3">
          <div class="rounded-2xl border border-outline/10 bg-surface-container-low p-3">
            <dt class="text-[0.65rem] font-semibold uppercase tracking-[0.1em] text-on-surface-variant">Total cost</dt>
            <dd class="mt-1 font-sans text-lg font-semibold text-on-surface">{formatCost(costView.totalCents)}</dd>
          </div>
          <div class="rounded-2xl border border-outline/10 bg-surface-container-low p-3">
            <dt class="text-[0.65rem] font-semibold uppercase tracking-[0.1em] text-on-surface-variant">Avg / day</dt>
            <dd class="mt-1 font-sans text-lg font-semibold text-on-surface">{formatCost(costView.perDayCents)}</dd>
          </div>
          <div class="rounded-2xl border border-outline/10 bg-surface-container-low p-3">
            <dt class="text-[0.65rem] font-semibold uppercase tracking-[0.1em] text-on-surface-variant">Current month</dt>
            <dd class="mt-1 font-sans text-lg font-semibold text-on-surface">{formatCost(costView.monthCents)}</dd>
          </div>
          <div class="rounded-2xl border border-outline/10 bg-surface-container-low p-3">
            <dt class="text-[0.65rem] font-semibold uppercase tracking-[0.1em] text-on-surface-variant">Tokens used</dt>
            <dd class="mt-1 font-sans text-lg font-semibold text-on-surface">{costView.totalTokens.toLocaleString()}</dd>
          </div>
        </dl>

        <div class="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <article class="rounded-2xl border border-outline/10 bg-surface-container-low p-4">
            <p class="text-[0.65rem] font-semibold uppercase tracking-[0.1em] text-on-surface-variant">Extraction</p>
            <p class="mt-1 font-sans text-lg font-semibold text-on-surface">{formatCost(costView.extractionCents)}</p>
            <p class="mt-1 text-[0.65rem] uppercase tracking-[0.08em] text-on-surface-variant">{costView.extractionTokens.toLocaleString()} tokens</p>
          </article>
          <article class="rounded-2xl border border-outline/10 bg-surface-container-low p-4">
            <p class="text-[0.65rem] font-semibold uppercase tracking-[0.1em] text-on-surface-variant">Synthesis</p>
            <p class="mt-1 font-sans text-lg font-semibold text-on-surface">{formatCost(costView.synthesisCents)}</p>
            <p class="mt-1 text-[0.65rem] uppercase tracking-[0.08em] text-on-surface-variant">{costView.synthesisTokens.toLocaleString()} tokens</p>
          </article>
        </div>
      {/if}
    </article>

    <article class="flex flex-col gap-4 rounded-[24px] bg-surface-container-lowest p-6 shadow-sm ring-1 ring-outline/10">
      <header class="flex flex-wrap items-baseline justify-between gap-2">
        <div class="flex items-center gap-2">
          <span class="material-symbols-outlined text-[22px] text-on-surface-variant">calendar_month</span>
          <div>
            <h3 class="text-lg font-semibold text-on-surface">Cost by day</h3>
            <p class="text-[0.65rem] font-semibold uppercase tracking-[0.14em] text-on-surface-variant">rolling daily spend</p>
          </div>
        </div>
      </header>

      {#if loading}
        <p class="rounded-xl border border-dashed border-outline/30 bg-surface-container px-4 py-3 text-sm text-on-surface-variant">Loading daily cost points…</p>
      {:else if costView.byDay.length === 0}
        <p class="rounded-xl border border-dashed border-outline/30 bg-surface-container px-4 py-3 text-sm text-on-surface-variant">No reported cost points in this range.</p>
      {:else}
        <ul class="divide-y divide-outline/15 rounded-2xl border border-outline/10 bg-surface-container-low overflow-hidden">
          {#each costView.byDay.slice(-8).reverse() as point (point.date)}
            <li class="flex flex-wrap items-baseline justify-between gap-3 px-4 py-3">
              <span class="text-[0.7rem] font-semibold uppercase tracking-[0.1em] text-on-surface-variant">{formatDateLabel(point.date)}</span>
              <span class="font-sans text-base font-semibold text-on-surface">{formatCost(point.cents)}</span>
              <span class="w-full text-right text-[0.65rem] uppercase tracking-[0.08em] text-on-surface-variant sm:w-auto sm:text-left">{point.tokens.toLocaleString()} tokens</span>
            </li>
          {/each}
        </ul>
      {/if}
    </article>
  </section>
</section>
