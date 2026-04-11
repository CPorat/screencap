<script lang="ts">
  import { onMount } from 'svelte';

  import {
    getCaptureDetail,
    listCaptures,
    type CaptureRecord,
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

  async function loadNextPage(): Promise<void> {
    if (loadingMore || !hasMore) {
      return;
    }

    loadingMore = true;
    loadError = null;

    try {
      const page: CaptureListResponse = await listCaptures(PAGE_SIZE, nextOffset);
      const hydrated = await hydrateCaptures(page.captures);

      timeline = [...timeline, ...hydrated];
      nextOffset += page.captures.length;
      hasMore = page.captures.length === PAGE_SIZE;
    } catch (error) {
      hasMore = false;
      loadError = error instanceof Error ? error.message : 'Failed to load timeline data.';
    } finally {
      loadingMore = false;
      loadingInitial = false;
    }
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

  onMount(() => {
    void loadNextPage();
  });

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
