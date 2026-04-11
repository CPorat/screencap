<script lang="ts">
  import type { TimelineCapture } from './types';

  export let item: TimelineCapture;

  const timeFormatter = new Intl.DateTimeFormat(undefined, {
    hour: 'numeric',
    minute: '2-digit',
  });

  const dateFormatter = new Intl.DateTimeFormat(undefined, {
    weekday: 'short',
    month: 'short',
    day: 'numeric',
  });

  let expanded = false;
  let imageFailed = false;

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

  function iconHue(appName: string, bundleId: string): number {
    let hash = 0;
    const seed = `${appName}:${bundleId}`;

    for (let index = 0; index < seed.length; index += 1) {
      hash = (hash * 33 + seed.charCodeAt(index)) % 360;
    }

    return hash;
  }

  function humanizeActivity(activityType: string): string {
    return activityType
      .split('_')
      .filter(Boolean)
      .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
      .join(' ');
  }

  function buildScreenshotSrc(): string | null {
    if (item.capture.screenshot_url?.trim()) {
      return item.capture.screenshot_url;
    }

    if (!item.capture.screenshot_path?.trim()) {
      return null;
    }

    const normalizedPath = item.capture.screenshot_path.replace(/^\/+/, '');
    return `/api/screenshots/${encodeURIComponent(normalizedPath)}`;
  }

  $: capturedAt = new Date(item.capture.timestamp);
  $: hasTimestamp = Number.isFinite(capturedAt.getTime());
  $: timeLabel = hasTimestamp ? timeFormatter.format(capturedAt) : 'Unknown time';
  $: dateLabel = hasTimestamp ? dateFormatter.format(capturedAt) : 'Timestamp unavailable';

  $: appLabel = item.capture.app_name?.trim() || 'Unknown app';
  $: bundleLabel = item.capture.bundle_id?.trim() || 'unidentified.bundle';
  $: appGlyph = toMonogram(appLabel);
  $: hue = iconHue(appLabel, bundleLabel);

  $: titleLabel = item.capture.window_title?.trim() || 'Untitled window';

  $: extraction = item.extraction;
  $: extractionStatus = item.capture.extraction_status ?? 'pending';
  $: extractionPending = !extraction && extractionStatus === 'pending';
  $: extractionMissing = !extraction && extractionStatus === 'processed';

  $: activityLabel = extraction?.activity_type
    ? humanizeActivity(extraction.activity_type)
    : extraction?.description?.trim() ||
      (extractionPending
        ? 'Pending analysis'
        : extractionMissing
          ? 'Extraction unavailable'
          : 'No activity classified');

  $: projectLabel = extraction?.project?.trim()
    ? extraction.project.trim()
    : extractionPending
      ? 'Awaiting project context'
      : extractionMissing
        ? 'Project unavailable'
        : 'Unassigned';

  $: summaryLabel =
    extraction?.key_content?.trim() ||
    extraction?.description?.trim() ||
    (extractionPending
      ? 'Capture queued for extraction.'
      : extractionMissing
        ? 'Capture is marked processed but no extraction payload is available.'
        : 'No summary available for this capture.');
  $: topics = extraction?.topics ?? [];
  $: people = extraction?.people ?? [];

  $: screenshotSrc = imageFailed ? null : buildScreenshotSrc();
</script>

<article class="capture-card" data-pending={extractionPending} aria-label={`Capture from ${timeLabel}`}>
  <header class="capture-card__header">
    <p class="capture-card__time">{timeLabel}</p>
    <p class="capture-card__date">{dateLabel}</p>
  </header>

  <div class="capture-card__app-chip">
    <span class="capture-card__glyph" aria-hidden="true" style={`--hue:${hue}`}>{appGlyph}</span>
    <div class="capture-card__app-copy">
      <p>{appLabel}</p>
      <small>{bundleLabel}</small>
    </div>
  </div>

  <h4>{titleLabel}</h4>

  <dl class="capture-card__signals">
    <div>
      <dt>Activity</dt>
      <dd>{activityLabel}</dd>
    </div>
    <div>
      <dt>Project</dt>
      <dd>{projectLabel}</dd>
    </div>
    <div>
      <dt>Summary</dt>
      <dd>{summaryLabel}</dd>
    </div>
  </dl>

  {#if screenshotSrc}
    <img
      class="capture-card__thumb"
      src={screenshotSrc}
      alt={`Thumbnail for ${titleLabel}`}
      loading="lazy"
      on:error={() => {
        imageFailed = true;
      }}
    />
  {:else}
    <div class="capture-card__placeholder" role="img" aria-label="Screenshot unavailable">
      Screenshot unavailable
    </div>
  {/if}

  <button class="capture-card__toggle" type="button" aria-expanded={expanded} on:click={() => (expanded = !expanded)}>
    {expanded ? 'Hide details' : 'Expand details'}
  </button>

  {#if expanded}
    <section class="capture-card__details" aria-label="Capture details">
      <div class="capture-card__detail-grid">
        <p>
          <strong>Status:</strong>
          {extractionStatus}
        </p>
        <p>
          <strong>Capture ID:</strong>
          {item.capture.id}
        </p>
      </div>

      {#if extraction}
        <p class="capture-card__context">
          <strong>Context:</strong>
          {extraction.app_context?.trim() || 'No extra context provided.'}
        </p>

        {#if topics.length > 0}
          <div class="capture-card__pill-row" aria-label="Topics">
            {#each topics as topic (topic)}
              <span>{topic}</span>
            {/each}
          </div>
        {/if}

        {#if people.length > 0}
          <p class="capture-card__people">
            <strong>People:</strong>
            {people.join(', ')}
          </p>
        {/if}
      {:else}
        <p class="capture-card__context">
          {extractionPending
            ? 'Extraction is still processing for this capture.'
            : extractionMissing
              ? 'Extraction data is missing for this processed capture.'
              : 'Extraction failed or produced no usable data.'}
        </p>
      {/if}

      {#if screenshotSrc}
        <img class="capture-card__full" src={screenshotSrc} alt={`Full screenshot for ${titleLabel}`} loading="lazy" />
      {/if}
    </section>
  {/if}
</article>

<style>
  .capture-card {
    display: grid;
    gap: 0.72rem;
    border: 1px solid rgb(246 241 231 / 30%);
    border-radius: 0.95rem;
    background:
      linear-gradient(165deg, rgb(76 87 122 / 38%), rgb(21 25 35 / 92%)),
      radial-gradient(circle at 8% 8%, rgb(112 255 227 / 13%), transparent 42%);
    padding: 0.92rem;
    box-shadow: 0.34rem 0.34rem 0 rgb(8 10 16 / 88%);
    min-width: 0;
    transition: transform 180ms ease, box-shadow 180ms ease, border-color 180ms ease;
    animation: card-rise 260ms ease both;
  }

  .capture-card:hover {
    transform: translate(-0.08rem, -0.08rem);
    box-shadow: 0.42rem 0.42rem 0 rgb(8 10 16 / 92%);
    border-color: rgb(246 241 231 / 48%);
  }

  .capture-card[data-pending='true'] {
    border-style: dashed;
  }

  @keyframes card-rise {
    from {
      opacity: 0;
      transform: translateY(0.5rem);
    }

    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  .capture-card__header {
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

  .capture-card__app-chip {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    width: fit-content;
    border: 1px solid rgb(246 241 231 / 28%);
    border-radius: 999px;
    padding: 0.26rem 0.58rem 0.26rem 0.3rem;
    background: rgb(11 14 22 / 80%);
  }

  .capture-card__glyph {
    width: 1.55rem;
    height: 1.55rem;
    border-radius: 999px;
    border: 1px solid hsl(var(--hue) 94% 72% / 65%);
    color: hsl(var(--hue) 100% 84%);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 0.66rem;
    font-weight: 700;
    letter-spacing: 0.08em;
    background: rgb(9 13 18 / 88%);
    flex-shrink: 0;
  }

  .capture-card__app-copy {
    display: grid;
    line-height: 1.1;
  }

  .capture-card__app-copy p {
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.1em;
  }

  .capture-card__app-copy small {
    color: rgb(221 213 198 / 78%);
    font-size: 0.65rem;
  }

  h4 {
    font-size: 1rem;
    line-height: 1.15;
  }

  .capture-card__signals {
    display: grid;
    gap: 0.5rem;
    margin: 0;
  }

  .capture-card__signals div {
    display: grid;
    gap: 0.15rem;
  }

  dt {
    font-size: 0.64rem;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: rgb(221 213 198 / 76%);
  }

  dd {
    margin: 0;
    font-size: 0.81rem;
    line-height: 1.35;
  }

  .capture-card__thumb,
  .capture-card__placeholder,
  .capture-card__full {
    width: 100%;
    border-radius: 0.72rem;
    border: 1px solid rgb(246 241 231 / 34%);
    background: rgb(8 10 18 / 86%);
    object-fit: cover;
  }

  .capture-card__thumb,
  .capture-card__placeholder {
    aspect-ratio: 16 / 10;
  }

  .capture-card__full {
    max-height: 18rem;
  }

  .capture-card__placeholder {
    display: grid;
    place-content: center;
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    color: rgb(221 213 198 / 72%);
  }

  .capture-card__toggle {
    justify-self: start;
    border: 1px solid rgb(112 255 227 / 50%);
    background: rgb(8 13 18 / 72%);
    color: var(--pulse);
    border-radius: 999px;
    padding: 0.32rem 0.72rem;
    font: inherit;
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    cursor: pointer;
  }

  .capture-card__toggle:hover {
    background: rgb(8 15 20 / 88%);
  }

  .capture-card__details {
    display: grid;
    gap: 0.55rem;
    border-top: 1px solid rgb(246 241 231 / 18%);
    padding-top: 0.62rem;
  }

  .capture-card__detail-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.45rem;
  }

  .capture-card__detail-grid p,
  .capture-card__context,
  .capture-card__people {
    font-size: 0.74rem;
    color: var(--paper-200);
  }

  .capture-card__pill-row {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
  }

  .capture-card__pill-row span {
    border: 1px solid rgb(255 179 71 / 45%);
    border-radius: 999px;
    padding: 0.2rem 0.48rem;
    font-size: 0.64rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    background: rgb(34 24 10 / 65%);
    color: rgb(255 222 171 / 92%);
  }
</style>
