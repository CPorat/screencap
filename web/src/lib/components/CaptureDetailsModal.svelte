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
  $: capturedLabel =
    capture && Number.isFinite(new Date(capture.timestamp).getTime())
      ? timestampFormatter.format(new Date(capture.timestamp))
      : 'Timestamp unavailable';
  $: activityLabel = extraction?.activity_type?.trim() || 'unclassified';
  $: descriptionLabel = extraction?.description?.trim() || 'No extraction description available.';
  $: keyContentLabel = extraction?.key_content?.trim() || 'No key content extracted for this capture.';
  $: topics = extraction?.topics ?? [];
  $: extractionJson = extraction ? JSON.stringify(extraction, null, 2) : 'null';
</script>

<svelte:window on:keydown={handleWindowKeydown} />

{#if open && capture}
  <div class="modal-backdrop" role="presentation" on:click={handleBackdropClick}>
    <div class="modal" role="dialog" aria-modal="true" aria-labelledby="capture-modal-title">
      <header class="modal__header">
        <div>
          <p class="modal__eyebrow">Capture Details</p>
          <h2 id="capture-modal-title">{appLabel}</h2>
          <p class="modal__timestamp">{capturedLabel}</p>
        </div>
        <button class="modal__close" type="button" on:click={closeModal} aria-label="Close capture details">
          Close
        </button>
      </header>

      {#if screenshotSrc}
        <img
          class="modal__screenshot"
          src={screenshotSrc}
          alt={`Screenshot for ${appLabel} at ${capturedLabel}`}
          loading="lazy"
          on:error={() => {
            imageFailed = true;
          }}
        />
      {:else}
        <div class="modal__screenshot modal__screenshot--fallback" role="img" aria-label="Screenshot unavailable">
          Screenshot unavailable
        </div>
      {/if}

      <div class="modal__content">
        <section class="modal__section">
          <h3>Summary</h3>
          <dl class="modal__facts">
            <div>
              <dt>Activity type</dt>
              <dd>{activityLabel}</dd>
            </div>
            <div>
              <dt>Description</dt>
              <dd>{descriptionLabel}</dd>
            </div>
            <div>
              <dt>Key content</dt>
              <dd>{keyContentLabel}</dd>
            </div>
          </dl>

          <h4>Topics</h4>
          {#if topics.length > 0}
            <div class="modal__topics" aria-label="Capture topics">
              {#each topics as topic (topic)}
                <span>{topic}</span>
              {/each}
            </div>
          {:else}
            <p class="modal__muted">No topics extracted.</p>
          {/if}
        </section>

        <section class="modal__section">
          <h3>Full extraction payload</h3>
          <pre>{extractionJson}</pre>
        </section>
      </div>
    </div>
  </div>
{/if}

<style>
  .modal-backdrop {
    position: fixed;
    inset: 0;
    z-index: 40;
    display: grid;
    place-items: center;
    padding: clamp(0.8rem, 2vw, 1.3rem);
    background: rgb(4 6 10 / 72%);
    backdrop-filter: blur(8px);
  }

  .modal {
    width: min(980px, 100%);
    max-height: calc(100vh - 2.4rem);
    overflow: auto;
    border: 2px solid rgb(246 241 231 / 36%);
    border-radius: 1rem;
    background:
      linear-gradient(150deg, rgb(25 31 46 / 96%), rgb(11 14 24 / 98%)),
      radial-gradient(circle at 12% 0%, rgb(255 78 166 / 18%), transparent 40%);
    box-shadow: 0.6rem 0.6rem 0 rgb(8 10 16 / 95%);
    display: grid;
    gap: 0.85rem;
    padding: clamp(0.9rem, 2.2vw, 1.2rem);
  }

  .modal__header {
    display: flex;
    justify-content: space-between;
    gap: 0.8rem;
    align-items: flex-start;
  }

  .modal__eyebrow {
    font-size: 0.68rem;
    text-transform: uppercase;
    letter-spacing: 0.2em;
    color: var(--pulse);
  }

  h2 {
    font-size: clamp(1.25rem, 2.8vw, 2rem);
    margin-top: 0.2rem;
  }

  .modal__timestamp {
    margin-top: 0.16rem;
    color: var(--paper-200);
    font-size: 0.78rem;
    text-transform: uppercase;
    letter-spacing: 0.1em;
  }

  .modal__close {
    border: 1px solid rgb(246 241 231 / 42%);
    border-radius: 999px;
    background: rgb(12 16 24 / 90%);
    color: var(--paper-100);
    padding: 0.34rem 0.74rem;
    font: inherit;
    font-size: 0.7rem;
    letter-spacing: 0.09em;
    text-transform: uppercase;
    cursor: pointer;
  }

  .modal__close:hover,
  .modal__close:focus-visible {
    border-color: var(--pulse);
    color: var(--pulse);
    outline: none;
  }

  .modal__screenshot {
    width: 100%;
    border-radius: 0.8rem;
    border: 1px solid rgb(246 241 231 / 24%);
    background: rgb(8 11 19 / 90%);
    object-fit: contain;
    max-height: 48vh;
  }

  .modal__screenshot--fallback {
    display: grid;
    place-content: center;
    min-height: 220px;
    font-size: 0.74rem;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: rgb(246 241 231 / 72%);
  }

  .modal__content {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: 0.75rem;
  }

  .modal__section {
    border: 1px solid rgb(246 241 231 / 24%);
    border-radius: 0.8rem;
    background: rgb(7 10 18 / 72%);
    padding: 0.72rem;
    display: grid;
    gap: 0.52rem;
    align-content: start;
  }

  h3,
  h4 {
    font-size: 0.88rem;
    letter-spacing: 0.08em;
  }

  .modal__facts {
    margin: 0;
    display: grid;
    gap: 0.46rem;
  }

  .modal__facts div {
    display: grid;
    gap: 0.16rem;
  }

  .modal__facts dt {
    font-size: 0.65rem;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--paper-200);
  }

  .modal__facts dd {
    margin: 0;
    font-size: 0.84rem;
    line-height: 1.35;
    color: var(--paper-100);
  }

  .modal__topics {
    display: flex;
    flex-wrap: wrap;
    gap: 0.38rem;
  }

  .modal__topics span {
    border: 1px solid rgb(112 255 227 / 44%);
    border-radius: 999px;
    padding: 0.16rem 0.52rem;
    font-size: 0.67rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--pulse);
    background: rgb(4 14 16 / 45%);
  }

  .modal__muted {
    color: var(--paper-200);
    font-size: 0.78rem;
  }

  pre {
    margin: 0;
    border-radius: 0.68rem;
    border: 1px solid rgb(246 241 231 / 18%);
    background: rgb(4 7 13 / 94%);
    color: rgb(231 244 255 / 92%);
    padding: 0.62rem;
    font-size: 0.72rem;
    line-height: 1.35;
    max-height: 280px;
    overflow: auto;
    white-space: pre-wrap;
    word-break: break-word;
  }

  @media (max-width: 860px) {
    .modal__content {
      grid-template-columns: 1fr;
    }
  }
</style>
