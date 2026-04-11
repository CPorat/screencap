<script lang="ts">
  import { onMount } from 'svelte';
  import {
    api,
    type DailyInsight,
    type FocusBlock,
    type HourlyInsight,
    type RollingInsight,
  } from '../lib/api';

  interface TimeAllocationRow {
    key: string;
    value: string;
    minutes: number;
    share: number;
  }

  interface FocusBlockVisual extends FocusBlock {
    share: number;
    tone: 'deep' | 'steady' | 'light' | 'ambient';
  }

  const dateLabelFormatter = new Intl.DateTimeFormat(undefined, {
    weekday: 'long',
    month: 'short',
    day: 'numeric',
  });

  const hourFormatter = new Intl.DateTimeFormat(undefined, {
    hour: '2-digit',
    minute: '2-digit',
    hour12: false,
  });

  let selectedDate = toInputDate(new Date());

  let currentInsight: RollingInsight | null = null;
  let hourlyInsights: HourlyInsight[] = [];
  let dailyInsight: DailyInsight | null = null;

  let isLoading = false;
  let errorMessage: string | null = null;

  let insightsRequestId = 0;
  let expandedHourlyIds: number[] = [];

  let selectedDateLabel = '';
  let currentAppsUsed: Array<[string, string]> = [];
  let timeAllocationRows: TimeAllocationRow[] = [];
  let focusBlockVisuals: FocusBlockVisual[] = [];

  $: selectedDateLabel = formatDateLabel(selectedDate);
  $: currentAppsUsed = currentInsight
    ? Object.entries(currentInsight.data.apps_used).sort(([left], [right]) => left.localeCompare(right))
    : [];
  $: timeAllocationRows = dailyInsight ? buildTimeAllocationRows(dailyInsight.data.time_allocation) : [];
  $: focusBlockVisuals = dailyInsight ? buildFocusBlockVisuals(dailyInsight.data.focus_blocks) : [];

  onMount(() => {
    void refreshInsights();
  });

  async function refreshInsights(): Promise<void> {
    const currentRequestId = ++insightsRequestId;

    if (!/^\d{4}-\d{2}-\d{2}$/.test(selectedDate)) {
      errorMessage = 'Choose a valid day to review insights.';
      currentInsight = null;
      hourlyInsights = [];
      dailyInsight = null;
      return;
    }

    isLoading = true;
    errorMessage = null;

    try {
      const [rolling, hourly, daily] = await Promise.all([
        api.fetchCurrentInsight(),
        api.fetchHourlyInsights(selectedDate),
        api.fetchDailyInsight(selectedDate),
      ]);

      if (currentRequestId !== insightsRequestId) {
        return;
      }

      currentInsight = rolling;
      hourlyInsights = [...hourly].sort(
        (left, right) => Date.parse(right.data.hour_start) - Date.parse(left.data.hour_start),
      );
      dailyInsight = daily;

      const availableHourlyIds = new Set(hourlyInsights.map((insight) => insight.id));
      expandedHourlyIds = expandedHourlyIds.filter((insightId) => availableHourlyIds.has(insightId));
    } catch (error) {
      if (currentRequestId !== insightsRequestId) {
        return;
      }

      errorMessage = error instanceof Error ? error.message : 'Failed to load insights.';
      currentInsight = null;
      hourlyInsights = [];
      dailyInsight = null;
      expandedHourlyIds = [];
    } finally {
      if (currentRequestId === insightsRequestId) {
        isLoading = false;
      }
    }
  }

  function toggleHourlyDetails(insightId: number): void {
    if (expandedHourlyIds.includes(insightId)) {
      expandedHourlyIds = expandedHourlyIds.filter((id) => id !== insightId);
    } else {
      expandedHourlyIds = [...expandedHourlyIds, insightId];
    }
  }

  function toInputDate(value: Date): string {
    const localValue = new Date(value);
    localValue.setMinutes(localValue.getMinutes() - localValue.getTimezoneOffset());
    return localValue.toISOString().slice(0, 10);
  }

  function formatDateLabel(inputDate: string): string {
    const parsed = parseInputDate(inputDate);
    return parsed ? dateLabelFormatter.format(parsed) : inputDate;
  }

  function parseInputDate(inputDate: string): Date | null {
    if (!/^\d{4}-\d{2}-\d{2}$/.test(inputDate)) {
      return null;
    }

    const [year, month, day] = inputDate.split('-').map((value) => Number.parseInt(value, 10));
    const date = new Date(year, month - 1, day, 0, 0, 0, 0);
    return Number.isNaN(date.getTime()) ? null : date;
  }

  function formatHourRange(insight: HourlyInsight): string {
    const start = new Date(insight.data.hour_start);
    const end = new Date(insight.data.hour_end);
    return `${hourFormatter.format(start)}–${hourFormatter.format(end)}`;
  }

  function formatFocusScore(score: number): string {
    const bounded = Math.max(0, Math.min(score, 1));
    return `${Math.round(bounded * 100)}%`;
  }

  function formatMinutes(minutes: number): string {
    const hours = Math.floor(minutes / 60);
    const remainingMinutes = minutes % 60;

    if (hours > 0 && remainingMinutes > 0) {
      return `${hours}h ${remainingMinutes}m`;
    }

    if (hours > 0) {
      return `${hours}h`;
    }

    return `${remainingMinutes}m`;
  }

  function parseDurationToMinutes(duration: string): number {
    const hourMatch = duration.match(/(\d+(?:\.\d+)?)\s*h/i);
    const minuteMatch = duration.match(/(\d+)\s*m/i);

    let totalMinutes = 0;

    if (hourMatch) {
      totalMinutes += Math.round(Number.parseFloat(hourMatch[1]) * 60);
    }

    if (minuteMatch) {
      totalMinutes += Number.parseInt(minuteMatch[1], 10);
    }

    if (totalMinutes === 0) {
      const numericValue = Number.parseFloat(duration);
      if (Number.isFinite(numericValue)) {
        totalMinutes = Math.round(numericValue * 60);
      }
    }

    return Math.max(totalMinutes, 0);
  }

  function buildTimeAllocationRows(timeAllocation: Record<string, string>): TimeAllocationRow[] {
    const parsedRows = Object.entries(timeAllocation).map(([key, value]) => ({
      key,
      value,
      minutes: parseDurationToMinutes(value),
    }));

    const totalMinutes = parsedRows.reduce((sum, row) => sum + row.minutes, 0);

    return parsedRows
      .sort((left, right) => right.minutes - left.minutes)
      .map((row) => ({
        ...row,
        share: totalMinutes > 0 ? (row.minutes / totalMinutes) * 100 : 0,
      }));
  }

  function buildFocusBlockVisuals(blocks: FocusBlock[]): FocusBlockVisual[] {
    const totalMinutes = blocks.reduce((sum, block) => sum + block.duration_min, 0);

    return blocks.map((block) => ({
      ...block,
      share:
        totalMinutes > 0 ? (block.duration_min / totalMinutes) * 100 : 100 / Math.max(blocks.length, 1),
      tone: qualityTone(block.quality),
    }));
  }

  function qualityTone(quality: string): FocusBlockVisual['tone'] {
    const normalized = quality.toLowerCase();

    if (normalized.includes('deep')) {
      return 'deep';
    }

    if (normalized.includes('moderate') || normalized.includes('steady')) {
      return 'steady';
    }

    if (normalized.includes('light') || normalized.includes('shallow')) {
      return 'light';
    }

    return 'ambient';
  }
</script>

<section class="view insights-view">
  <header class="insights-view__header">
    <div>
      <p class="view__kicker">Insights</p>
      <h2 class="view__title">Signal architecture for {selectedDateLabel}.</h2>
      <p class="view__copy">
        Rolling context, hourly digests, and daily synthesis in one board. Expand the dense slices and
        keep open threads visible until they move.
      </p>
    </div>

    <div class="insights-controls">
      <label class="insights-controls__field" for="insights-date">
        <span>Review day</span>
        <input
          id="insights-date"
          type="date"
          bind:value={selectedDate}
          on:change={() => {
            void refreshInsights();
          }}
        />
      </label>

      <button type="button" class="insights-controls__button" on:click={() => void refreshInsights()}>
        Refresh insights
      </button>

      <p class="insights-controls__status" aria-live="polite">
        {#if isLoading}
          Updating insight feed…
        {:else if !errorMessage}
          Updated for {selectedDateLabel}
        {:else}
          Update failed
        {/if}
      </p>
    </div>
  </header>

  {#if errorMessage}
    <section class="insights-state insights-state--error" aria-live="assertive">
      <h3>Insights unavailable</h3>
      <p>{errorMessage}</p>
    </section>
  {:else if isLoading && !currentInsight && hourlyInsights.length === 0 && !dailyInsight}
    <section class="insights-state insights-state--loading" aria-label="Loading insights">
      <div></div>
      <div></div>
      <div></div>
    </section>
  {:else}
    <section class="panel rolling-context" aria-label="Current rolling context">
      <header class="panel__header">
        <h3>Current rolling context</h3>
      </header>

      {#if currentInsight}
        <p class="rolling-context__focus">{currentInsight.data.current_focus}</p>
        <p class="rolling-context__project">
          <span>Active project</span>
          <strong>{currentInsight.data.active_project ?? 'No project detected'}</strong>
        </p>

        <div class="rolling-context__apps">
          <p>Apps in play</p>
          {#if currentAppsUsed.length === 0}
            <p class="panel__empty-copy">No app usage has been attributed in this rolling window yet.</p>
          {:else}
            <ul>
              {#each currentAppsUsed as [appName, duration] (appName)}
                <li>
                  <span>{appName}</span>
                  <em>{duration}</em>
                </li>
              {/each}
            </ul>
          {/if}
        </div>
      {:else}
        <p class="panel__empty-copy">
          Rolling context is not available yet. Keep capturing activity and this card will fill in.
        </p>
      {/if}
    </section>

    <section class="panel hourly-digests" aria-label="Hourly digests">
      <header class="panel__header">
        <h3>Hourly digests</h3>
        <p>{hourlyInsights.length} hour block(s)</p>
      </header>

      {#if hourlyInsights.length === 0}
        <p class="panel__empty-copy">No hourly digests exist for this day yet.</p>
      {:else}
        <div class="hourly-digests__list">
          {#each hourlyInsights as insight (insight.id)}
            <article class="hourly-card" class:hourly-card--expanded={expandedHourlyIds.includes(insight.id)}>
              <button
                class="hourly-card__toggle"
                type="button"
                aria-expanded={expandedHourlyIds.includes(insight.id)}
                on:click={() => toggleHourlyDetails(insight.id)}
              >
                <div>
                  <p class="hourly-card__window">{formatHourRange(insight)}</p>
                  <p class="hourly-card__activity">{insight.data.dominant_activity}</p>
                </div>
                <p class="hourly-card__score">Focus {formatFocusScore(insight.data.focus_score)}</p>
              </button>

              {#if expandedHourlyIds.includes(insight.id)}
                <div class="hourly-card__details">
                  <section>
                    <h4>Projects</h4>
                    {#if insight.data.projects.length === 0}
                      <p class="panel__empty-copy">No project attribution for this hour.</p>
                    {:else}
                      <ul>
                        {#each insight.data.projects as project, projectIndex (`${insight.id}-project-${projectIndex}`)}
                          <li>
                            <p>
                              <strong>{project.name ?? 'Unattributed work'}</strong>
                              <span>{formatMinutes(project.minutes)}</span>
                            </p>
                            {#if project.activities.length > 0}
                              <div class="chip-row">
                                {#each project.activities as activity (`${insight.id}-activity-${activity}`)}
                                  <span>{activity}</span>
                                {/each}
                              </div>
                            {/if}
                          </li>
                        {/each}
                      </ul>
                    {/if}
                  </section>

                  <section>
                    <h4>Topics</h4>
                    {#if insight.data.topics.length === 0}
                      <p class="panel__empty-copy">No topics detected.</p>
                    {:else}
                      <div class="chip-row">
                        {#each insight.data.topics as topic (`${insight.id}-topic-${topic}`)}
                          <span>{topic}</span>
                        {/each}
                      </div>
                    {/if}
                  </section>

                  <section>
                    <h4>Key moments</h4>
                    {#if insight.data.key_moments.length === 0}
                      <p class="panel__empty-copy">No key moments captured in this hour.</p>
                    {:else}
                      <ul class="moment-list">
                        {#each insight.data.key_moments as moment (`${insight.id}-moment-${moment}`)}
                          <li>{moment}</li>
                        {/each}
                      </ul>
                    {/if}
                  </section>
                </div>
              {/if}
            </article>
          {/each}
        </div>
      {/if}
    </section>

    <section class="panel daily-summary" aria-label="Daily summary">
      <header class="panel__header">
        <h3>Daily synthesis</h3>
        {#if dailyInsight}
          <p>{dailyInsight.data.total_active_hours.toFixed(1)} active hours</p>
        {/if}
      </header>

      {#if dailyInsight}
        <p class="daily-summary__narrative">{dailyInsight.data.narrative}</p>

        <div class="daily-grid">
          <section>
            <h4>Project breakdown</h4>
            {#if dailyInsight.data.projects.length === 0}
              <p class="panel__empty-copy">No project roll-up available yet.</p>
            {:else}
              <ul class="project-list">
                {#each dailyInsight.data.projects as project (project.name)}
                  <li>
                    <p>
                      <strong>{project.name}</strong>
                      <span>{formatMinutes(project.total_minutes)}</span>
                    </p>
                    {#if project.activities.length > 0}
                      <div class="chip-row">
                        {#each project.activities as activity (`${project.name}-activity-${activity}`)}
                          <span>{activity}</span>
                        {/each}
                      </div>
                    {/if}
                    {#if project.key_accomplishments.length > 0}
                      <ul class="moment-list">
                        {#each project.key_accomplishments as accomplishment (`${project.name}-accomplishment-${accomplishment}`)}
                          <li>{accomplishment}</li>
                        {/each}
                      </ul>
                    {/if}
                  </li>
                {/each}
              </ul>
            {/if}
          </section>

          <section>
            <h4>Time allocation</h4>
            {#if timeAllocationRows.length === 0}
              <p class="panel__empty-copy">No time allocation summary available yet.</p>
            {:else}
              <ul class="allocation-list">
                {#each timeAllocationRows as row (row.key)}
                  <li>
                    <div>
                      <span>{row.key}</span>
                      <strong>{row.value}</strong>
                    </div>
                    <div class="allocation-list__track" aria-hidden="true">
                      <span style={`width: ${Math.max(row.share, 5)}%;`}></span>
                    </div>
                  </li>
                {/each}
              </ul>
            {/if}
          </section>
        </div>

        <section class="focus-visualization">
          <h4>Focus blocks</h4>
          {#if focusBlockVisuals.length === 0}
            <p class="panel__empty-copy">No focus blocks recorded for this day yet.</p>
          {:else}
            <div class="focus-track" aria-label="Focus blocks timeline">
              {#each focusBlockVisuals as block (`${block.start}-${block.end}-${block.project}`)}
                <div
                  class={`focus-track__segment focus-track__segment--${block.tone}`}
                  style={`--segment-share: ${Math.max(block.share, 7)};`}
                  title={`${block.start}–${block.end} · ${block.project} · ${block.quality}`}
                >
                  <span>{block.start}</span>
                  <strong>{block.project}</strong>
                </div>
              {/each}
            </div>
          {/if}
        </section>

        <section>
          <h4>Open threads</h4>
          {#if dailyInsight.data.open_threads.length === 0}
            <p class="panel__empty-copy">No open threads.</p>
          {:else}
            <ul class="thread-list">
              {#each dailyInsight.data.open_threads as thread (`thread-${thread}`)}
                <li>{thread}</li>
              {/each}
            </ul>
          {/if}
        </section>
      {:else}
        <p class="panel__empty-copy">
          Daily summary is not available for this day yet. Keep the daemon running until the daily synthesis
          interval completes.
        </p>
      {/if}
    </section>
  {/if}
</section>

<style>
  .insights-view {
    gap: 1.2rem;
    position: relative;
  }

  .insights-view::before {
    content: '';
    position: absolute;
    inset: 0;
    background: linear-gradient(135deg, rgba(46, 92, 255, 0.08), rgba(255, 208, 0, 0.05));
    border-radius: 1rem;
    pointer-events: none;
    z-index: 0;
  }

  .insights-view > * {
    position: relative;
    z-index: 1;
  }

  .insights-view__header {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(220px, 280px);
    gap: 1rem;
    align-items: start;
  }

  .insights-controls {
    border: 2px solid var(--surface-border);
    background: color-mix(in srgb, var(--surface-lift) 82%, black 18%);
    padding: 0.9rem;
    display: grid;
    gap: 0.75rem;
    box-shadow: 8px 8px 0 rgba(46, 92, 255, 0.35);
  }

  .insights-controls__field {
    display: grid;
    gap: 0.35rem;
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.12em;
  }

  .insights-controls__field input {
    border: 2px solid var(--surface-border);
    background: var(--bg);
    color: var(--text);
    padding: 0.48rem 0.6rem;
    font-family: var(--body-font);
    font-size: 0.9rem;
  }

  .insights-controls__button {
    border: 2px solid var(--surface-border);
    background: var(--accent);
    color: #0f1114;
    font-family: var(--body-font);
    font-size: 0.74rem;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    padding: 0.58rem 0.72rem;
    cursor: pointer;
    transition: transform 140ms ease, box-shadow 140ms ease;
  }

  .insights-controls__button:hover {
    transform: translate(2px, -2px);
    box-shadow: 4px 4px 0 #0f1114;
  }

  .insights-controls__status {
    margin: 0;
    color: var(--muted);
    font-size: 0.75rem;
  }

  .panel {
    border: 2px solid var(--surface-border);
    background: color-mix(in srgb, var(--surface) 90%, black 10%);
    padding: clamp(0.85rem, 2vw, 1.2rem);
    display: grid;
    gap: 0.85rem;
    box-shadow: 10px 10px 0 rgba(46, 92, 255, 0.2);
  }

  .panel__header {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 1rem;
  }

  .panel__header h3 {
    margin: 0;
    font-family: var(--display-font);
    font-size: clamp(1rem, 2vw, 1.4rem);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .panel__header p {
    margin: 0;
    color: var(--muted);
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.1em;
  }

  .panel__empty-copy {
    margin: 0;
    color: var(--muted);
  }

  .rolling-context {
    border-width: 3px;
    box-shadow: 14px 14px 0 rgba(255, 208, 0, 0.3);
  }

  .rolling-context__focus {
    margin: 0;
    font-size: clamp(1.1rem, 2.4vw, 1.45rem);
    line-height: 1.2;
    text-wrap: balance;
  }

  .rolling-context__project {
    margin: 0;
    display: grid;
    gap: 0.22rem;
  }

  .rolling-context__project span {
    text-transform: uppercase;
    letter-spacing: 0.14em;
    font-size: 0.68rem;
    color: var(--muted);
  }

  .rolling-context__project strong {
    font-size: 1rem;
  }

  .rolling-context__apps {
    display: grid;
    gap: 0.35rem;
  }

  .rolling-context__apps p {
    margin: 0;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    font-size: 0.68rem;
    color: var(--muted);
  }

  .rolling-context__apps ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-wrap: wrap;
    gap: 0.45rem;
  }

  .rolling-context__apps li {
    border: 1px solid var(--surface-border);
    background: color-mix(in srgb, var(--surface-lift) 80%, black 20%);
    padding: 0.45rem 0.55rem;
    display: inline-flex;
    gap: 0.5rem;
    align-items: baseline;
  }

  .rolling-context__apps em {
    color: var(--accent);
    font-style: normal;
    font-size: 0.78rem;
  }

  .hourly-digests__list {
    display: grid;
    gap: 0.7rem;
  }

  .hourly-card {
    border: 1px solid var(--surface-border);
    background: color-mix(in srgb, var(--surface-lift) 72%, black 28%);
    transition: transform 140ms ease, border-color 140ms ease;
  }

  .hourly-card--expanded {
    border-color: var(--accent);
    transform: translateX(4px);
  }

  .hourly-card__toggle {
    width: 100%;
    border: 0;
    background: transparent;
    color: inherit;
    font-family: inherit;
    text-align: left;
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 1rem;
    padding: 0.8rem 0.9rem;
    cursor: pointer;
  }

  .hourly-card__window {
    margin: 0;
    text-transform: uppercase;
    letter-spacing: 0.14em;
    font-size: 0.66rem;
    color: var(--muted);
  }

  .hourly-card__activity {
    margin: 0.18rem 0 0;
    font-size: 0.95rem;
    text-transform: capitalize;
  }

  .hourly-card__score {
    margin: 0;
    font-size: 0.74rem;
    color: var(--accent);
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  .hourly-card__details {
    border-top: 1px solid color-mix(in srgb, var(--surface-border) 45%, transparent 55%);
    padding: 0.75rem 0.9rem 0.9rem;
    display: grid;
    gap: 0.85rem;
  }

  .hourly-card__details h4,
  .daily-grid h4,
  .focus-visualization h4,
  .daily-summary h4 {
    margin: 0;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    font-size: 0.68rem;
    color: var(--accent);
  }

  .hourly-card__details ul,
  .project-list,
  .allocation-list,
  .thread-list,
  .moment-list {
    margin: 0;
    padding: 0;
    list-style: none;
    display: grid;
    gap: 0.55rem;
  }

  .hourly-card__details li,
  .project-list li,
  .allocation-list li,
  .thread-list li {
    border: 1px solid color-mix(in srgb, var(--surface-border) 38%, transparent 62%);
    padding: 0.5rem 0.6rem;
    background: color-mix(in srgb, var(--surface) 72%, black 28%);
  }

  .hourly-card__details li p,
  .project-list li p,
  .allocation-list li div {
    margin: 0;
    display: flex;
    justify-content: space-between;
    gap: 0.8rem;
    align-items: baseline;
  }

  .hourly-card__details li span,
  .project-list li span {
    color: var(--accent);
    font-size: 0.78rem;
  }

  .chip-row {
    margin-top: 0.45rem;
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
  }

  .chip-row span {
    font-size: 0.7rem;
    border: 1px solid color-mix(in srgb, var(--surface-border) 40%, transparent 60%);
    padding: 0.2rem 0.4rem;
    color: var(--muted);
  }

  .moment-list li {
    margin-left: 0.95rem;
    list-style: square;
    color: var(--muted);
  }

  .daily-summary {
    gap: 1rem;
  }

  .daily-summary__narrative {
    margin: 0;
    color: var(--muted);
    max-width: 68ch;
  }

  .daily-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.9rem;
  }

  .allocation-list__track {
    margin-top: 0.4rem;
    width: 100%;
    background: color-mix(in srgb, var(--surface-border) 18%, transparent 82%);
    border: 1px solid color-mix(in srgb, var(--surface-border) 32%, transparent 68%);
    height: 0.5rem;
  }

  .allocation-list__track span {
    display: block;
    height: 100%;
    background: linear-gradient(90deg, var(--accent), color-mix(in srgb, var(--accent) 40%, #ff7a00 60%));
  }

  .focus-visualization {
    display: grid;
    gap: 0.55rem;
  }

  .focus-track {
    display: flex;
    gap: 0.45rem;
    flex-wrap: wrap;
  }

  .focus-track__segment {
    flex: var(--segment-share) 1 90px;
    min-height: 3.2rem;
    border: 1px solid color-mix(in srgb, var(--surface-border) 45%, transparent 55%);
    padding: 0.4rem 0.45rem;
    display: grid;
    align-content: space-between;
    gap: 0.2rem;
  }

  .focus-track__segment span {
    font-size: 0.64rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--muted);
  }

  .focus-track__segment strong {
    font-size: 0.78rem;
    font-weight: 700;
  }

  .focus-track__segment--deep {
    background: color-mix(in srgb, #2e5cff 58%, #051126 42%);
  }

  .focus-track__segment--steady {
    background: color-mix(in srgb, #2e5cff 34%, #24324f 66%);
  }

  .focus-track__segment--light {
    background: color-mix(in srgb, #ffd000 40%, #5b4e1b 60%);
  }

  .focus-track__segment--ambient {
    background: color-mix(in srgb, #6f6f75 42%, #1f1f24 58%);
  }

  .thread-list li {
    border-left: 3px solid var(--accent);
  }

  .insights-state {
    border: 2px solid var(--surface-border);
    background: color-mix(in srgb, var(--surface-lift) 75%, black 25%);
    padding: 1rem;
  }

  .insights-state h3,
  .insights-state p {
    margin: 0;
  }

  .insights-state--loading {
    display: grid;
    gap: 0.6rem;
  }

  .insights-state--loading div {
    height: 2.6rem;
    background: linear-gradient(
      90deg,
      rgba(247, 245, 235, 0.09) 0%,
      rgba(247, 245, 235, 0.2) 50%,
      rgba(247, 245, 235, 0.09) 100%
    );
    background-size: 240% 100%;
    animation: shimmer 1.1s linear infinite;
  }

  .insights-state--error {
    border-color: #ff7a7a;
    box-shadow: 8px 8px 0 rgba(255, 122, 122, 0.2);
    display: grid;
    gap: 0.4rem;
  }

  @keyframes shimmer {
    from {
      background-position: 100% 0;
    }

    to {
      background-position: -100% 0;
    }
  }

  @media (max-width: 980px) {
    .insights-view__header {
      grid-template-columns: 1fr;
    }

    .daily-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
