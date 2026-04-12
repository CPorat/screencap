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
  $: activityType = extraction?.activity_type?.trim() || null;
  $: screenshotSrc = imageFailed ? null : buildScreenshotSrc();
</script>

<button
  class="group bg-surface-container-lowest rounded-2xl overflow-hidden transition-all duration-300 hover:shadow-2xl hover:shadow-outline/10 hover:-translate-y-1 text-left w-full cursor-pointer"
  type="button"
  on:click={openDetails}
  aria-label={`Open capture from ${timeLabel}`}
>
  <div class="aspect-video relative overflow-hidden bg-surface-container-high">
    {#if screenshotSrc}
      <img
        class="w-full h-full object-cover transition-transform duration-500 group-hover:scale-105"
        src={screenshotSrc}
        alt={`Capture from ${appLabel} at ${timeLabel}`}
        loading="lazy"
        on:error={() => { imageFailed = true; }}
      />
    {:else}
      <div class="w-full h-full flex items-center justify-center text-on-surface-variant text-xs uppercase tracking-widest font-bold">
        No preview
      </div>
    {/if}
    <div class="absolute inset-0 bg-gradient-to-t from-black/40 to-transparent opacity-0 group-hover:opacity-100 transition-opacity flex items-end p-4">
      <span class="bg-surface-container-lowest/90 backdrop-blur text-on-surface rounded-lg px-3 py-1.5 text-xs font-bold flex items-center gap-1">
        <span class="material-symbols-outlined text-sm" style="font-variation-settings: 'FILL' 1;">play_arrow</span> Preview
      </span>
    </div>
  </div>

  <div class="p-4">
    <div class="flex items-start justify-between mb-2">
      <span class="text-[10px] font-bold text-on-surface-variant tracking-wider uppercase">{appLabel}</span>
      {#if activityType}
        <span class="px-2 py-0.5 bg-primary-fixed text-primary rounded text-[10px] font-bold">{activityType}</span>
      {/if}
    </div>

    <p class="text-sm font-semibold text-on-surface mb-1 truncate italic">"{description}"</p>

    <div class="mt-3 flex items-center justify-between text-[10px] font-medium text-on-surface-variant">
      <time datetime={capture.timestamp}>{timeLabel}</time>
      {#if capture.window_title?.trim()}
        <span class="truncate ml-2 max-w-[50%]">{capture.window_title}</span>
      {/if}
    </div>
  </div>
</button>
