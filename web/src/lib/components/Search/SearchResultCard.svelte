<script lang="ts">
  import type { SearchResult } from './search-client';

  export let result: SearchResult;
  export let position = 1;

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

  function iconHue(appName: string, project: string): number {
    let hash = 0;
    const seed = `${appName}:${project}`;

    for (let index = 0; index < seed.length; index += 1) {
      hash = (hash * 31 + seed.charCodeAt(index)) % 360;
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
    if (result.capture.screenshot_url?.trim()) {
      return result.capture.screenshot_url;
    }

    if (!result.capture.screenshot_path?.trim()) {
      return null;
    }

    const normalizedPath = result.capture.screenshot_path.replace(/^\/+/, '');
    return `/api/screenshots/${encodeURIComponent(normalizedPath)}`;
  }

  $: capturedAt = new Date(result.capture.timestamp);
  $: hasTimestamp = Number.isFinite(capturedAt.getTime());
  $: timeLabel = hasTimestamp ? timeFormatter.format(capturedAt) : 'Unknown time';
  $: dateLabel = hasTimestamp ? dateFormatter.format(capturedAt) : 'Timestamp unavailable';

  $: appLabel = result.capture.app_name?.trim() || 'Unknown app';
  $: projectLabel = result.extraction.project?.trim() || 'Unassigned';
  $: appGlyph = toMonogram(appLabel);
  $: hue = iconHue(appLabel, projectLabel);

  $: activityLabel = result.extraction.activity_type
    ? humanizeActivity(result.extraction.activity_type)
    : result.extraction.description?.trim() || 'Unclassified';

  $: titleLabel = result.capture.window_title?.trim() || 'Untitled window';
  $: summaryLabel =
    result.extraction.key_content?.trim() ||
    result.extraction.description?.trim() ||
    result.batchNarrative?.trim() ||
    'No summary returned for this result.';

  $: topics = result.extraction.topics;
  $: people = result.extraction.people;

  $: screenshotSrc = imageFailed ? null : buildScreenshotSrc();
</script>

<article class="capture-card" aria-label={`Search hit ${position} from ${timeLabel}`}>
  <header class="capture-card__header">
    <div>
      <p class="capture-card__rank">#{position}</p>
      <p class="capture-card__score">{result.relevance}% relevance</p>
    </div>
    <div class="capture-card__time-group">
      <p class="capture-card__time">{timeLabel}</p>
      <p class="capture-card__date">{dateLabel}</p>
    </div>
  </header>

  <div class="capture-card__app-chip">
    <span class="capture-card__glyph" aria-hidden="true" style={`--hue:${hue}`}>{appGlyph}</span>
    <div class="capture-card__app-copy">
      <p>{appLabel}</p>
      <small>{projectLabel}</small>
    </div>
  </div>

  <h4>{titleLabel}</h4>

  <dl class="capture-card__signals">
    <div>
      <dt>Activity</dt>
      <dd>{activityLabel}</dd>
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
    <section class="capture-card__details" aria-label="Search hit details">
      <div class="capture-card__detail-grid">
        <p>
          <strong>Rank:</strong>
          {result.rank.toFixed(3)}
        </p>
        <p>
          <strong>Capture ID:</strong>
          {result.capture.id}
        </p>
      </div>

      <p class="capture-card__context">
        <strong>Context:</strong>
        {result.extraction.app_context?.trim() || result.batchNarrative?.trim() || 'No additional context returned.'}
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

      {#if screenshotSrc}
        <img class="capture-card__full" src={screenshotSrc} alt={`Full screenshot for ${titleLabel}`} loading="lazy" />
      {/if}
    </section>
  {/if}
</article>

<style>
  .capture-card {
    display: grid;
    gap: 0.74rem;
    border: 1px solid rgb(246 241 231 / 30%);
    border-radius: 0.95rem;
    background:
      linear-gradient(162deg, rgb(72 84 120 / 38%), rgb(17 21 31 / 94%)),
      radial-gradient(circle at 12% 8%, rgb(112 255 227 / 12%), transparent 46%);
    padding: 0.95rem;
    box-shadow: 0.34rem 0.34rem 0 rgb(8 10 16 / 88%);
    min-width: 0;
    transition: transform 180ms ease, box-shadow 180ms ease, border-color 180ms ease;
    animation: card-rise 240ms ease both;
  }

  .capture-card:hover {
    transform: translate(-0.08rem, -0.08rem);
    box-shadow: 0.42rem 0.42rem 0 rgb(8 10 16 / 92%);
    border-color: rgb(246 241 231 / 48%);
  }

  .capture-card__header {
    display: flex;
    justify-content: space-between;
    gap: 0.7rem;
    align-items: baseline;
  }

  .capture-card__rank {
    font-size: 0.7rem;
    letter-spacing: 0.18em;
    text-transform: uppercase;
    color: var(--pulse);
  }

  .capture-card__score {
    font-size: 0.78rem;
    color: var(--paper-200);
  }

  .capture-card__time-group {
    text-align: right;
    display: grid;
    gap: 0.1rem;
  }

  .capture-card__time {
    font-size: 0.95rem;
    font-family: var(--display-font);
    letter-spacing: 0.03em;
  }

  .capture-card__date {
    font-size: 0.73rem;
    color: var(--paper-200);
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .capture-card__app-chip {
    display: grid;
    grid-template-columns: auto 1fr;
    align-items: center;
    gap: 0.65rem;
  }

  .capture-card__glyph {
    width: 2.4rem;
    aspect-ratio: 1;
    border-radius: 0.7rem;
    display: grid;
    place-items: center;
    font-family: var(--display-font);
    font-size: 0.82rem;
    color: var(--ink-950);
    background: hsl(var(--hue) 88% 68%);
    box-shadow: inset 0 0 0 1px rgb(255 255 255 / 46%);
  }

  .capture-card__app-copy p {
    font-family: var(--display-font);
    font-size: 0.85rem;
    letter-spacing: 0.05em;
    text-transform: uppercase;
  }

  .capture-card__app-copy small {
    font-size: 0.72rem;
    color: var(--paper-200);
    letter-spacing: 0.06em;
  }

  h4 {
    font-size: 1.15rem;
    letter-spacing: 0.04em;
  }

  .capture-card__signals {
    display: grid;
    gap: 0.55rem;
    margin: 0;
  }

  .capture-card__signals div {
    display: grid;
    gap: 0.16rem;
  }

  .capture-card__signals dt {
    font-size: 0.66rem;
    text-transform: uppercase;
    letter-spacing: 0.15em;
    color: var(--paper-200);
  }

  .capture-card__signals dd {
    margin: 0;
    font-size: 0.81rem;
    color: var(--paper-100);
    line-height: 1.4;
  }

  .capture-card__thumb {
    width: 100%;
    border-radius: 0.72rem;
    border: 1px solid rgb(246 241 231 / 30%);
    object-fit: cover;
    aspect-ratio: 16 / 10;
    background: rgb(8 9 14 / 86%);
  }

  .capture-card__placeholder {
    border-radius: 0.72rem;
    border: 1px dashed rgb(246 241 231 / 35%);
    min-height: 7.8rem;
    display: grid;
    place-items: center;
    color: var(--paper-200);
    background: rgb(8 9 14 / 54%);
    font-size: 0.78rem;
    text-transform: uppercase;
    letter-spacing: 0.12em;
  }

  .capture-card__toggle {
    border: 1px solid rgb(255 179 71 / 52%);
    border-radius: 0.66rem;
    padding: 0.52rem 0.75rem;
    background: rgb(255 179 71 / 12%);
    color: var(--paper-100);
    font-size: 0.68rem;
    text-transform: uppercase;
    letter-spacing: 0.14em;
    cursor: pointer;
    transition: border-color 150ms ease, transform 150ms ease, background 150ms ease;
  }

  .capture-card__toggle:hover {
    border-color: rgb(255 179 71 / 82%);
    transform: translateY(-1px);
    background: rgb(255 179 71 / 18%);
  }

  .capture-card__details {
    border-top: 1px solid rgb(246 241 231 / 22%);
    padding-top: 0.7rem;
    display: grid;
    gap: 0.66rem;
  }

  .capture-card__detail-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.46rem;
  }

  .capture-card__detail-grid p,
  .capture-card__context,
  .capture-card__people {
    font-size: 0.76rem;
    color: var(--paper-200);
    line-height: 1.4;
  }

  .capture-card__context strong,
  .capture-card__people strong,
  .capture-card__detail-grid strong {
    color: var(--paper-100);
    font-family: var(--display-font);
    letter-spacing: 0.04em;
    text-transform: uppercase;
    font-size: 0.68rem;
  }

  .capture-card__pill-row {
    display: flex;
    flex-wrap: wrap;
    gap: 0.45rem;
  }

  .capture-card__pill-row span {
    padding: 0.32rem 0.5rem;
    border-radius: 999px;
    font-size: 0.66rem;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    border: 1px solid rgb(112 255 227 / 42%);
    background: rgb(112 255 227 / 8%);
    color: var(--paper-100);
  }

  .capture-card__full {
    width: 100%;
    border-radius: 0.72rem;
    border: 1px solid rgb(246 241 231 / 30%);
    background: rgb(8 9 14 / 86%);
  }

  @keyframes card-rise {
    from {
      opacity: 0;
      transform: translateY(0.44rem);
    }

    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
</style>
