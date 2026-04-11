<script lang="ts">
  import type { CaptureRecord } from '$lib/api';

  export let capture: CaptureRecord;

  const timeFormatter = new Intl.DateTimeFormat(undefined, {
    hour: 'numeric',
    minute: '2-digit',
  });

  const dateFormatter = new Intl.DateTimeFormat(undefined, {
    weekday: 'short',
    month: 'short',
    day: 'numeric',
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

  function toMonogram(appName: string): string {
    const parts = appName
      .split(/\s+/)
      .map((part) => part.trim())
      .filter(Boolean);

    if (parts.length === 0) {
      return '??';
    }

    return parts
      .slice(0, 2)
      .map((part) => part.charAt(0).toUpperCase())
      .join('');
  }

  $: capturedAt = new Date(capture.timestamp);
  $: hasTimestamp = Number.isFinite(capturedAt.getTime());
  $: timeLabel = hasTimestamp ? timeFormatter.format(capturedAt) : 'Unknown time';
  $: dateLabel = hasTimestamp ? dateFormatter.format(capturedAt) : 'Timestamp unavailable';

  $: appLabel = capture.app_name?.trim() || 'Unknown app';
  $: appGlyph = toMonogram(appLabel);
  $: titleLabel = capture.window_title?.trim() || 'Untitled window';

  $: activityLabel =
    capture.primary_activity?.trim() ||
    capture.narrative?.trim() ||
    capture.batch_narrative?.trim() ||
    'Processing...';

  $: screenshotSrc = imageFailed ? null : buildScreenshotSrc();
</script>

<article class="capture-card" aria-label={`Capture from ${timeLabel}`}>
  <header class="capture-card__meta">
    <p class="capture-card__time">{timeLabel}</p>
    <p class="capture-card__date">{dateLabel}</p>
  </header>

  <div class="capture-card__app">
    <span class="capture-card__glyph" aria-hidden="true">{appGlyph}</span>
    <p>{appLabel}</p>
  </div>

  <h3>{titleLabel}</h3>
  <p class="capture-card__activity">{activityLabel}</p>

  {#if screenshotSrc}
    <img
      src={screenshotSrc}
      alt={`Thumbnail for ${titleLabel}`}
      loading="lazy"
      on:error={() => {
        imageFailed = true;
      }}
    />
  {:else}
    <div class="capture-card__placeholder" role="img" aria-label="Screenshot unavailable">
      Thumbnail unavailable
    </div>
  {/if}
</article>

<style>
  .capture-card {
    display: grid;
    gap: 0.72rem;
    border: 2px solid rgb(246 241 231 / 38%);
    border-radius: 0.95rem;
    background:
      linear-gradient(165deg, rgb(76 87 122 / 42%), rgb(21 25 35 / 92%)),
      radial-gradient(circle at 8% 8%, rgb(112 255 227 / 12%), transparent 42%);
    padding: 0.92rem;
    box-shadow: 0.38rem 0.38rem 0 rgb(10 12 18 / 90%);
    min-width: 0;
    animation: card-rise 260ms ease both;
  }

  @keyframes card-rise {
    from {
      opacity: 0;
      transform: translateY(0.45rem);
    }

    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  .capture-card__meta {
    display: flex;
    justify-content: space-between;
    gap: 0.8rem;
    align-items: baseline;
  }

  .capture-card__time {
    font-family: var(--display-font);
    font-size: 1rem;
    letter-spacing: 0.03em;
  }

  .capture-card__date {
    color: var(--paper-200);
    font-size: 0.74rem;
    text-transform: uppercase;
    letter-spacing: 0.12em;
  }

  .capture-card__app {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    width: fit-content;
    border: 1px solid rgb(246 241 231 / 35%);
    border-radius: 999px;
    padding: 0.26rem 0.58rem 0.26rem 0.28rem;
    background: rgb(14 17 26 / 76%);
  }

  .capture-card__app p {
    font-size: 0.76rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  .capture-card__glyph {
    width: 1.52rem;
    height: 1.52rem;
    border-radius: 999px;
    border: 1px solid rgb(112 255 227 / 44%);
    color: var(--pulse);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 0.66rem;
    font-weight: 700;
    letter-spacing: 0.08em;
    background: rgb(9 13 18 / 78%);
  }

  h3 {
    font-size: 1.02rem;
    line-height: 1.1;
  }

  .capture-card__activity {
    color: var(--paper-200);
    font-size: 0.84rem;
    line-height: 1.35;
    min-height: 2.3em;
  }

  img,
  .capture-card__placeholder {
    width: 100%;
    border-radius: 0.74rem;
    border: 1px solid rgb(246 241 231 / 36%);
    background: rgb(8 10 18 / 86%);
    aspect-ratio: 16 / 10;
    object-fit: cover;
  }

  .capture-card__placeholder {
    display: grid;
    place-content: center;
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    color: rgb(221 213 198 / 72%);
  }
</style>
