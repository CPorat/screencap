<script lang="ts">
  import { createEventDispatcher } from 'svelte';

  import type { SearchResult } from './search-client';

  export let result: SearchResult;
  export let position = 1;

  const dispatch = createEventDispatcher<{ open: { result: SearchResult } }>();

  const timestampFormatter = new Intl.DateTimeFormat(undefined, {
    dateStyle: 'medium',
    timeStyle: 'short',
  });

  let imageFailed = false;
  let previousCaptureId: number | null = null;

  function buildScreenshotSrc(): string | null {
    if (result.capture.screenshot_url?.trim()) {
      return result.capture.screenshot_url;
    }

    if (!result.capture.screenshot_path?.trim()) {
      return null;
    }

    const normalizedPath = result.capture.screenshot_path.replace(/^\/+/, '');
    return `/api/screenshots/${encodeURIComponent(normalizedPath)}`;
  }

  function humanizeActivity(activityType: string | null): string {
    if (!activityType?.trim()) {
      return 'Unclassified';
    }

    return activityType
      .split('_')
      .filter(Boolean)
      .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
      .join(' ');
  }

  function openDetails(): void {
    dispatch('open', { result });
  }

  $: if (result.capture.id !== previousCaptureId) {
    imageFailed = false;
    previousCaptureId = result.capture.id;
  }

  $: capturedAt = new Date(result.capture.timestamp);
  $: hasTimestamp = Number.isFinite(capturedAt.getTime());
  $: timestampLabel = hasTimestamp ? timestampFormatter.format(capturedAt) : 'Timestamp unavailable';

  $: screenshotSrc = imageFailed ? null : buildScreenshotSrc();
  $: appLabel = result.capture.app_name?.trim() || 'Unknown app';
  $: projectLabel = result.extraction.project?.trim() || 'Unassigned project';
  $: activityLabel = humanizeActivity(result.extraction.activity_type);
  $: descriptionLabel =
    result.extraction.description?.trim() ||
    result.extraction.key_content?.trim() ||
    result.batchNarrative?.trim() ||
    'No extraction description available.';
  $: topics = result.extraction.topics;
</script>

<button class="search-result" type="button" on:click={openDetails} aria-label={`Open search result ${position}`}>
  <div class="search-result__thumb-wrap">
    {#if screenshotSrc}
      <img
        class="search-result__thumb"
        src={screenshotSrc}
        alt={`Screenshot from ${appLabel} at ${timestampLabel}`}
        loading="lazy"
        on:error={() => {
          imageFailed = true;
        }}
      />
    {:else}
      <div class="search-result__thumb search-result__thumb--fallback" role="img" aria-label="Screenshot unavailable">
        Screenshot unavailable
      </div>
    {/if}
  </div>

  <div class="search-result__meta">
    <p class="search-result__rank">#{position}</p>
    <p class="search-result__time">{timestampLabel}</p>
  </div>

  <h3 class="search-result__summary" title={descriptionLabel}>{descriptionLabel}</h3>

  <dl class="search-result__facts">
    <div>
      <dt>App</dt>
      <dd>{appLabel}</dd>
    </div>
    <div>
      <dt>Project</dt>
      <dd>{projectLabel}</dd>
    </div>
    <div>
      <dt>Activity</dt>
      <dd>{activityLabel}</dd>
    </div>
  </dl>

  {#if topics.length > 0}
    <div class="search-result__topics" aria-label="Topics">
      {#each topics.slice(0, 6) as topic (topic)}
        <span>{topic}</span>
      {/each}
    </div>
  {/if}
</button>

<style>
  .search-result {
    all: unset;
    display: grid;
    gap: 0.68rem;
    border: 1px solid rgb(246 241 231 / 30%);
    border-radius: 0.95rem;
    background:
      linear-gradient(162deg, rgb(72 84 120 / 38%), rgb(17 21 31 / 94%)),
      radial-gradient(circle at 12% 8%, rgb(112 255 227 / 12%), transparent 46%);
    padding: 0.9rem;
    box-shadow: 0.34rem 0.34rem 0 rgb(8 10 16 / 88%);
    min-width: 0;
    cursor: pointer;
    transition: transform 180ms ease, box-shadow 180ms ease, border-color 180ms ease;
  }

  .search-result:hover,
  .search-result:focus-visible {
    transform: translate(-0.08rem, -0.08rem);
    box-shadow: 0.42rem 0.42rem 0 rgb(8 10 16 / 92%);
    border-color: rgb(246 241 231 / 48%);
    outline: none;
  }

  .search-result__thumb-wrap {
    border-radius: 0.72rem;
    overflow: hidden;
    border: 1px solid rgb(246 241 231 / 28%);
  }

  .search-result__thumb {
    width: 100%;
    aspect-ratio: 16 / 10;
    object-fit: cover;
    background: rgb(8 9 14 / 86%);
    display: block;
  }

  .search-result__thumb--fallback {
    display: grid;
    place-content: center;
    min-height: 7.8rem;
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--paper-200);
    background: rgb(8 9 14 / 54%);
  }

  .search-result__meta {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 0.5rem;
  }

  .search-result__rank {
    font-size: 0.68rem;
    text-transform: uppercase;
    letter-spacing: 0.18em;
    color: var(--pulse);
  }

  .search-result__time {
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    color: var(--paper-200);
  }

  .search-result__summary {
    margin: 0;
    font-size: 1rem;
    letter-spacing: 0.03em;
    line-height: 1.2;
    overflow: hidden;
    display: -webkit-box;
    -webkit-line-clamp: 3;
    -webkit-box-orient: vertical;
    line-clamp: 3;
    text-overflow: ellipsis;
  }

  .search-result__facts {
    margin: 0;
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 0.45rem;
  }

  .search-result__facts div {
    min-width: 0;
    display: grid;
    gap: 0.14rem;
  }

  .search-result__facts dt {
    font-size: 0.62rem;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--paper-200);
  }

  .search-result__facts dd {
    margin: 0;
    font-size: 0.74rem;
    line-height: 1.32;
    color: var(--paper-100);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .search-result__topics {
    display: flex;
    flex-wrap: wrap;
    gap: 0.36rem;
  }

  .search-result__topics span {
    padding: 0.28rem 0.52rem;
    border-radius: 999px;
    border: 1px solid rgb(112 255 227 / 42%);
    background: rgb(112 255 227 / 8%);
    font-size: 0.64rem;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--paper-100);
  }

  @media (width <= 760px) {
    .search-result__facts {
      grid-template-columns: 1fr;
    }

    .search-result__facts dd {
      white-space: normal;
    }
  }
</style>
