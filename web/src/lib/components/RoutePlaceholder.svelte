<script lang="ts">
  export let section = '';
  export let title = '';
  export let summary = '';
  export let highlights: string[] = [];
  export let endpointHint = '';
</script>

<section class="placeholder" aria-labelledby="route-title">
  <p class="placeholder__section">{section}</p>
  <h2 id="route-title">{title}</h2>
  <p class="placeholder__summary">{summary}</p>

  <ul class="placeholder__highlights" aria-label={`${section} roadmap highlights`}>
    {#each highlights as highlight}
      <li>{highlight}</li>
    {/each}
  </ul>

  <p class="placeholder__endpoint">Planned API endpoint: <code>{endpointHint}</code></p>
</section>

<style>
  .placeholder {
    height: 100%;
    padding: clamp(1.2rem, 2.6vw, 2rem);
    display: grid;
    align-content: start;
    gap: 1rem;
    background:
      linear-gradient(150deg, rgb(33 37 51 / 94%), rgb(18 21 32 / 98%)),
      radial-gradient(circle at 82% 12%, rgb(255 78 166 / 12%), transparent 38%);
    animation: reveal 520ms cubic-bezier(0.2, 0.9, 0.2, 1);
  }

  @keyframes reveal {
    from {
      opacity: 0;
      transform: translateY(0.75rem);
    }

    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  .placeholder__section {
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.22em;
    color: var(--pulse);
  }

  h2 {
    font-size: clamp(1.95rem, 4.2vw, 3.25rem);
  }

  .placeholder__summary {
    max-width: 65ch;
    color: var(--paper-200);
    font-size: 0.92rem;
  }

  .placeholder__highlights {
    margin: 0.2rem 0 0;
    padding: 0;
    list-style: none;
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 0.72rem;
  }

  .placeholder__highlights li {
    border: 2px solid rgb(246 241 231 / 38%);
    border-radius: 0.85rem;
    background: linear-gradient(165deg, rgb(79 88 121 / 42%), rgb(28 32 45 / 88%));
    padding: 0.78rem 0.84rem;
    font-size: 0.84rem;
  }

  .placeholder__endpoint {
    margin-top: 0.2rem;
    font-size: 0.8rem;
    color: var(--paper-200);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  code {
    font-family: var(--body-font);
    color: var(--ember);
  }
</style>
