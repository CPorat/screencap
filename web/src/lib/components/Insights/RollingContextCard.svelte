<script lang="ts">
  import type { RollingContextView } from './types';

  export let loading = false;
  export let context: RollingContextView | null = null;
</script>

<article class="card" aria-busy={loading}>
  <header>
    <p class="eyebrow">Rolling context</p>
    <h3>What am I doing right now?</h3>
  </header>

  {#if loading}
    <p class="state">Reading the latest 30-minute synthesis…</p>
  {:else if !context}
    <p class="state">No rolling context available yet.</p>
  {:else}
    <div class="focus">
      <p class="focus__label">Current focus</p>
      <p class="focus__value">{context.currentFocus}</p>
      {#if context.activeProject}
        <p class="focus__project">Project · {context.activeProject}</p>
      {/if}
    </div>

    <div class="meta-grid">
      <div>
        <p class="meta-label">Mood</p>
        <p class="meta-value">{context.mood ?? 'Unspecified'}</p>
      </div>

      <div>
        <p class="meta-label">Apps in rotation</p>
        {#if context.appsUsed.length === 0}
          <p class="meta-value">No app context in this window.</p>
        {:else}
          <ul>
            {#each context.appsUsed as app}
              <li>
                <span>{app.name}</span>
                <strong>{app.share}</strong>
              </li>
            {/each}
          </ul>
        {/if}
      </div>
    </div>

    {#if context.summary}
      <p class="summary">{context.summary}</p>
    {/if}
  {/if}
</article>

<style>
  .card {
    border: 1px solid rgb(246 241 231 / 36%);
    border-radius: 1rem;
    background:
      linear-gradient(145deg, rgb(10 15 24 / 88%), rgb(20 14 30 / 84%)),
      radial-gradient(circle at 10% 10%, rgb(112 255 227 / 18%), transparent 30%);
    padding: 1rem;
    display: grid;
    gap: 0.95rem;
    box-shadow: 0 0.5rem 1.2rem rgb(0 0 0 / 28%);
    animation: rise 300ms ease both;
  }

  header {
    display: grid;
    gap: 0.45rem;
  }

  .eyebrow,
  .meta-label {
    margin: 0;
    font-size: 0.69rem;
    text-transform: uppercase;
    letter-spacing: 0.18em;
    color: var(--pulse);
  }

  h3 {
    font-size: clamp(1.3rem, 2.3vw, 1.9rem);
  }

  .state,
  .summary,
  .meta-value,
  .focus__project {
    color: var(--paper-200);
    font-size: 0.88rem;
  }

  .focus {
    padding: 0.85rem;
    border-radius: 0.86rem;
    border: 1px solid rgb(112 255 227 / 44%);
    background: linear-gradient(132deg, rgb(9 15 23 / 92%), rgb(11 11 18 / 75%));
    display: grid;
    gap: 0.34rem;
  }

  .focus__label {
    margin: 0;
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.14em;
    color: var(--paper-200);
  }

  .focus__value {
    margin: 0;
    font-family: var(--display-font);
    font-size: clamp(1.18rem, 2.4vw, 1.9rem);
    line-height: 1.06;
  }

  .focus__project {
    margin: 0;
  }

  .meta-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.7rem;
  }

  ul {
    margin: 0.2rem 0 0;
    padding: 0;
    list-style: none;
    display: grid;
    gap: 0.3rem;
  }

  li {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 0.6rem;
    border-bottom: 1px solid rgb(246 241 231 / 18%);
    padding-bottom: 0.2rem;
    font-size: 0.84rem;
  }

  strong {
    font-family: var(--display-font);
    font-size: 0.82rem;
    color: var(--ember);
  }

  .summary {
    margin: 0;
    border-left: 3px solid var(--surge);
    padding-left: 0.65rem;
    line-height: 1.5;
  }

  @keyframes rise {
    from {
      transform: translateY(6px);
      opacity: 0;
    }

    to {
      transform: translateY(0);
      opacity: 1;
    }
  }

  @media (width <= 760px) {
    .meta-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
