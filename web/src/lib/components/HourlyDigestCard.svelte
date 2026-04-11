<script lang="ts">
  type HourlyProject = {
    name: string;
    minutes: number;
    activities: string[];
  };

  type HourlyDigest = {
    id: number;
    label: string;
    dominantActivity: string;
    focusScoreLabel: string | null;
    projects: HourlyProject[];
    topics: string[];
    keyMoments: string[];
    narrative: string | null;
    hourStart: string | null;
    hourEnd: string | null;
  };

  export let digest: HourlyDigest;

  let expanded = false;

  function toggleExpanded(): void {
    expanded = !expanded;
  }
</script>

<article class="card">
  <header class="card__header">
    <div>
      <p class="card__eyebrow">Hourly digest</p>
      <h4>{digest.label}</h4>
    </div>

    <button type="button" class="card__toggle" on:click={toggleExpanded} aria-expanded={expanded}>
      {expanded ? 'Collapse' : 'Expand'}
    </button>
  </header>

  <div class="card__meta">
    <p>{digest.dominantActivity}</p>
    {#if digest.focusScoreLabel}
      <span>{digest.focusScoreLabel} focus</span>
    {/if}
  </div>

  {#if expanded}
    <div class="card__body">
      <section>
        <p class="card__eyebrow">Projects</p>
        {#if digest.projects.length === 0}
          <p class="card__state">No project data in this digest.</p>
        {:else}
          <ul class="list">
            {#each digest.projects as project}
              <li>
                <div class="list__top">
                  <strong>{project.name}</strong>
                  <span>{project.minutes}m</span>
                </div>
                {#if project.activities.length > 0}
                  <p>{project.activities.join(' · ')}</p>
                {/if}
              </li>
            {/each}
          </ul>
        {/if}
      </section>

      <section>
        <p class="card__eyebrow">Topics</p>
        {#if digest.topics.length === 0}
          <p class="card__state">No topics surfaced.</p>
        {:else}
          <div class="chips">
            {#each digest.topics as topic}
              <span>{topic}</span>
            {/each}
          </div>
        {/if}
      </section>

      <section>
        <p class="card__eyebrow">Key moments</p>
        {#if digest.keyMoments.length === 0}
          <p class="card__state">No key moments recorded.</p>
        {:else}
          <ul class="moments">
            {#each digest.keyMoments as moment}
              <li>{moment}</li>
            {/each}
          </ul>
        {/if}
      </section>

      {#if digest.narrative}
        <p class="narrative">{digest.narrative}</p>
      {/if}
    </div>
  {/if}
</article>

<style>
  .card {
    border: 1px solid color-mix(in srgb, var(--paper-100) 24%, transparent);
    border-radius: 0.95rem;
    background: linear-gradient(156deg, rgb(9 13 22 / 92%), rgb(18 12 29 / 78%));
    padding: 0.9rem;
    display: grid;
    gap: 0.75rem;
  }

  .card__header {
    display: flex;
    justify-content: space-between;
    align-items: start;
    gap: 0.7rem;
  }

  .card__eyebrow {
    margin: 0;
    font-size: 0.66rem;
    letter-spacing: 0.16em;
    text-transform: uppercase;
    color: var(--paper-200);
  }

  h4 {
    font-size: clamp(1rem, 2vw, 1.35rem);
  }

  .card__toggle {
    border: 1px solid rgb(246 241 231 / 35%);
    border-radius: 999px;
    background: transparent;
    color: var(--paper-100);
    padding: 0.32rem 0.65rem;
    font-size: 0.68rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    cursor: pointer;
  }

  .card__toggle:hover {
    border-color: var(--pulse);
    color: var(--pulse);
  }

  .card__meta {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.5rem;
    color: var(--paper-200);
    font-size: 0.82rem;
  }

  .card__meta span {
    color: var(--ember);
    font-family: var(--display-font);
    font-size: 0.73rem;
    letter-spacing: 0.08em;
  }

  .card__body {
    display: grid;
    gap: 0.78rem;
  }

  section {
    display: grid;
    gap: 0.45rem;
  }

  .card__state,
  .narrative {
    margin: 0;
    color: var(--paper-200);
    font-size: 0.82rem;
  }

  .list,
  .moments {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.4rem;
  }

  .list li,
  .moments li {
    border: 1px solid rgb(246 241 231 / 18%);
    border-radius: 0.65rem;
    padding: 0.45rem 0.54rem;
    background: rgb(8 12 19 / 60%);
    font-size: 0.8rem;
  }

  .list__top {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 0.6rem;
    margin-bottom: 0.2rem;
  }

  .list__top strong {
    font-size: 0.83rem;
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 0.34rem;
  }

  .chips span {
    border-radius: 999px;
    border: 1px solid rgb(112 255 227 / 38%);
    background: rgb(112 255 227 / 11%);
    padding: 0.16rem 0.48rem;
    font-size: 0.74rem;
  }

  .narrative {
    border-left: 3px solid var(--surge);
    padding-left: 0.58rem;
    line-height: 1.45;
  }
</style>
