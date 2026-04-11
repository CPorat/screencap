<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import type { CaptureRecord, ExtractionRecord } from '$lib/api';

  export let capture: CaptureRecord;
  export let extraction: ExtractionRecord | null = null;

  const dispatch = createEventDispatcher<{ open: { captureId: number } }>();

  const timeFormatter = new Intl.DateTimeFormat(undefined, {
    hour: 'numeric',
    minute: '2-digit',
  });

  let imageFailed = false;

  function buildScreenshotSrc(): string | null {
    if (capture.screenshot_url?.trim()) {
      return capture.screenshot_url;
    }

    if (!capture.screenshot_path?.trim()) {
      return null;
    }

    const normalizedPath = capture.screenshot_path.replace(/^\/+/, '');
    return `/api/screenshots/${encodeURIComponent(normalizedPath)}`;
  }

  function openDetails(): void {
    dispatch('open', { captureId: capture.id });
  }

  $: capturedAt = new Date(capture.timestamp);
  $: hasTimestamp = Number.isFinite(capturedAt.getTime());
  $: timeLabel = hasTimestamp ? timeFormatter.format(capturedAt) : 'Unknown time';
  $: appLabel = capture.app_name?.trim() || 'Unknown app';
  $: description =
    extraction?.description?.trim() ||
    extraction?.key_content?.trim() ||
    capture.primary_activity?.trim() ||
    capture.narrative?.trim() ||
    capture.batch_narrative?.trim() ||
    'No extraction description available.';

  $: screenshotSrc = imageFailed ? null : buildScreenshotSrc();
</script>

<button class="capture-card" type="button" on:click={openDetails} aria-label={`Open capture from ${timeLabel}`}>
  <div class="capture-card__thumb-wrap">
    {#if screenshotSrc}
      <img
        class="capture-card__thumb"
        src={screenshotSrc}
        alt={`Capture from ${appLabel} at ${timeLabel}`}
        loading="lazy"
        on:error={() => {
          imageFailed = true;
        }}
      />
    {:else}
      <div class="capture-card__thumb capture-card__thumb--fallback" role="img" aria-label="Screenshot unavailable">
        Unavailable
      </div>
    {/if}
  </div>

  <div class="capture-card__meta">
    <p class="capture-card__app" title={appLabel}>{appLabel}</p>
    <time class="capture-card__time" datetime={capture.timestamp}>{timeLabel}</time>
  </div>

  <p class="capture-card__description" title={description}>{description}</p>
</button>

<style>
  .capture-card {
    all: unset;
    display: grid;
    gap: 0.62rem;
    padding: 0.72rem;
    border-radius: 0.9rem;
    border: 1px solid rgb(246 241 231 / 34%);
    background:
      linear-gradient(145deg, rgb(48 55 78 / 78%), rgb(14 17 26 / 96%)),
      radial-gradient(circle at 20% 10%, rgb(255 179 71 / 18%), transparent 42%);
    box-shadow: 0.3rem 0.3rem 0 rgb(8 10 16 / 90%);
    cursor: pointer;
    min-width: 0;
    transition: transform 180ms ease, border-color 180ms ease, box-shadow 180ms ease;
  }

  .capture-card:hover,
  .capture-card:focus-visible {
    transform: translate(-0.05rem, -0.05rem);
    border-color: var(--pulse);
    box-shadow: 0.4rem 0.4rem 0 rgb(8 10 16 / 94%);
    outline: none;
  }

  .capture-card__thumb-wrap {
    border-radius: 0.72rem;
    overflow: hidden;
    border: 1px solid rgb(246 241 231 / 24%);
  }

  .capture-card__thumb {
    width: 100%;
    aspect-ratio: 16 / 10;
    object-fit: cover;
    display: block;
    background: rgb(7 9 14 / 92%);
  }

  .capture-card__thumb--fallback {
    display: grid;
    place-content: center;
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.14em;
    color: rgb(240 232 219 / 76%);
  }

  .capture-card__meta {
    display: flex;
    justify-content: space-between;
    gap: 0.75rem;
    align-items: baseline;
    min-width: 0;
  }

  .capture-card__app {
    font-size: 0.8rem;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--paper-100);
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .capture-card__time {
    font-family: var(--display-font);
    font-size: 0.88rem;
    color: var(--pulse);
    flex-shrink: 0;
  }

  .capture-card__description {
    margin: 0;
    font-size: 0.82rem;
    line-height: 1.34;
    color: var(--paper-200);
    overflow: hidden;
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    text-overflow: ellipsis;
    min-height: 2.2em;
  }
</style>