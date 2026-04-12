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
  let previousRenderKey: string | null = null;

  function buildScreenshotSrc(): string | null {
    if (result.capture?.screenshot_url?.trim()) {
      return result.capture.screenshot_url;
    }

    if (!result.capture?.screenshot_path?.trim()) {
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
    if (interactive) {
      dispatch('open', { result });
    }
  }

  $: renderKey = `${result.sourceType}-${result.capture?.id ?? result.insight?.id ?? result.timestamp}`;
  $: if (renderKey !== previousRenderKey) {
    imageFailed = false;
    previousRenderKey = renderKey;
  }

  $: interactive = result.sourceType === 'extraction' && result.capture !== null;
  $: capturedAt = new Date(result.timestamp);
  $: hasTimestamp = Number.isFinite(capturedAt.getTime());
  $: timestampLabel = hasTimestamp ? timestampFormatter.format(capturedAt) : 'Timestamp unavailable';
  $: screenshotSrc = imageFailed ? null : buildScreenshotSrc();
  $: appLabel =
    result.capture?.app_name?.trim() || (result.sourceType === 'insight' ? 'Insight summary' : 'Unknown app');
  $: projectLabel = result.primaryProject?.trim() || 'Unassigned project';
  $: activityLabel = humanizeActivity(result.primaryActivityType);
  $: descriptionLabel = result.narrative?.trim() || result.batchNarrative?.trim() || 'No indexed narrative available.';
  $: topics = result.extraction?.topics ?? [];
  $: sourceLabel =
    result.sourceType === 'insight'
      ? `${result.insight?.insight_type ?? 'search'} insight`
      : 'extraction';
</script>

<button
  class="group bg-surface-container-lowest rounded-2xl overflow-hidden transition-all duration-300 hover:shadow-2xl hover:shadow-outline/10 hover:-translate-y-1 text-left w-full {interactive ? 'cursor-pointer' : 'cursor-default'}"
  type="button"
  disabled={!interactive}
  on:click={openDetails}
  aria-label={interactive ? `Open search result ${position}` : `Search result ${position}`}
>
  <div class="aspect-video relative overflow-hidden bg-surface-container-high">
    {#if screenshotSrc}
      <img
        class="w-full h-full object-cover transition-transform duration-500 group-hover:scale-105"
        src={screenshotSrc}
        alt={`Screenshot from ${appLabel} at ${timestampLabel}`}
        loading="lazy"
        on:error={() => { imageFailed = true; }}
      />
    {:else}
      <div class="w-full h-full flex items-center justify-center text-on-surface-variant text-xs uppercase tracking-widest font-bold">
        {sourceLabel}
      </div>
    {/if}
    {#if interactive}
      <div class="absolute inset-0 bg-gradient-to-t from-black/40 to-transparent opacity-0 group-hover:opacity-100 transition-opacity flex items-end p-4">
        <span class="bg-surface-container-lowest/90 backdrop-blur text-on-surface rounded-lg px-3 py-1.5 text-xs font-bold flex items-center gap-1">
          <span class="material-symbols-outlined text-sm" style="font-variation-settings: 'FILL' 1;">play_arrow</span> Preview
        </span>
      </div>
    {/if}
  </div>

  <div class="p-4">
    <div class="flex items-start justify-between mb-2">
      <span class="text-[10px] font-bold text-on-surface-variant tracking-wider uppercase">{appLabel}</span>
      <span class="px-2 py-0.5 bg-primary-fixed text-primary rounded text-[10px] font-bold">{activityLabel}</span>
    </div>

    <p class="text-sm font-semibold text-on-surface mb-1 line-clamp-2 italic">"{descriptionLabel}"</p>

    {#if topics.length > 0}
      <div class="flex flex-wrap gap-1.5 mt-2 mb-2">
        {#each topics.slice(0, 4) as topic (topic)}
          <span class="px-2 py-0.5 bg-surface-container-low text-on-surface-variant rounded-full text-[10px] font-medium">{topic}</span>
        {/each}
      </div>
    {/if}

    <div class="mt-3 flex items-center justify-between text-[10px] font-medium text-on-surface-variant">
      <time datetime={result.timestamp}>{timestampLabel}</time>
      <span>{projectLabel}</span>
    </div>
  </div>
</button>
