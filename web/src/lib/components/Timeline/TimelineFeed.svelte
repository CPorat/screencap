<script lang="ts">
  import { onMount } from 'svelte';

  import {
    getApps,
    getCaptureDetail,
    getProjectTimeAllocations,
    listCaptures,
    type CaptureRecord,
    type CaptureListOptions,
    type CaptureListResponse,
    type ExtractionRecord,
  } from '$lib/api';

  import TimelineHourGroup from './TimelineHourGroup.svelte';
  import TimelineSkeleton from './TimelineSkeleton.svelte';
  import type { TimelineCapture, TimelineHourBucket } from './types';

  const PAGE_SIZE = 24;
  const LOAD_AHEAD_MARGIN = '420px';
  const hourHeadingFormatter = new Intl.DateTimeFormat(undefined, {
    weekday: 'short',
    month: 'short',
    day: 'numeric',
    hour: 'numeric',
  });

  const shortDateFormatter = new Intl.DateTimeFormat(undefined, {
    month: 'short',
    day: 'numeric',
  });
  const activityFilterOptions = [
    { value: 'coding', label: 'Coding' },
    { value: 'browsing', label: 'Browsing' },
    { value: 'communication', label: 'Communication' },
    { value: 'reading', label: 'Reading' },
    { value: 'writing', label: 'Writing' },
    { value: 'design', label: 'Design' },
    { value: 'terminal', label: 'Terminal' },
    { value: 'meeting', label: 'Meeting' },
    { value: 'media', label: 'Media' },
    { value: 'other', label: 'Other' },
  ] as const;

  const hourClockFormatter = new Intl.DateTimeFormat(undefined, {
    hour: 'numeric',
    minute: '2-digit',
  });

  let timeline: TimelineCapture[] = [];
  let loadingInitial = true;
  let loadingMore = false;
  let hasMore = true;
  let loadError: string | null = null;
  let nextOffset = 0;
  let requestVersion = 0;
  let mounted = false;

  let selectedDate = '';
  let selectedApp = '';
  let selectedProject = '';
  let selectedActivity = '';

  let appOptions: string[] = [];
  let projectOptions: string[] = [];
  let filterFingerprint = '';

  function buildFilterFingerprint(): string {
    return [selectedDate, selectedApp, selectedProject, selectedActivity].join('::');
  }

  function isSameLocalDay(a: Date, b: Date): boolean {
    return (
      a.getFullYear() === b.getFullYear() &&
      a.getMonth() === b.getMonth() &&
      a.getDate() === b.getDate()
    );
  }

  function hourStart(date: Date): Date {
    const normalized = new Date(date);
    normalized.setMinutes(0, 0, 0);
    return normalized;
  }

  function hourRangeLabel(start: Date): string {
    const end = new Date(start);
    end.setMinutes(59, 59, 999);
    return `${hourClockFormatter.format(start)} — ${hourClockFormatter.format(end)}`;
  }

  function hourHeading(start: Date): string {
    const now = new Date();
    if (isSameLocalDay(start, now)) {
      return `Today · ${hourClockFormatter.format(start)}`;
    }

    const yesterday = new Date(now);
    yesterday.setDate(yesterday.getDate() - 1);
    if (isSameLocalDay(start, yesterday)) {
      return `Yesterday · ${hourClockFormatter.format(start)}`;
    }

    return `${hourHeadingFormatter.format(start)} · ${shortDateFormatter.format(start)}`;
  }

  async function hydrateCaptures(captures: CaptureRecord[]): Promise<TimelineCapture[]> {
    const extractionByCapture = new Map<number, ExtractionRecord>();

    const capturesWithExtraction = captures.filter(
      (capture) => capture.extraction_status === 'processed' && typeof capture.extraction_id === 'number'
    );

    const details = await Promise.all(
      capturesWithExtraction.map(async (capture) => {
        const detail = await getCaptureDetail(capture.id);
        return [capture.id, detail?.extraction ?? null] as const;
      })
    );

    for (const [captureId, extraction] of details) {
      if (extraction) {
        extractionByCapture.set(captureId, extraction);
      }
    }

    return captures.map((capture) => ({
      capture,
      extraction: extractionByCapture.get(capture.id) ?? null,
    }));
  }

  function parseInputDate(value: string): Date | null {
    if (!/^\d{4}-\d{2}-\d{2}$/.test(value)) {
      return null;
    }

    const [yearRaw, monthRaw, dayRaw] = value.split('-').map((part) => Number(part));
    const parsed = new Date(yearRaw, monthRaw - 1, dayRaw, 0, 0, 0, 0);

    if (!Number.isFinite(parsed.getTime())) {
      return null;
    }

    return parsed;
  }

  function dayBoundsIso(value: string): { from: string; to: string } | null {
    const parsed = parseInputDate(value);
    if (!parsed) {
      return null;
    }

    const from = new Date(parsed);
    from.setHours(0, 0, 0, 0);

    const to = new Date(parsed);
    to.setHours(23, 59, 59, 999);

    return {
      from: from.toISOString(),
      to: to.toISOString(),
    };
  }

  function listCaptureOptions(): CaptureListOptions {
    const options: CaptureListOptions = {};

    if (selectedDate) {
      const bounds = dayBoundsIso(selectedDate);
      if (bounds) {
        options.from = bounds.from;
        options.to = bounds.to;
      }
    }

    const app = selectedApp.trim();
    if (app) {
      options.app = app;
    }

    const project = selectedProject.trim();
    if (project) {
      options.project = project;
    }

    const activity = selectedActivity.trim();
    if (activity) {
      options.activityType = activity;
    }

    return options;
  }

  async function loadNextPage(expectedVersion = requestVersion): Promise<void> {
    if (loadingMore || !hasMore) {
      return;
    }

    loadingMore = true;
    loadError = null;

    try {
      const page: CaptureListResponse = await listCaptures(PAGE_SIZE, nextOffset, listCaptureOptions());
      const hydrated = await hydrateCaptures(page.captures);

      if (expectedVersion !== requestVersion) {
        return;
      }

      timeline = [...timeline, ...hydrated];
      nextOffset += page.captures.length;
      hasMore = page.captures.length === PAGE_SIZE;
    } catch (error) {
      if (expectedVersion !== requestVersion) {
        return;
      }

      hasMore = false;
      loadError = error instanceof Error ? error.message : 'Failed to load timeline data.';
    } finally {
      if (expectedVersion === requestVersion) {
        loadingMore = false;
        loadingInitial = false;
      }
    }
  }

  async function reloadTimeline(): Promise<void> {
    requestVersion += 1;
    timeline = [];
    loadingInitial = true;
    loadingMore = false;
    hasMore = true;
    loadError = null;
    nextOffset = 0;
    await loadNextPage(requestVersion);
  }

  function infiniteSentinel(node: HTMLElement) {
    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            void loadNextPage();
            break;
          }
        }
      },
      {
        root: null,
        rootMargin: LOAD_AHEAD_MARGIN,
      }
    );

    observer.observe(node);

    return {
      destroy() {
        observer.disconnect();
      },
    };
  }

  onMount(async () => {
    filterFingerprint = buildFilterFingerprint();
    mounted = true;
    try {
      const [apps, projects] = await Promise.all([getApps(), getProjectTimeAllocations()]);
      appOptions = apps
        .map((entry) => entry.app_name.trim())
        .filter((value) => value.length > 0);
      projectOptions = projects
        .map((entry) => entry.project?.trim() ?? '')
        .filter((value) => value.length > 0);
    } catch (error) {
      console.warn('Failed to load timeline filter options', error);
    }

    await reloadTimeline();
  });

  $: {
    const nextFingerprint = buildFilterFingerprint();
    if (mounted && nextFingerprint !== filterFingerprint) {
      filterFingerprint = nextFingerprint;
      void reloadTimeline();
    }
  }

  function groupByHour(items: TimelineCapture[]): TimelineHourBucket[] {
    const grouped = new Map<string, TimelineHourBucket>();

    const sorted = [...items].sort((left, right) => {
      const leftTime = new Date(left.capture.timestamp).getTime();
      const rightTime = new Date(right.capture.timestamp).getTime();
      return rightTime - leftTime;
    });

    for (const item of sorted) {
      const capturedAt = new Date(item.capture.timestamp);
      if (!Number.isFinite(capturedAt.getTime())) {
        continue;
      }

      const start = hourStart(capturedAt);
      const key = start.toISOString();

      const bucket = grouped.get(key);
      if (bucket) {
        bucket.captures.push(item);
        continue;
      }

      grouped.set(key, {
        key,
        heading: hourHeading(start),
        rangeLabel: hourRangeLabel(start),
        captures: [item],
      });
    }

    return Array.from(grouped.values());
  }

  $: hourBuckets = groupByHour(timeline);
</script>

<section class="panel" aria-busy={loadingInitial || loadingMore}>
  <header class="panel__header">
    <p class="panel__section">Timeline</p>
    <h2>Chronology stream</h2>
    <p class="panel__summary">
      Captures are grouped by hour with extraction overlays for activity, projects, and summaries.
    </p>
    <div class="panel__filters" aria-label="Timeline filters">
      <label>
        <span>Date jump</span>
        <input type="date" bind:value={selectedDate} />
      </label>
      <label>
        <span>App</span>
        <select bind:value={selectedApp}>
          <option value="">All apps</option>
          {#each appOptions as appName}
            <option value={appName}>{appName}</option>
          {/each}
        </select>
      </label>
      <label>
        <span>Project</span>
        <select bind:value={selectedProject}>
          <option value="">All projects</option>
          {#each projectOptions as projectName}
            <option value={projectName}>{projectName}</option>
          {/each}
        </select>
      </label>
      <label>
        <span>Activity</span>
        <select bind:value={selectedActivity}>
          <option value="">All activity</option>
          {#each activityFilterOptions as option}
            <option value={option.value}>{option.label}</option>
          {/each}
        </select>
      </label>
    </div>
  </header>

  {#if loadingInitial}
    <TimelineSkeleton />
  {:else if hourBuckets.length === 0}
    <div class="panel__empty" role="status">
      <h3>No captures yet</h3>
      <p>Keep the daemon running and new captures will appear here automatically.</p>
    </div>
  {:else}
    <div class="panel__hours">
      {#each hourBuckets as bucket (bucket.key)}
        <TimelineHourGroup {bucket} />
      {/each}
    </div>

    {#if loadingMore}
      <TimelineSkeleton compact={true} />
    {/if}

    {#if !hasMore}
      <p class="panel__ending">Reached the earliest loaded captures.</p>
    {/if}
  {/if}

  {#if loadError}
    <div class="panel__error" role="alert">
      <p>{loadError}</p>
      <button type="button" on:click={() => {
        hasMore = true;
        void loadNextPage();
      }}>Retry</button>
    </div>
  {/if}

  <div class="panel__sentinel" use:infiniteSentinel aria-hidden="true"></div>
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
      linear-gradient(150deg, rgb(33 37 51 / 94%), rgb(18 21 32 / 98%)),
      radial-gradient(circle at 82% 12%, rgb(255 78 166 / 12%), transparent 38%);
    scroll-behavior: smooth;
  }

  .panel__header {
    display: grid;
    gap: 0.52rem;
  }

  .panel__filters {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(9rem, 1fr));
    gap: 0.6rem;
    margin-top: 0.45rem;
  }

  .panel__filters label {
    display: grid;
    gap: 0.28rem;
  }

  .panel__filters span {
    font-size: 0.67rem;
    text-transform: uppercase;
    letter-spacing: 0.11em;
    color: rgb(246 241 231 / 72%);
  }

  .panel__filters input,
  .panel__filters select {
    border: 1px solid rgb(246 241 231 / 24%);
    border-radius: 0.62rem;
    background: rgb(10 13 20 / 64%);
    color: var(--paper-100);
    font: inherit;
    font-size: 0.8rem;
    padding: 0.42rem 0.52rem;
    min-height: 2rem;
  }

  .panel__section {
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.22em;
    color: var(--pulse);
  }

  h2 {
    font-size: clamp(1.95rem, 4.2vw, 3.25rem);
  }

  .panel__summary,
  .panel__empty p,
  .panel__ending {
    color: var(--paper-200);
    font-size: 0.88rem;
  }

  .panel__hours {
    display: grid;
    gap: 1rem;
  }

  .panel__sentinel {
    height: 1px;
    width: 100%;
  }

  .panel__empty {
    border: 1px dashed rgb(246 241 231 / 32%);
    border-radius: 0.9rem;
    padding: 1rem;
    background: rgb(10 13 20 / 62%);
    display: grid;
    gap: 0.35rem;
  }

  .panel__empty h3 {
    font-size: clamp(1.2rem, 2.4vw, 1.6rem);
  }

  .panel__ending {
    justify-self: center;
    text-transform: uppercase;
    letter-spacing: 0.11em;
    font-size: 0.7rem;
  }

  .panel__error {
    border: 1px solid rgb(255 78 166 / 54%);
    border-radius: 0.9rem;
    padding: 0.75rem;
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: center;
    gap: 0.75rem;
    background: rgb(41 16 31 / 72%);
  }

  .panel__error p {
    color: rgb(255 214 230 / 92%);
    font-size: 0.8rem;
  }

  .panel__error button {
    border: 1px solid rgb(255 78 166 / 65%);
    background: rgb(30 8 21 / 88%);
    color: rgb(255 194 223 / 96%);
    border-radius: 999px;
    padding: 0.32rem 0.7rem;
    font: inherit;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 0.68rem;
    cursor: pointer;
  }
</style>
