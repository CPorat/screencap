<script lang="ts">
  import { onMount } from 'svelte';

  import {
    getCaptureDetail,
    getCaptures,
    type CaptureRecord,
    type ExtractionRecord,
  } from '$lib/api';
  import CaptureCard from '$lib/components/CaptureCard.svelte';
  import CaptureDetailsModal from '$lib/components/CaptureDetailsModal.svelte';

  type TimelineItem = {
    capture: CaptureRecord;
    extraction: ExtractionRecord | null;
  };

  type HourBucket = {
    key: string;
    heading: string;
    captures: TimelineItem[];
  };

  const HOUR_HEADING_FORMATTER = new Intl.DateTimeFormat(undefined, {
    hour: 'numeric',
    minute: '2-digit',
  });

  const LIMIT = 10;

  let selectedDate = formatLocalDate(new Date());
  let appFilter = '';
  let activityFilter = '';

  let loading = true;
  let loadingMore = false;
  let hasMore = true;
  let errorMessage: string | null = null;
  let timelineItems: TimelineItem[] = [];
  let selectedItem: TimelineItem | null = null;
  let offset = 0;

  let requestVersion = 0;
  function formatLocalDate(value: Date): string {
    const year = value.getFullYear();
    const month = String(value.getMonth() + 1).padStart(2, '0');
    const day = String(value.getDate()).padStart(2, '0');
    return `${year}-${month}-${day}`;
  }

  function buildDayBounds(date: string): { from: string; to: string } {
    const [yearRaw, monthRaw, dayRaw] = date.split('-').map((part) => Number(part));
    const year = Number.isFinite(yearRaw) ? yearRaw : 1970;
    const month = Number.isFinite(monthRaw) ? monthRaw - 1 : 0;
    const day = Number.isFinite(dayRaw) ? dayRaw : 1;

    const fromLocal = new Date(year, month, day, 0, 0, 0, 0);
    const toLocal = new Date(year, month, day + 1, 0, 0, 0, 0);

    return {
      from: fromLocal.toISOString(),
      to: toLocal.toISOString(),
    };
  }

  async function hydrateCapture(capture: CaptureRecord): Promise<TimelineItem> {
    if (capture.extraction_status !== 'processed') {
      return { capture, extraction: null };
    }

    const detail = await getCaptureDetail(capture.id);
    return {
      capture,
      extraction: detail?.extraction ?? null,
    };
  }

  function sortByNewestFirst(left: TimelineItem, right: TimelineItem): number {
    const leftTime = new Date(left.capture.timestamp).getTime();
    const rightTime = new Date(right.capture.timestamp).getTime();
    return rightTime - leftTime;
  }

  async function fetchTimelinePage(pageOffset: number): Promise<{ items: TimelineItem[]; count: number }> {
    const { from, to } = buildDayBounds(selectedDate);
    const captures = await getCaptures(LIMIT, pageOffset, { from, to });
    const hydrated = await Promise.all(captures.map((capture) => hydrateCapture(capture)));

    return {
      items: hydrated,
      count: captures.length,
    };
  }

  async function loadTimelineForDate(): Promise<void> {
    const currentRequest = ++requestVersion;

    loading = true;
    loadingMore = false;
    hasMore = true;
    errorMessage = null;
    selectedItem = null;
    offset = 0;

    try {
      const page = await fetchTimelinePage(offset);

      if (currentRequest !== requestVersion) {
        return;
      }

      timelineItems = page.items.sort(sortByNewestFirst);
      offset += LIMIT;
      hasMore = page.count === LIMIT;
    } catch (error) {
      if (currentRequest !== requestVersion) {
        return;
      }

      timelineItems = [];
      hasMore = false;
      errorMessage = error instanceof Error ? error.message : 'Failed to load captures.';
    } finally {
      if (currentRequest === requestVersion) {
        loading = false;
      }
    }
  }

  async function loadMore(): Promise<void> {
    if (loading || loadingMore || !hasMore) {
      return;
    }

    const currentRequest = requestVersion;
    loadingMore = true;
    errorMessage = null;

    try {
      const page = await fetchTimelinePage(offset);

      if (currentRequest !== requestVersion) {
        return;
      }

      timelineItems = [...timelineItems, ...page.items].sort(sortByNewestFirst);
      offset += LIMIT;
      hasMore = page.count === LIMIT;
    } catch (error) {
      if (currentRequest !== requestVersion) {
        return;
      }

      errorMessage = error instanceof Error ? error.message : 'Failed to load more captures.';
    } finally {
      if (currentRequest === requestVersion) {
        loadingMore = false;
      }
    }
  }

  function buildHourBuckets(items: TimelineItem[]): HourBucket[] {
    const grouped = new Map<string, HourBucket>();

    for (const item of items) {
      const capturedAt = new Date(item.capture.timestamp);
      if (!Number.isFinite(capturedAt.getTime())) {
        continue;
      }

      const hourStart = new Date(capturedAt);
      hourStart.setMinutes(0, 0, 0);

      const key = hourStart.toISOString();
      const existing = grouped.get(key);

      if (existing) {
        existing.captures.push(item);
        continue;
      }

      grouped.set(key, {
        key,
        heading: HOUR_HEADING_FORMATTER.format(hourStart),
        captures: [item],
      });
    }

    return Array.from(grouped.values()).sort((left, right) => {
      const leftTime = new Date(left.key).getTime();
      const rightTime = new Date(right.key).getTime();
      return rightTime - leftTime;
    });
  }

  function normalizeFilterValue(value: string): string {
    return value.trim().toLowerCase();
  }

  function activityText(item: TimelineItem): string {
    return [
      item.extraction?.activity_type,
      item.extraction?.description,
      item.capture.primary_activity,
      item.capture.narrative,
      item.capture.batch_narrative,
    ]
      .filter((value): value is string => typeof value === 'string' && value.trim().length > 0)
      .join(' ')
      .toLowerCase();
  }

  onMount(() => {
    void loadTimelineForDate();
  });

  $: normalizedAppFilter = normalizeFilterValue(appFilter);
  $: normalizedActivityFilter = normalizeFilterValue(activityFilter);

  $: filteredItems = timelineItems.filter((item) => {
    const appName = item.capture.app_name?.toLowerCase() ?? '';
    const appMatches = !normalizedAppFilter || appName.includes(normalizedAppFilter);
    const activityMatches =
      !normalizedActivityFilter || activityText(item).includes(normalizedActivityFilter);

    return appMatches && activityMatches;
  });

  $: hourBuckets = buildHourBuckets(filteredItems);
</script>

<section class="timeline" aria-busy={loading}>
  <header class="timeline__header">
    <p class="timeline__eyebrow">Timeline</p>
    <h2>Capture Chronicle</h2>
    <p class="timeline__summary">Select a day, filter captures, and inspect each frame's extraction payload.</p>
  </header>

  <div class="timeline__controls" role="group" aria-label="Timeline filters">
    <label>
      <span>Date</span>
      <input
        type="date"
        bind:value={selectedDate}
        on:change={() => {
          void loadTimelineForDate();
        }}
      />
    </label>

    <label>
      <span>App name</span>
      <input type="text" bind:value={appFilter} placeholder="Filter by app" autocomplete="off" />
    </label>

    <label>
      <span>Activity type</span>
      <input
        type="text"
        bind:value={activityFilter}
        placeholder="coding, reading, meeting..."
        autocomplete="off"
      />
    </label>
  </div>

  {#if loading}
    <div class="timeline__grid" aria-hidden="true">
      {#each Array.from({ length: 6 }, (_, index) => index) as skeleton (skeleton)}
        <article class="timeline__skeleton">
          <div class="timeline__skeleton-image"></div>
          <div class="timeline__skeleton-line timeline__skeleton-line--short"></div>
          <div class="timeline__skeleton-line"></div>
        </article>
      {/each}
    </div>
  {:else if errorMessage}
    <div class="timeline__state" role="alert">
      <h3>Timeline unavailable</h3>
      <p>{errorMessage}</p>
      <button type="button" on:click={() => void loadTimelineForDate()}>Retry</button>
    </div>
  {:else if hourBuckets.length === 0}
    <div class="timeline__state" role="status">
      <h3>No captures for this day</h3>
      <p>Try a different date or clear filters to broaden results.</p>
    </div>
  {:else}
    <div class="timeline__groups">
      {#each hourBuckets as bucket (bucket.key)}
        <section class="timeline__hour" aria-label={`Captures for ${bucket.heading}`}>
          <header class="timeline__hour-header">
            <h3>{bucket.heading}</h3>
            <p>{bucket.captures.length} capture{bucket.captures.length === 1 ? '' : 's'}</p>
          </header>

          <div class="timeline__grid">
            {#each bucket.captures as item (item.capture.id)}
              <CaptureCard
                capture={item.capture}
                extraction={item.extraction}
                on:open={() => {
                  selectedItem = item;
                }}
              />
            {/each}
          </div>
        </section>
      {/each}
    </div>
    {#if hasMore}
      <div class="timeline__load-more">
        <button type="button" on:click={() => void loadMore()} disabled={loadingMore}>
          {loadingMore ? 'Loading...' : 'Load More'}
        </button>
      </div>
    {/if}

  {/if}

  <CaptureDetailsModal
    open={selectedItem !== null}
    capture={selectedItem?.capture ?? null}
    extraction={selectedItem?.extraction ?? null}
    on:close={() => {
      selectedItem = null;
    }}
  />
</section>

<style>
  .timeline {
    height: 100%;
    overflow: auto;
    padding: clamp(1.1rem, 2.5vw, 1.9rem);
    display: grid;
    align-content: start;
    gap: 1rem;
    background:
      linear-gradient(160deg, rgb(25 31 47 / 95%), rgb(12 15 26 / 98%)),
      radial-gradient(circle at 84% 8%, rgb(112 255 227 / 18%), transparent 34%);
  }

  .timeline__header {
    display: grid;
    gap: 0.5rem;
  }

  .timeline__eyebrow {
    font-size: 0.68rem;
    text-transform: uppercase;
    letter-spacing: 0.22em;
    color: var(--pulse);
  }

  h2 {
    font-size: clamp(1.85rem, 4vw, 3rem);
  }

  .timeline__summary {
    color: var(--paper-200);
    font-size: 0.88rem;
    max-width: 68ch;
  }

  .timeline__controls {
    border: 1px solid rgb(246 241 231 / 32%);
    border-radius: 0.95rem;
    background: rgb(9 13 21 / 78%);
    padding: 0.78rem;
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 0.7rem;
  }

  .timeline__controls label {
    display: grid;
    gap: 0.32rem;
  }

  .timeline__controls span {
    font-size: 0.66rem;
    text-transform: uppercase;
    letter-spacing: 0.14em;
    color: var(--paper-200);
  }

  .timeline__controls input {
    width: 100%;
    border: 1px solid rgb(246 241 231 / 35%);
    border-radius: 0.65rem;
    background: rgb(16 20 30 / 92%);
    color: var(--paper-100);
    font: inherit;
    font-size: 0.82rem;
    padding: 0.42rem 0.52rem;
  }

  .timeline__controls input:focus-visible {
    outline: 2px solid var(--pulse);
    outline-offset: 1px;
    border-color: var(--pulse);
  }

  .timeline__groups {
    display: grid;
    gap: 0.95rem;
  }

  .timeline__hour {
    display: grid;
    gap: 0.6rem;
  }

  .timeline__hour-header {
    position: sticky;
    top: 0;
    z-index: 1;
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 0.7rem;
    border: 1px solid rgb(246 241 231 / 26%);
    border-radius: 0.8rem;
    background: linear-gradient(92deg, rgb(12 19 31 / 96%), rgb(12 16 24 / 96%));
    padding: 0.42rem 0.58rem;
    backdrop-filter: blur(8px);
  }

  h3 {
    font-size: 1rem;
  }

  .timeline__hour-header p {
    font-size: 0.68rem;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--paper-200);
  }

  .timeline__grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
    gap: 0.75rem;
  }

  .timeline__state {
    border: 1px dashed rgb(246 241 231 / 32%);
    border-radius: 0.9rem;
    background: rgb(9 12 20 / 72%);
    padding: 0.95rem;
    display: grid;
    gap: 0.44rem;
  }

  .timeline__state h3 {
    font-size: clamp(1.1rem, 2.3vw, 1.4rem);
  }

  .timeline__state p {
    color: var(--paper-200);
    font-size: 0.84rem;
  }

  .timeline__state button {
    justify-self: start;
    border: 1px solid rgb(246 241 231 / 40%);
    border-radius: 999px;
    background: rgb(15 20 32 / 88%);
    color: var(--paper-100);
    font: inherit;
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    padding: 0.32rem 0.7rem;
    cursor: pointer;
  }

  .timeline__state button:hover,
  .timeline__state button:focus-visible {
    border-color: var(--pulse);
    color: var(--pulse);
    outline: none;
  }

  .timeline__skeleton {
    border: 1px solid rgb(246 241 231 / 28%);
    border-radius: 0.9rem;
    padding: 0.72rem;
    display: grid;
    gap: 0.54rem;
    background: rgb(13 16 24 / 88%);
  }

  .timeline__skeleton-image,
  .timeline__skeleton-line {
    border-radius: 0.62rem;
    background: linear-gradient(90deg, rgb(49 55 72 / 72%), rgb(76 85 110 / 78%), rgb(49 55 72 / 72%));
    background-size: 200% 100%;
    animation: pulse 1.4s ease infinite;
  }

  .timeline__skeleton-image {
    width: 100%;
    aspect-ratio: 16 / 10;
  }

  .timeline__skeleton-line {
    height: 0.72rem;
  }

  .timeline__skeleton-line--short {
    width: 62%;
  }

  @keyframes pulse {
    0%,
    100% {
      background-position: 0% 0;
    }

    50% {
      background-position: 100% 0;
    }
  }

  .timeline__load-more {
    display: flex;
    justify-content: center;
  }

  .timeline__load-more button {
    border: 1px solid rgb(246 241 231 / 40%);
    border-radius: 999px;
    background: rgb(15 20 32 / 88%);
    color: var(--paper-100);
    font: inherit;
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    padding: 0.36rem 0.78rem;
    cursor: pointer;
  }

  .timeline__load-more button:hover,
  .timeline__load-more button:focus-visible {
    border-color: var(--pulse);
    color: var(--pulse);
    outline: none;
  }

  .timeline__load-more button:disabled {
    opacity: 0.72;
    cursor: wait;
  }

  .timeline__load-more button:disabled:hover,
  .timeline__load-more button:disabled:focus-visible {
    border-color: rgb(246 241 231 / 40%);
    color: var(--paper-100);
  }

  @media (max-width: 900px) {
    .timeline__controls {
      grid-template-columns: 1fr;
    }
  }
</style>
