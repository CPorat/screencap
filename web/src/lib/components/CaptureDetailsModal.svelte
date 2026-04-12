<script lang="ts">
  import { browser } from '$app/environment';
  import { createEventDispatcher, onDestroy } from 'svelte';
  import type { CaptureRecord, ExtractionRecord } from '$lib/api';

  export let open = false;
  export let capture: CaptureRecord | null = null;
  export let extraction: ExtractionRecord | null = null;

  const dispatch = createEventDispatcher<{ close: void }>();

  const timestampFormatter = new Intl.DateTimeFormat(undefined, {
    dateStyle: 'medium',
    timeStyle: 'short',
  });

  let imageFailed = false;
  let scrollLocked = false;
  let previousCaptureId: number | null = null;

  function buildScreenshotSrc(target: CaptureRecord): string | null {
    if (target.screenshot_url?.trim()) {
      return target.screenshot_url;
    }

    if (!target.screenshot_path?.trim()) {
      return null;
    }

    const normalizedPath = target.screenshot_path.replace(/^\/+/, '');
    return `/api/screenshots/${encodeURIComponent(normalizedPath)}`;
  }

  function closeModal(): void {
    dispatch('close');
  }

  function handleBackdropClick(event: MouseEvent): void {
    if (event.target === event.currentTarget) {
      closeModal();
    }
  }

  function handleWindowKeydown(event: KeyboardEvent): void {
    if (!open || event.key !== 'Escape') {
      return;
    }

    event.preventDefault();
    closeModal();
  }

  function lockScroll(): void {
    if (!browser || scrollLocked) {
      return;
    }

    document.body.style.overflow = 'hidden';
    scrollLocked = true;
  }

  function unlockScroll(): void {
    if (!browser || !scrollLocked) {
      return;
    }

    document.body.style.overflow = '';
    scrollLocked = false;
  }

  onDestroy(() => {
    unlockScroll();
  });

  $: if (capture?.id !== previousCaptureId) {
    imageFailed = false;
    previousCaptureId = capture?.id ?? null;
  }

  $: if (open) {
    lockScroll();
  } else {
    unlockScroll();
  }

  $: screenshotSrc = capture && !imageFailed ? buildScreenshotSrc(capture) : null;
  $: appLabel = capture?.app_name?.trim() || 'Unknown app';
  $: capturedDate = capture ? new Date(capture.timestamp) : null;
  $: capturedLabel =
    capturedDate && Number.isFinite(capturedDate.getTime())
      ? timestampFormatter.format(capturedDate)
      : 'Timestamp unavailable';
  $: activityLabel = extraction?.activity_type?.trim() || 'unclassified';
  $: descriptionLabel = extraction?.description?.trim() || 'No extraction description available.';
  $: keyContentLabel = extraction?.key_content?.trim() || 'No key content extracted for this capture.';
  $: topics = extraction?.topics ?? [];
  $: extractionJson = extraction ? JSON.stringify(extraction, null, 2) : 'null';
</script>

<svelte:window on:keydown={handleWindowKeydown} />

{#if open && capture}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/40 backdrop-blur-sm"
    role="presentation"
    on:click={handleBackdropClick}
  >
    <div
      class="w-full max-w-[980px] max-h-[calc(100vh-3rem)] overflow-auto bg-surface-container-lowest rounded-[24px] shadow-2xl"
      role="dialog"
      aria-modal="true"
      aria-labelledby="capture-modal-title"
    >
      <header class="flex items-start justify-between p-6 pb-0">
        <div>
          <p class="text-xs font-bold text-primary uppercase tracking-widest mb-1">Capture Details</p>
          <h2 id="capture-modal-title" class="text-2xl font-semibold tracking-tight text-on-surface">{appLabel}</h2>
          <p class="text-sm text-secondary mt-1">{capturedLabel}</p>
        </div>
        <button
          type="button"
          on:click={closeModal}
          aria-label="Close capture details"
          class="p-2 rounded-xl hover:bg-surface-container-high transition-colors text-on-surface-variant hover:text-on-surface"
        >
          <span class="material-symbols-outlined">close</span>
        </button>
      </header>

      <div class="p-6">
        {#if screenshotSrc}
          <img
            class="w-full rounded-2xl bg-surface-container-high object-contain max-h-[48vh] mb-6"
            src={screenshotSrc}
            alt={`Screenshot for ${appLabel} at ${capturedLabel}`}
            loading="lazy"
            on:error={() => { imageFailed = true; }}
          />
        {:else}
          <div class="w-full rounded-2xl bg-surface-container-high flex items-center justify-center min-h-[220px] mb-6 text-on-surface-variant text-xs uppercase tracking-widest font-bold">
            Screenshot unavailable
          </div>
        {/if}

        <div class="grid grid-cols-1 md:grid-cols-2 gap-6">
          <div class="space-y-5">
            <div>
              <p class="text-[10px] font-bold text-secondary uppercase tracking-widest mb-1">Activity Type</p>
              <span class="inline-block px-3 py-1 bg-primary-fixed text-primary rounded-lg text-xs font-semibold">{activityLabel}</span>
            </div>
            <div>
              <p class="text-[10px] font-bold text-secondary uppercase tracking-widest mb-1">Description</p>
              <p class="text-sm text-on-surface-variant leading-relaxed">{descriptionLabel}</p>
            </div>
            <div>
              <p class="text-[10px] font-bold text-secondary uppercase tracking-widest mb-1">Key Content</p>
              <p class="text-sm text-on-surface-variant leading-relaxed">{keyContentLabel}</p>
            </div>
            <div>
              <p class="text-[10px] font-bold text-secondary uppercase tracking-widest mb-2">Topics</p>
              {#if topics.length > 0}
                <div class="flex flex-wrap gap-2">
                  {#each topics as topic (topic)}
                    <span class="px-3 py-1 bg-surface-container-low text-on-surface-variant rounded-full text-xs font-medium">{topic}</span>
                  {/each}
                </div>
              {:else}
                <p class="text-xs text-on-surface-variant">No topics extracted.</p>
              {/if}
            </div>
          </div>

          <div>
            <p class="text-[10px] font-bold text-secondary uppercase tracking-widest mb-2">Raw Extraction</p>
            <pre class="p-4 bg-surface-container-low rounded-2xl text-xs text-on-surface-variant leading-relaxed max-h-[320px] overflow-auto font-mono whitespace-pre-wrap break-words">{extractionJson}</pre>
          </div>
        </div>
      </div>
    </div>
  </div>
{/if}
