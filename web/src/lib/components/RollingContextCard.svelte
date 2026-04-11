<script lang="ts">
  type AppUsage = {
    name: string;
    share: string;
  };

  type RollingContext = {
    currentFocus: string;
    activeProject: string | null;
    appsUsed: AppUsage[];
  };

  export let loading = false;
  export let context: RollingContext | null = null;
</script>

<article class="card" aria-busy={loading}>
  <header class="card__header">
    <p class="card__eyebrow">Rolling context</p>
    <h3>Current trajectory</h3>
  </header>

  {#if loading}
    <p class="card__state">Loading current focus…</p>
  {:else if !context}
    <p class="card__state">No rolling context available right now.</p>
  {:else}
    <section class="focus">
      <p class="focus__label">Current focus</p>
      <p class="focus__value">{context.currentFocus}</p>
      <p class="focus__project">
        Active project: {context.activeProject ?? 'Unassigned'}
      </p>
    </section>

    <section>
      <p class="card__eyebrow">Apps used</p>
      {#if context.appsUsed.length === 0}
        <p class="card__state">No app activity for this window.</p>
      {:else}
        <ul class="apps">
          {#each context.appsUsed as app}
            <li>
              <span>{app.name}</span>
              <strong>{app.share}</strong>
            </li>
          {/each}
        </ul>
      {/if}
    </section>
  {/if}
</article>

<style>
  .card {
    border: 1px solid color-mix(in srgb, var(--paper-100) 28%, transparent);
    border-radius: var(--radius-card);
    background:
      linear-gradient(144deg, rgb(12 17 28 / 92%), rgb(20 14 31 / 84%)),
      radial-gradient(circle at 6% 16%, rgb(112 255 227 / 22%), transparent 35%);
    padding: 1rem;
    display: grid;
    gap: 0.9rem;
  }

  .card__header {
    display: grid;
    gap: 0.4rem;
  }

  .card__eyebrow,
  .focus__label {
    margin: 0;
    font-size: 0.68rem;
    letter-spacing: 0.18em;
    text-transform: uppercase;
    color: var(--pulse);
  }

  h3 {
    font-size: clamp(1.2rem, 2.4vw, 1.9rem);
  }

  .card__state,
  .focus__project {
    color: var(--paper-200);
    font-size: 0.86rem;
  }

  .focus {
    border: 1px solid rgb(112 255 227 / 40%);
    border-radius: 0.82rem;
    padding: 0.72rem;
    display: grid;
    gap: 0.3rem;
    background: rgb(7 12 20 / 62%);
  }

  .focus__value {
    margin: 0;
    font-family: var(--display-font);
    font-size: clamp(1.15rem, 2.3vw, 1.75rem);
    line-height: 1.05;
  }

  .apps {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.3rem;
  }

  .apps li {
    display: flex;
    justify-content: space-between;
    gap: 0.7rem;
    border-bottom: 1px solid rgb(246 241 231 / 16%);
    padding-bottom: 0.22rem;
    font-size: 0.82rem;
  }

  .apps strong {
    color: var(--ember);
    font-family: var(--display-font);
    font-size: 0.8rem;
  }
</style>
