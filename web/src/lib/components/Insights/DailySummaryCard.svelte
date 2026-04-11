<script lang="ts">
  import type { DailySummaryView } from './types';

  export let loading = false;
  export let summary: DailySummaryView | null = null;
  export let selectedDate = '';

  $: strongestProjectMinutes =
    summary?.projects.reduce((max, project) => Math.max(max, project.minutes), 0) ?? 0;

  function segmentWidth(minutes: number): number {
    if (!summary || summary.totalMinutes <= 0) {
      return 0;
    }

    return Math.max(6, Math.round((minutes / summary.totalMinutes) * 100));
  }

  function projectWidth(minutes: number): number {
    if (strongestProjectMinutes <= 0) {
      return 0;
    }

    return Math.max(8, Math.round((minutes / strongestProjectMinutes) * 100));
  }
</script>

<article class="card" aria-busy={loading}>
  <header>
    <p class="eyebrow">Daily synthesis</p>
    <h3>{selectedDate}</h3>
  </header>

  {#if loading}
    <p class="state">Compiling today’s synthesized activity…</p>
  {:else if !summary}
    <p class="state">No summary exists for this date.</p>
  {:else}
    <div class="kpis">
      <div class="kpi">
        <p>Total active time</p>
        <strong>{summary.totalLabel}</strong>
      </div>
      <div class="kpi">
        <p>Focus score</p>
        <strong>{summary.focusScoreLabel}</strong>
      </div>
    </div>

    <section>
      <div class="section__heading">
        <h4>Project breakdown</h4>
      </div>

      {#if summary.projects.length === 0}
        <p class="state">No project allocation available.</p>
      {:else}
        <ul class="projects">
          {#each summary.projects as project}
            <li>
              <div class="projects__label">
                <span>{project.name}</span>
                <strong>{project.durationLabel}</strong>
              </div>
              <div class="projects__track" role="presentation">
                <div class="projects__bar" style={`width: ${projectWidth(project.minutes)}%`}></div>
              </div>
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <section>
      <div class="section__heading">
        <h4>Key moments</h4>
      </div>

      {#if summary.keyMoments.length === 0}
        <p class="state">No key moments captured for this date.</p>
      {:else}
        <ul class="moments">
          {#each summary.keyMoments as moment}
            <li>{moment}</li>
          {/each}
        </ul>
      {/if}
    </section>

    <section>
      <div class="section__heading">
        <h4>Focus blocks</h4>
      </div>

      {#if summary.focusBlocks.length === 0}
        <p class="state">No focus blocks generated.</p>
      {:else}
        <div class="focus-track" aria-label="Focus block timeline">
          {#each summary.focusBlocks as block}
            <div
              class="focus-segment"
              style={`width:${segmentWidth(block.minutes)}%; --focus-tint:${block.tint};`}
              title={`${block.project} · ${block.label} · ${block.quality}`}
            >
              <span>{block.project}</span>
            </div>
          {/each}
        </div>
      {/if}
    </section>

    <section>
      <div class="section__heading">
        <h4>Open threads</h4>
      </div>

      {#if summary.openThreads.length === 0}
        <p class="state">No open threads tracked.</p>
      {:else}
        <ul class="threads">
          {#each summary.openThreads as thread}
            <li>{thread}</li>
          {/each}
        </ul>
      {/if}
    </section>

    {#if summary.narrative}
      <p class="narrative">{summary.narrative}</p>
    {/if}
  {/if}
</article>

<style>
  .card {
    border: 1px solid rgb(246 241 231 / 36%);
    border-radius: 1rem;
    background:
      linear-gradient(152deg, rgb(9 12 20 / 92%), rgb(22 17 32 / 82%)),
      radial-gradient(circle at 92% 2%, rgb(255 179 71 / 16%), transparent 38%);
    padding: 1rem;
    display: grid;
    gap: 1rem;
    align-content: start;
    animation: rise 330ms ease both;
  }

  header {
    display: grid;
    gap: 0.42rem;
  }

  .eyebrow,
  .kpi p,
  .section__heading h4 {
    margin: 0;
    font-size: 0.69rem;
    letter-spacing: 0.17em;
    text-transform: uppercase;
    color: var(--paper-200);
  }

  h3 {
    font-size: clamp(1.32rem, 2.4vw, 2rem);
  }

  .state,
  .narrative {
    margin: 0;
    color: var(--paper-200);
    font-size: 0.88rem;
  }

  .kpis {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.68rem;
  }

  .kpi {
    border: 1px solid rgb(246 241 231 / 30%);
    border-radius: 0.78rem;
    background: rgb(8 11 18 / 66%);
    padding: 0.62rem;
    display: grid;
    gap: 0.2rem;
  }

  .kpi strong {
    font-family: var(--display-font);
    font-size: clamp(1.08rem, 2vw, 1.5rem);
    line-height: 1;
  }

  section {
    display: grid;
    gap: 0.55rem;
  }

  .section__heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .section__heading h4 {
    color: var(--pulse);
  }

  .projects,
  .moments,
  .threads {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.46rem;
  }

  .projects li {
    display: grid;
    gap: 0.24rem;
  }

  .projects__label {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 0.55rem;
    font-size: 0.82rem;
  }

  .projects__label strong {
    font-family: var(--display-font);
    color: var(--ember);
    font-size: 0.84rem;
  }

  .projects__track {
    height: 0.45rem;
    border-radius: 999px;
    background: rgb(246 241 231 / 16%);
    overflow: hidden;
  }

  .projects__bar {
    height: 100%;
    border-radius: inherit;
    background: linear-gradient(90deg, rgb(112 255 227 / 88%), rgb(255 78 166 / 84%));
    transition: width 260ms ease;
  }

  .moments li,
  .threads li {
    border: 1px solid rgb(246 241 231 / 20%);
    border-radius: 0.66rem;
    padding: 0.47rem 0.6rem;
    background: rgb(8 11 18 / 58%);
    font-size: 0.83rem;
    line-height: 1.4;
  }

  .focus-track {
    display: flex;
    gap: 0.35rem;
    align-items: stretch;
    overflow: hidden;
  }

  .focus-segment {
    min-width: 3.5rem;
    border-radius: 0.6rem;
    border: 1px solid color-mix(in srgb, var(--focus-tint) 75%, black 25%);
    background: linear-gradient(130deg, color-mix(in srgb, var(--focus-tint) 75%, transparent 25%), rgb(8 9 15 / 85%));
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0.38rem 0.26rem;
    font-size: 0.66rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .focus-segment span {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .narrative {
    border-left: 3px solid var(--ember);
    padding-left: 0.62rem;
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
    .kpis {
      grid-template-columns: 1fr;
    }
  }
</style>
