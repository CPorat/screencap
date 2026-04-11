<script lang="ts">
  export let compact = false;

  $: count = compact ? 2 : 6;
  $: placeholders = Array.from({ length: count }, (_, index) => index);
</script>

<div class="skeleton" data-compact={compact} aria-hidden="true">
  {#each placeholders as placeholder (placeholder)}
    <article class="skeleton__card">
      <div class="skeleton__meta"></div>
      <div class="skeleton__line skeleton__line--short"></div>
      <div class="skeleton__line"></div>
      <div class="skeleton__line"></div>
      <div class="skeleton__image"></div>
      <div class="skeleton__line skeleton__line--button"></div>
    </article>
  {/each}
</div>

<style>
  .skeleton {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 0.85rem;
  }

  .skeleton[data-compact='true'] {
    opacity: 0.72;
  }

  .skeleton__card {
    display: grid;
    gap: 0.58rem;
    border-radius: 0.95rem;
    border: 1px solid rgb(246 241 231 / 30%);
    padding: 0.85rem;
    background: rgb(14 16 24 / 92%);
    overflow: hidden;
  }

  .skeleton__card::after {
    content: '';
    position: absolute;
    inset: 0;
    translate: -120% 0;
    background: linear-gradient(110deg, transparent, rgb(255 255 255 / 12%), transparent);
    animation: shimmer 1.55s linear infinite;
  }

  .skeleton__card {
    position: relative;
  }

  .skeleton__meta,
  .skeleton__line,
  .skeleton__image {
    border-radius: 0.52rem;
    background: linear-gradient(90deg, rgb(58 64 82 / 72%), rgb(83 92 119 / 78%), rgb(58 64 82 / 72%));
    background-size: 200% 100%;
    animation: pulse 1.4s ease infinite;
  }

  .skeleton__meta {
    height: 0.95rem;
    width: 48%;
  }

  .skeleton__line {
    height: 0.68rem;
  }

  .skeleton__line--short {
    width: 66%;
  }

  .skeleton__line--button {
    width: 44%;
    height: 0.82rem;
  }

  .skeleton__image {
    height: 0;
    padding-bottom: 62%;
  }

  @keyframes pulse {
    0%,
    100% {
      background-position: 0% 0;
    }

    50% {
      background-position: 100% 0;
    }
  }

  @keyframes shimmer {
    to {
      translate: 120% 0;
    }
  }
</style>
