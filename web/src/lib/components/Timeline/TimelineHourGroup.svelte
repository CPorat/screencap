<script lang="ts">
  import TimelineCaptureCard from './TimelineCaptureCard.svelte';
  import type { TimelineHourBucket } from './types';

  export let bucket: TimelineHourBucket;
</script>

<section class="hour-block" aria-label={`Captures for ${bucket.heading}`}>
  <header class="hour-block__header">
    <h3>{bucket.heading}</h3>
    <p>{bucket.rangeLabel} · {bucket.captures.length} capture{bucket.captures.length === 1 ? '' : 's'}</p>
  </header>

  <div class="hour-block__grid">
    {#each bucket.captures as item (item.capture.id)}
      <TimelineCaptureCard {item} />
    {/each}
  </div>
</section>

<style>
  .hour-block {
    display: grid;
    gap: 0.8rem;
  }

  .hour-block__header {
    position: sticky;
    top: 0;
    z-index: 1;
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 0.8rem;
    padding: 0.45rem 0.65rem;
    border: 1px solid rgb(246 241 231 / 24%);
    border-radius: 0.7rem;
    background: linear-gradient(90deg, rgb(16 20 30 / 95%), rgb(17 17 24 / 90%));
    backdrop-filter: blur(8px);
  }

  h3 {
    font-size: clamp(1rem, 2.2vw, 1.35rem);
  }

  .hour-block__header p {
    color: var(--paper-200);
    font-size: 0.69rem;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    text-align: right;
  }

  .hour-block__grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(270px, 1fr));
    gap: 0.85rem;
    align-content: start;
  }
</style>
