<script lang="ts">
  import { onMount } from 'svelte';
  import CaptureCard from '../lib/components/CaptureCard.svelte';
  import { api, type ActivityType, type ApiCapture, type AppCaptureCount, type Extraction } from '../lib/api';

  type ActivityFilter = 'all' | ActivityType;
  type TimelineCapture = ApiCapture & { extraction: Extraction | null };

  interface CaptureHourGroup {
    key: string;
    hourStamp: number;
    hourLabel: string;
    hourContext: string;
    captures: TimelineCapture[];
  }

  const activityOptions: Array<{ value: ActivityFilter; label: string }> = [
    { value: 'all', label: 'All activity' },
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
  ];

  const hourLabelFormatter = new Intl.DateTimeFormat(undefined, {
    hour: '2-digit',
    minute: '2-digit',
    hour12: false,
  });

  const hourContextFormatter = new Intl.DateTimeFormat(undefined, {
    hour: 'numeric',
    hour12: true,
  });

  let selectedDate = toInputDate(new Date());
  let selectedApp = 'all';
  let selectedActivity: ActivityFilter = 'all';

  let appOptions: AppCaptureCount[] = [];
  let hourGroups: CaptureHourGroup[] = [];

  let totalForDay = 0;
  let totalVisible = 0;
  let expandedCaptureId: number | null = null;

  let isLoading = false;
  let isLoadingApps = false;
  let errorMessage: string | null = null;

  let timelineRequestId = 0;

  onMount(() => {
    void loadApps();
  });

  $: void refreshTimeline(selectedDate, selectedApp, selectedActivity);

  async function loadApps(): Promise<void> {
    isLoadingApps = true;

    try {
      const response = await api.fetchApps();
      appOptions = response.apps
        .filter((app) => app.app_name.trim().length > 0)
        .sort((a, b) => a.app_name.localeCompare(b.app_name));
    } catch (error) {
      console.error('Failed to load app filters', error);
    } finally {
      isLoadingApps = false;
    }
  }

  async function refreshTimeline(
    dateValue: string,
    appFilter: string,
    activityFilter: ActivityFilter,
  ): Promise<void> {
    const dayRange = toDayRange(dateValue);

    if (!dayRange) {
      errorMessage = 'Choose a valid date.';
      hourGroups = [];
      totalForDay = 0;
      totalVisible = 0;
      return;
    }

    const requestId = ++timelineRequestId;
    isLoading = true;
    errorMessage = null;

    try {
      const capturesResponse = await api.getCaptures({
        from: dayRange.from,
        to: dayRange.to,
        app: appFilter === 'all' ? undefined : appFilter,
        limit: 600,
      });

      if (requestId !== timelineRequestId) {
        return;
      }

      const baseCaptures: TimelineCapture[] = capturesResponse.captures
        .map((capture) => ({ ...capture, extraction: null }))
        .sort((a, b) => Date.parse(b.timestamp) - Date.parse(a.timestamp));

      totalForDay = baseCaptures.length;

      const details = await Promise.all(
        baseCaptures.map(async (capture) => {
          if (capture.extraction_status !== 'processed') {
            return [capture.id, null] as const;
          }

          try {
            const detail = await api.fetchCaptureDetail(capture.id);
            return [capture.id, detail.extraction] as const;
          } catch {
            return [capture.id, null] as const;
          }
        }),
      );

      if (requestId !== timelineRequestId) {
        return;
      }

      const extractionByCaptureId = new Map<number, TimelineCapture['extraction']>(details);

      let capturesWithDetails = baseCaptures.map((capture) => ({
        ...capture,
        extraction: extractionByCaptureId.get(capture.id) ?? null,
      }));

      if (activityFilter !== 'all') {
        capturesWithDetails = capturesWithDetails.filter(
          (capture) => capture.extraction?.activity_type === activityFilter,
        );
      }

      totalVisible = capturesWithDetails.length;
      hourGroups = groupByHour(capturesWithDetails);

      if (!capturesWithDetails.some((capture) => capture.id === expandedCaptureId)) {
        expandedCaptureId = null;
      }
    } catch (error) {
      if (requestId !== timelineRequestId) {
        return;
      }

      errorMessage = error instanceof Error ? error.message : 'Failed to load timeline captures.';
      hourGroups = [];
      totalForDay = 0;
      totalVisible = 0;
    } finally {
      if (requestId === timelineRequestId) {
        isLoading = false;
      }
    }
  }

  function groupByHour(captures: TimelineCapture[]): CaptureHourGroup[] {
    const groups = new Map<string, CaptureHourGroup>();

    for (const capture of captures) {
      const captureDate = new Date(capture.timestamp);
      const hourDate = new Date(captureDate);
      hourDate.setMinutes(0, 0, 0);

      const key = hourDate.toISOString();
      const hourStamp = hourDate.getTime();

      if (!groups.has(key)) {
        groups.set(key, {
          key,
          hourStamp,
          hourLabel: hourLabelFormatter.format(hourDate),
          hourContext: hourContextFormatter.format(hourDate),
          captures: [],
        });
      }

      groups.get(key)?.captures.push(capture);
    }

    return [...groups.values()].sort((a, b) => b.hourStamp - a.hourStamp);
  }

  function toInputDate(value: Date): string {
    const local = new Date(value);
    local.setMinutes(local.getMinutes() - local.getTimezoneOffset());
    return local.toISOString().slice(0, 10);
  }

  function toDayRange(inputDate: string): { from: string; to: string } | null {
    if (!/^\d{4}-\d{2}-\d{2}$/.test(inputDate)) {
      return null;
    }

    const [year, month, day] = inputDate.split('-').map((part) => Number.parseInt(part, 10));
    const start = new Date(year, month - 1, day, 0, 0, 0, 0);

    if (Number.isNaN(start.getTime())) {
      return null;
    }

    const end = new Date(year, month - 1, day + 1, 0, 0, 0, 0);
    end.setMilliseconds(end.getMilliseconds() - 1);

    return {
      from: start.toISOString(),
      to: end.toISOString(),
    };
  }

  function toggleExpanded(captureId: number): void {
    expandedCaptureId = expandedCaptureId === captureId ? null : captureId;
  }
</script>

<section class="view timeline-view">
  <header class="timeline-view__header">
    <p class="view__kicker">Timeline</p>
    <h2 class="view__title">Today, reconstructed by the hour.</h2>
    <p class="view__copy">
      Review every capture with extraction context. Track intent shifts, context switches, and deep-work
      windows in a single flow.
    </p>
  </header>

  <section class="filters" aria-label="Timeline filters">
    <label>
      <span>Date</span>
      <input type="date" bind:value={selectedDate} />
    </label>

    <label>
      <span>App</span>
      <select bind:value={selectedApp} disabled={isLoadingApps}>
        <option value="all">All apps</option>
        {#each appOptions as app (app.app_name)}
          <option value={app.app_name}>{app.app_name} ({app.capture_count})</option>
        {/each}
      </select>
    </label>

    <label>
      <span>Activity</span>
      <select bind:value={selectedActivity}>
        {#each activityOptions as option (option.value)}
          <option value={option.value}>{option.label}</option>
        {/each}
      </select>
    </label>
  </section>

  <section class="timeline-view__meta" aria-live="polite">
    {#if isLoading}
      <p>Loading captures…</p>
    {:else if errorMessage}
      <p class="is-error">{errorMessage}</p>
    {:else if totalForDay !== totalVisible}
      <p>Showing {totalVisible} of {totalForDay} captures for this day.</p>
    {:else}
      <p>{totalVisible} captures on this day.</p>
    {/if}
  </section>

  {#if isLoading}
    <section class="timeline-state timeline-state--loading" aria-label="Loading timeline">
      <div></div>
      <div></div>
      <div></div>
    </section>
  {:else if !errorMessage && hourGroups.length === 0}
    <section class="timeline-state timeline-state--empty" aria-label="No captures found">
      <h3>No captures for this day.</h3>
      <p>Pick another date or remove filters to widen the timeline.</p>
    </section>
  {:else if !errorMessage}
    <section class="hour-groups" aria-label="Capture timeline">
      {#each hourGroups as group (group.key)}
        <article class="hour-group">
          <header class="hour-group__header">
            <p class="hour-group__time">{group.hourLabel}</p>
            <p class="hour-group__context">{group.hourContext}</p>
            <p class="hour-group__count">{group.captures.length} capture(s)</p>
          </header>

          <div class="hour-group__captures">
            {#each group.captures as capture (capture.id)}
              <CaptureCard
                {capture}
                expanded={expandedCaptureId === capture.id}
                on:toggle={(event) => toggleExpanded(event.detail)}
              />
            {/each}
          </div>
        </article>
      {/each}
    </section>
  {/if}
</section>

<style>
  .timeline-view {
    background:
      linear-gradient(145deg, rgba(255, 255, 255, 0.03), transparent 42%),
      transparent;
    gap: 1.2rem;
  }

  .timeline-view__header {
    display: grid;
    gap: 0.8rem;
  }

  .filters {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 0.75rem;
    padding: 0.8rem;
    border: 2px solid var(--surface-border);
    background: color-mix(in srgb, var(--surface-lift) 90%, #000 10%);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--surface-border) 20%, transparent 80%);
  }

  .filters label {
    display: grid;
    gap: 0.34rem;
  }

  .filters span {
    font-size: 0.68rem;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: var(--accent);
  }

  .filters input,
  .filters select {
    width: 100%;
    border: 2px solid color-mix(in srgb, var(--surface-border) 70%, #000 30%);
    background: color-mix(in srgb, var(--surface) 92%, #000 8%);
    color: var(--text);
    font: inherit;
    font-size: 0.8rem;
    padding: 0.6rem 0.62rem;
    outline: none;
    transition: transform 120ms ease, border-color 120ms ease, box-shadow 120ms ease;
  }

  .filters input:focus,
  .filters select:focus {
    border-color: var(--accent);
    box-shadow: 0 0 0 2px rgba(255, 208, 0, 0.18);
    transform: translateY(-1px);
  }

  .timeline-view__meta {
    border-left: 3px solid var(--accent);
    padding-left: 0.75rem;
  }

  .timeline-view__meta p {
    margin: 0;
    font-size: 0.76rem;
    letter-spacing: 0.07em;
    text-transform: uppercase;
  }

  .timeline-view__meta .is-error {
    color: #ff8f8f;
  }

  .timeline-state {
    border: 2px dashed color-mix(in srgb, var(--surface-border) 55%, transparent 45%);
    background: color-mix(in srgb, var(--surface) 75%, #000 25%);
    padding: clamp(1rem, 3vw, 2rem);
    min-height: 220px;
    display: grid;
    place-items: center;
    text-align: center;
    gap: 0.5rem;
  }

  .timeline-state--loading {
    gap: 0.8rem;
  }

  .timeline-state--loading div {
    width: min(100%, 460px);
    height: 20px;
    border: 1px solid color-mix(in srgb, var(--surface-border) 45%, transparent 55%);
    background: linear-gradient(
      120deg,
      color-mix(in srgb, var(--surface-lift) 60%, #000 40%) 0%,
      color-mix(in srgb, var(--surface-shadow) 35%, var(--surface-lift) 65%) 50%,
      color-mix(in srgb, var(--surface-lift) 60%, #000 40%) 100%
    );
    background-size: 180% 100%;
    animation: sweep 1.2s linear infinite;
  }

  .timeline-state--empty h3 {
    margin: 0;
    font-family: var(--display-font);
    text-transform: uppercase;
    letter-spacing: 0.03em;
    font-size: clamp(1.2rem, 2.5vw, 1.8rem);
  }

  .timeline-state--empty p {
    margin: 0;
    color: var(--muted);
    max-width: 34ch;
  }

  .hour-groups {
    display: grid;
    gap: 1.1rem;
  }

  .hour-group {
    border: 2px solid color-mix(in srgb, var(--surface-border) 85%, #000 15%);
    background: linear-gradient(130deg, rgba(46, 92, 255, 0.08), transparent 55%), var(--surface);
    box-shadow: 7px 7px 0 color-mix(in srgb, var(--surface-shadow) 58%, #000 42%);
    overflow: hidden;
  }

  .hour-group__header {
    display: grid;
    grid-template-columns: auto auto 1fr;
    align-items: baseline;
    gap: 0.8rem;
    padding: 0.85rem 0.95rem;
    border-bottom: 2px solid color-mix(in srgb, var(--surface-border) 80%, #000 20%);
    background: color-mix(in srgb, var(--surface-lift) 78%, #000 22%);
  }

  .hour-group__time {
    margin: 0;
    font-family: var(--display-font);
    font-size: clamp(1.35rem, 4vw, 2rem);
    line-height: 0.95;
    letter-spacing: 0.03em;
    text-transform: uppercase;
  }

  .hour-group__context {
    margin: 0;
    color: var(--accent);
    font-size: 0.7rem;
    letter-spacing: 0.14em;
    text-transform: uppercase;
  }

  .hour-group__count {
    margin: 0;
    justify-self: end;
    color: var(--muted);
    font-size: 0.68rem;
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }

  .hour-group__captures {
    display: grid;
    gap: 0.7rem;
    padding: 0.8rem;
  }

  @keyframes sweep {
    0% {
      background-position: 180% 0;
    }

    100% {
      background-position: -20% 0;
    }
  }

  @media (max-width: 980px) {
    .filters {
      grid-template-columns: 1fr;
    }
  }

  @media (max-width: 700px) {
    .hour-group__header {
      grid-template-columns: 1fr;
      gap: 0.2rem;
    }

    .hour-group__count {
      justify-self: start;
    }
  }
</style>
