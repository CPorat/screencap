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

  const DATE_DISPLAY_FORMATTER = new Intl.DateTimeFormat(undefined, {
    weekday: 'long',
    month: 'long',
    day: 'numeric',
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
  $: displayDate = (() => {
    const [y, m, d] = selectedDate.split('-').map(Number);
    return DATE_DISPLAY_FORMATTER.format(new Date(y, m - 1, d));
  })();
</script>

<div class="space-y-8" aria-busy={loading}>
  <!-- Page Header -->
  <div class="flex items-center justify-between">
    <div>
      <h1 class="text-[2.25rem] font-semibold tracking-tight text-on-surface">Timeline</h1>
      <p class="text-secondary text-sm">{displayDate}</p>
    </div>
    <div class="flex items-center gap-3">
      <div class="flex items-center gap-2 px-4 py-2 bg-surface-container-high rounded-lg text-on-surface cursor-pointer hover:bg-surface-container-highest transition-colors text-sm">
        <span class="material-symbols-outlined text-sm">calendar_today</span>
        <input
          type="date"
          class="bg-transparent border-none p-0 text-sm font-medium focus:ring-0 cursor-pointer"
          bind:value={selectedDate}
          on:change={() => { void loadTimelineForDate(); }}
        />
      </div>
    </div>
  </div>

  <!-- Filter Controls -->
  <div class="flex items-center gap-3">
    <div class="relative flex-1 max-w-xs">
      <span class="material-symbols-outlined absolute left-3 top-1/2 -translate-y-1/2 text-on-surface-variant text-[18px]">apps</span>
      <input
        type="text"
        class="w-full bg-surface-container-lowest border-none rounded-xl py-2 pl-10 pr-4 text-sm focus:ring-2 focus:ring-primary/20 transition-all placeholder:text-on-surface-variant"
        bind:value={appFilter}
        placeholder="Filter by app..."
        autocomplete="off"
      />
    </div>
    <div class="relative flex-1 max-w-xs">
      <span class="material-symbols-outlined absolute left-3 top-1/2 -translate-y-1/2 text-on-surface-variant text-[18px]">label</span>
      <input
        type="text"
        class="w-full bg-surface-container-lowest border-none rounded-xl py-2 pl-10 pr-4 text-sm focus:ring-2 focus:ring-primary/20 transition-all placeholder:text-on-surface-variant"
        bind:value={activityFilter}
        placeholder="Filter by activity..."
        autocomplete="off"
      />
    </div>
    <span class="text-xs text-on-surface-variant font-medium">
      {filteredItems.length} capture{filteredItems.length === 1 ? '' : 's'}
    </span>
  </div>

  <!-- Content Area -->
  {#if loading}
    <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-6">
      {#each Array.from({ length: 8 }, (_, i) => i) as skeleton (skeleton)}
        <div class="bg-surface-container-lowest rounded-2xl overflow-hidden animate-pulse">
          <div class="aspect-video bg-surface-container-high"></div>
          <div class="p-4 space-y-3">
            <div class="h-3 bg-surface-container-high rounded w-1/3"></div>
            <div class="h-4 bg-surface-container-high rounded w-3/4"></div>
            <div class="h-3 bg-surface-container-high rounded w-1/2"></div>
          </div>
        </div>
      {/each}
    </div>
  {:else if errorMessage}
    <div class="bg-surface-container-lowest rounded-[24px] p-8 text-center" role="alert">
      <span class="material-symbols-outlined text-4xl text-error mb-4">error_outline</span>
      <h3 class="text-lg font-semibold text-on-surface mb-2">Timeline unavailable</h3>
      <p class="text-sm text-secondary mb-6">{errorMessage}</p>
      <button
        type="button"
        class="px-6 py-2.5 bg-primary text-white rounded-xl font-semibold text-sm hover:opacity-90 transition-opacity"
        on:click={() => void loadTimelineForDate()}
      >
        Retry
      </button>
    </div>
  {:else if hourBuckets.length === 0}
    <div class="bg-surface-container-lowest rounded-[24px] p-12 text-center" role="status">
      <span class="material-symbols-outlined text-5xl text-on-surface-variant/40 mb-4">schedule</span>
      <h3 class="text-lg font-semibold text-on-surface mb-2">No captures for this day</h3>
      <p class="text-sm text-secondary">Try a different date or clear filters to broaden results.</p>
    </div>
  {:else}
    <div class="space-y-8">
      {#each hourBuckets as bucket (bucket.key)}
        <section aria-label={`Captures for ${bucket.heading}`}>
          <div class="flex items-center gap-3 mb-4">
            <div class="flex items-center gap-2 px-4 py-1.5 bg-surface-container-low rounded-full">
              <span class="material-symbols-outlined text-primary text-[18px]">schedule</span>
              <span class="text-sm font-bold text-on-surface">{bucket.heading}</span>
            </div>
            <span class="text-xs text-on-surface-variant font-medium">
              {bucket.captures.length} capture{bucket.captures.length === 1 ? '' : 's'}
            </span>
            <div class="flex-1 h-px bg-surface-container-high"></div>
          </div>

          <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-6">
            {#each bucket.captures as item (item.capture.id)}
              <CaptureCard
                capture={item.capture}
                extraction={item.extraction}
                on:open={() => { selectedItem = item; }}
              />
            {/each}
          </div>
        </section>
      {/each}
    </div>

    {#if hasMore}
      <div class="flex justify-center pt-4">
        <button
          type="button"
          class="px-8 py-3 bg-surface-container-highest text-on-surface rounded-xl font-semibold text-sm hover:bg-surface-container-high transition-colors disabled:opacity-50 disabled:cursor-wait"
          on:click={() => void loadMore()}
          disabled={loadingMore}
        >
          {loadingMore ? 'Loading...' : 'Load More'}
        </button>
      </div>
    {/if}
  {/if}

  <CaptureDetailsModal
    open={selectedItem !== null}
    capture={selectedItem?.capture ?? null}
    extraction={selectedItem?.extraction ?? null}
    on:close={() => { selectedItem = null; }}
  />
</div>
