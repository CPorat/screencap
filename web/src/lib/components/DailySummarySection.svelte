<script lang="ts">
  type DailyProject = {
    name: string;
    totalMinutes: number;
    activities: string[];
    keyAccomplishments: string[];
  };

  type TimeAllocationEntry = {
    label: string;
    value: string;
  };

  type FocusBlock = {
    start: string;
    end: string;
    durationMinutes: number;
    project: string;
    quality: string;
    tint: string;
  };

  type DailySummary = {
    date: string;
    totalActiveHours: number | null;
    projectBreakdown: DailyProject[];
    timeAllocation: TimeAllocationEntry[];
    focusBlocks: FocusBlock[];
    openThreads: string[];
    narrative: string | null;
  };

  export let loading = false;
  export let summary: DailySummary | null = null;
  export let selectedDate = '';

  $: maxProjectMinutes =
    summary?.projectBreakdown.reduce((max, project) => Math.max(max, project.totalMinutes), 0) ?? 0;

  $: focusBlockTotal =
    summary?.focusBlocks.reduce((total, block) => total + Math.max(block.durationMinutes, 0), 0) ?? 0;

  function projectWidth(totalMinutes: number): number {
    if (maxProjectMinutes <= 0) {
      return 0;
    }

    return Math.max(10, Math.round((totalMinutes / maxProjectMinutes) * 100));
  }

  function focusWidth(durationMinutes: number): number {
    if (focusBlockTotal <= 0) {
      return 0;
    }

    return Math.max(7, Math.round((durationMinutes / focusBlockTotal) * 100));
  }
</script>

<section class="card" aria-busy={loading}>
  <header class="card__header">
    <p class="card__eyebrow">Daily summary</p>
    <h3>{summary?.date ?? selectedDate}</h3>
  </header>

  {#if loading}
    <p class="card__state">Compiling daily summary…</p>
  {:else if !summary}
    <p class="card__state">No summary exists for this day.</p>
  {:else}
    <div class="topline">
      <p>
        Active time: <strong>{summary.totalActiveHours === null ? '—' : `${summary.totalActiveHours.toFixed(1)}h`}</strong>
      </p>
      <p>
        Focus blocks: <strong>{summary.focusBlocks.length}</strong>
      </p>
    </div>

    <section>
      <p class="card__eyebrow">Project breakdown</p>
      {#if summary.projectBreakdown.length === 0}
        <p class="card__state">No project breakdown generated.</p>
      {:else}
        <ul class="projects">
          {#each summary.projectBreakdown as project}
            <li>
              <div class="projects__top">
                <strong>{project.name}</strong>
                <span>{project.totalMinutes}m</span>
              </div>
              <div class="projects__track" role="presentation">
                <div class="projects__fill" style={`width:${projectWidth(project.totalMinutes)}%`}></div>
              </div>
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <section>
      <p class="card__eyebrow">Time allocation</p>
      {#if summary.timeAllocation.length === 0}
        <p class="card__state">No time allocation available.</p>
      {:else}
        <ul class="allocation">
          {#each summary.timeAllocation as allocation}
            <li>
              <span>{allocation.label}</span>
              <strong>{allocation.value}</strong>
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <section>
      <p class="card__eyebrow">Focus blocks</p>
      {#if summary.focusBlocks.length === 0}
        <p class="card__state">No deep-focus periods detected.</p>
      {:else}
        <div class="focus-track" aria-label="Focus blocks timeline">
          {#each summary.focusBlocks as block}
            <div
              class="focus-track__segment"
              style={`--segment-tint:${block.tint};width:${focusWidth(block.durationMinutes)}%`}
              title={`${block.project} · ${block.quality} · ${block.start}–${block.end}`}
            >
              <span>{block.project}</span>
            </div>
          {/each}
        </div>
      {/if}
    </section>

    <section>
      <p class="card__eyebrow">Open threads</p>
      {#if summary.openThreads.length === 0}
        <p class="card__state">No open threads captured.</p>
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
</section>

<style>
  .card {
    border: 1px solid color-mix(in srgb, var(--paper-100) 28%, transparent);
    border-radius: var(--radius-card);
    padding: 1rem;
    background:
      linear-gradient(152deg, rgb(9 12 21 / 92%), rgb(24 16 34 / 80%)),
      radial-gradient(circle at 90% 4%, rgb(255 179 71 / 20%), transparent 40%);
    display: grid;
    gap: 0.85rem;
  }

  .card__header {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 0.8rem;
  }

  .card__eyebrow {
    margin: 0;
    font-size: 0.67rem;
    letter-spacing: 0.17em;
    text-transform: uppercase;
    color: var(--pulse);
  }

  h3 {
    font-size: clamp(1.2rem, 2.3vw, 1.8rem);
  }

  .card__state,
  .narrative {
    color: var(--paper-200);
    font-size: 0.84rem;
  }

  .topline {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.52rem;
    font-size: 0.8rem;
  }

  .topline p {
    margin: 0;
    border: 1px solid rgb(246 241 231 / 22%);
    border-radius: 0.6rem;
    padding: 0.44rem 0.56rem;
    background: rgb(8 12 18 / 65%);
    color: var(--paper-200);
  }

  .topline strong {
    color: var(--ember);
    font-family: var(--display-font);
    font-size: 0.78rem;
  }

  section {
    display: grid;
    gap: 0.45rem;
  }

  .projects,
  .allocation,
  .threads {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.38rem;
  }

  .projects li,
  .allocation li,
  .threads li {
    border: 1px solid rgb(246 241 231 / 18%);
    border-radius: 0.63rem;
    padding: 0.45rem 0.55rem;
    background: rgb(8 11 18 / 58%);
    font-size: 0.8rem;
  }

  .projects__top,
  .allocation li {
    display: flex;
    justify-content: space-between;
    gap: 0.6rem;
    align-items: baseline;
  }

  .projects__track {
    margin-top: 0.24rem;
    height: 0.42rem;
    border-radius: 999px;
    background: rgb(246 241 231 / 15%);
    overflow: hidden;
  }

  .projects__fill {
    height: 100%;
    border-radius: inherit;
    background: linear-gradient(90deg, var(--pulse), var(--surge));
  }

  .allocation strong {
    color: var(--ember);
    font-family: var(--display-font);
    font-size: 0.78rem;
  }

  .focus-track {
    display: flex;
    gap: 0.3rem;
    overflow: hidden;
  }

  .focus-track__segment {
    min-width: 3.4rem;
    border-radius: 0.55rem;
    border: 1px solid color-mix(in srgb, var(--segment-tint) 70%, black 30%);
    background: linear-gradient(120deg, color-mix(in srgb, var(--segment-tint) 78%, transparent 22%), rgb(8 10 16 / 82%));
    display: grid;
    place-items: center;
    padding: 0.32rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 0.62rem;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .narrative {
    margin: 0;
    border-left: 3px solid var(--ember);
    padding-left: 0.62rem;
    line-height: 1.45;
  }

  @media (width <= 760px) {
    .topline {
      grid-template-columns: 1fr;
    }
  }
</style>
