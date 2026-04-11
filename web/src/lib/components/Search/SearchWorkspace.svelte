<script lang="ts">
  import { onDestroy, onMount } from 'svelte';

  import { getApps } from '$lib/api';

  import SearchResultCard from './SearchResultCard.svelte';
  import { collectFacetValues, listProjectFilters, searchCaptures, type SearchResult } from './search-client';

  type TimeWindow = 'all' | '24h' | '72h' | '168h' | '720h';

  interface TimeRangeOption {
    id: TimeWindow;
    label: string;
    description: string;
    hours: number | null;
  }

  const timeRangeOptions: TimeRangeOption[] = [
    {
      id: 'all',
      label: 'All time',
      description: 'Search across every indexed extraction.',
      hours: null,
    },
    {
      id: '24h',
      label: '24h',
      description: 'Only the most recent day of activity.',
      hours: 24,
    },
    {
      id: '72h',
      label: '3d',
      description: 'Last 72 hours of captures and extraction notes.',
      hours: 72,
    },
    {
      id: '168h',
      label: '7d',
      description: 'Last week of indexed context.',
      hours: 168,
    },
    {
      id: '720h',
      label: '30d',
      description: 'Search up to one month back.',
      hours: 720,
    },
  ];

  const quickPrompts = ['sprint planning', 'incident follow-up', 'debugging', 'PR review'];

  let searchInput: HTMLInputElement | null = null;
  let query = '';
  let loading = false;
  let hasSearched = false;
  let errorMessage = '';

  let selectedApp: string | null = null;
  let selectedProject: string | null = null;
  let selectedWindow: TimeWindow = '168h';

  let results: SearchResult[] = [];
  let appSuggestions: string[] = [];
  let projectSuggestions: string[] = [];

  let debounceHandle: ReturnType<typeof setTimeout> | null = null;
  let currentFingerprint = '';
  let requestCounter = 0;
  let mounted = false;

  $: facets = collectFacetValues(results);
  $: appChips = mergeFacetSuggestions(facets.apps, appSuggestions, selectedApp);
  $: projectChips = mergeFacetSuggestions(facets.projects, projectSuggestions, selectedProject);
  $: queryPreview = query.trim();

  $: fingerprint = [queryPreview, selectedApp ?? '', selectedProject ?? '', selectedWindow].join('::');
  $: if (mounted && fingerprint !== currentFingerprint) {
    currentFingerprint = fingerprint;
    queueSearch();
  }

  onMount(async () => {
    mounted = true;
    searchInput?.focus({ preventScroll: true });

    try {
      const [apps, projects] = await Promise.all([
        getApps(),
        listProjectFilters(hoursAgoIso(720)),
      ]);

      appSuggestions = apps
        .map((app) => app.app_name.trim())
        .filter((appName) => appName.length > 0)
        .slice(0, 10);

      projectSuggestions = projects.slice(0, 10);
    } catch (error) {
      console.warn('Failed to load search filter suggestions', error);
    }
  });

  onDestroy(() => {
    if (debounceHandle) {
      clearTimeout(debounceHandle);
    }
  });

  function mergeFacetSuggestions(primary: string[], secondary: string[], selected: string | null): string[] {
    const merged = new Set<string>();

    if (selected?.trim()) {
      merged.add(selected.trim());
    }

    for (const value of primary) {
      const trimmed = value.trim();
      if (trimmed) {
        merged.add(trimmed);
      }
    }

    for (const value of secondary) {
      const trimmed = value.trim();
      if (trimmed) {
        merged.add(trimmed);
      }
    }

    return [...merged].slice(0, 12);
  }

  function hoursAgoIso(hours: number | null): string | null {
    if (hours === null) {
      return null;
    }

    return new Date(Date.now() - hours * 60 * 60 * 1000).toISOString();
  }

  function queueSearch(): void {
    if (debounceHandle) {
      clearTimeout(debounceHandle);
    }

    if (!queryPreview) {
      loading = false;
      hasSearched = false;
      errorMessage = '';
      results = [];
      return;
    }

    debounceHandle = setTimeout(() => {
      void executeSearch();
    }, 280);
  }

  async function executeSearch(): Promise<void> {
    const trimmedQuery = query.trim();
    if (!trimmedQuery) {
      return;
    }

    loading = true;
    errorMessage = '';
    hasSearched = true;

    const requestId = ++requestCounter;

    try {
      const selectedRange = timeRangeOptions.find((option) => option.id === selectedWindow) ?? timeRangeOptions[0];
      const nextResults = await searchCaptures({
        query: trimmedQuery,
        app: selectedApp,
        project: selectedProject,
        from: hoursAgoIso(selectedRange.hours),
        limit: 90,
      });

      if (requestId !== requestCounter) {
        return;
      }

      results = nextResults;
    } catch (error) {
      if (requestId !== requestCounter) {
        return;
      }

      console.error('Failed to search captures', error);
      results = [];
      errorMessage = 'Could not load search results. Please try again.';
    } finally {
      if (requestId === requestCounter) {
        loading = false;
      }
    }
  }

  function toggleApp(appName: string): void {
    selectedApp = selectedApp === appName ? null : appName;
  }

  function toggleProject(project: string): void {
    selectedProject = selectedProject === project ? null : project;
  }

  function applyQuickPrompt(prompt: string): void {
    query = prompt;
    selectedApp = null;
    selectedProject = null;
  }
</script>

<section class="panel" aria-busy={loading}>
  <header class="panel__header">
    <p class="panel__section">Search</p>
    <h2>Memory retrieval deck</h2>
    <p class="panel__summary">
      Live FTS across extraction summaries, projects, topics, and app context. Ranked hits surface your most relevant captures first.
    </p>
  </header>

  <label class="search-input" for="search-query">
    <span>Search captures</span>
    <input
      id="search-query"
      name="q"
      type="search"
      bind:this={searchInput}
      bind:value={query}
      autocomplete="off"
      placeholder="Find moments, tasks, topics, or people"
      aria-label="Search captures"
    />
  </label>

  <section class="chip-stack" aria-label="Search filters">
    <article class="chip-group">
      <header>
        <h3>Time range</h3>
        <p>{timeRangeOptions.find((option) => option.id === selectedWindow)?.description}</p>
      </header>
      <div class="chips">
        {#each timeRangeOptions as range (range.id)}
          <button
            type="button"
            class="chip"
            class:chip--active={selectedWindow === range.id}
            on:click={() => {
              selectedWindow = range.id;
            }}
          >
            {range.label}
          </button>
        {/each}
      </div>
    </article>

    <article class="chip-group">
      <header>
        <h3>Apps</h3>
        <p>Refine by frontmost app name.</p>
      </header>
      <div class="chips">
        <button type="button" class="chip" class:chip--active={!selectedApp} on:click={() => (selectedApp = null)}>
          All apps
        </button>
        {#if appChips.length === 0}
          <span class="chip-empty">No app facets yet</span>
        {:else}
          {#each appChips as appName (appName)}
            <button
              type="button"
              class="chip"
              class:chip--active={selectedApp === appName}
              on:click={() => toggleApp(appName)}
            >
              {appName}
            </button>
          {/each}
        {/if}
      </div>
    </article>

    <article class="chip-group">
      <header>
        <h3>Projects</h3>
        <p>Slice relevance ranking by project context.</p>
      </header>
      <div class="chips">
        <button
          type="button"
          class="chip"
          class:chip--active={!selectedProject}
          on:click={() => (selectedProject = null)}
        >
          All projects
        </button>
        {#if projectChips.length === 0}
          <span class="chip-empty">No project facets yet</span>
        {:else}
          {#each projectChips as projectName (projectName)}
            <button
              type="button"
              class="chip"
              class:chip--active={selectedProject === projectName}
              on:click={() => toggleProject(projectName)}
            >
              {projectName}
            </button>
          {/each}
        {/if}
      </div>
    </article>
  </section>

  {#if queryPreview}
    <p class="result-summary">
      {#if loading}
        Searching “{queryPreview}”…
      {:else}
        {results.length} ranked result{results.length === 1 ? '' : 's'} for “{queryPreview}”.
      {/if}
    </p>
  {/if}

  {#if errorMessage}
    <p class="panel__error" role="alert">{errorMessage}</p>
  {/if}

  {#if !queryPreview}
    <section class="empty-state" aria-label="Search suggestions">
      <h3>Start with a prompt</h3>
      <p>
        Search waits for your input and debounces requests automatically. Try one of these prompts or type your own.
      </p>
      <div class="chips">
        {#each quickPrompts as prompt (prompt)}
          <button type="button" class="chip" on:click={() => applyQuickPrompt(prompt)}>
            {prompt}
          </button>
        {/each}
      </div>
    </section>
  {:else if loading}
    <p class="panel__state">Searching indexed captures…</p>
  {:else if hasSearched && results.length === 0}
    <section class="empty-state" aria-label="No search results">
      <h3>No matches for that filter set</h3>
      <p>Try broadening the time range or clearing app/project chips to surface adjacent captures.</p>
    </section>
  {:else}
    <div class="results-grid" aria-live="polite">
      {#each results as result, index (result.capture.id)}
        <SearchResultCard {result} position={index + 1} />
      {/each}
    </div>
  {/if}
</section>

<style>
  .panel {
    height: 100%;
    padding: clamp(1.2rem, 2.6vw, 2rem);
    display: grid;
    align-content: start;
    gap: 1rem;
    overflow: auto;
    background:
      linear-gradient(156deg, rgb(31 39 57 / 94%), rgb(12 15 24 / 98%)),
      radial-gradient(circle at 78% 12%, rgb(255 179 71 / 14%), transparent 34%);
  }

  .panel__header {
    display: grid;
    gap: 0.52rem;
  }

  .panel__section {
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.22em;
    color: var(--pulse);
  }

  h2 {
    font-size: clamp(2.2rem, 5.8vw, 4rem);
    text-wrap: balance;
  }

  .panel__summary,
  .panel__state {
    color: var(--paper-200);
    font-size: 0.9rem;
  }

  .search-input {
    display: grid;
    gap: 0.52rem;
    border: 1px solid rgb(246 241 231 / 34%);
    border-radius: 1.05rem;
    padding: 0.95rem;
    background:
      linear-gradient(130deg, rgb(6 9 16 / 90%), rgb(17 20 33 / 72%)),
      radial-gradient(circle at 90% 10%, rgb(112 255 227 / 12%), transparent 48%);
    box-shadow: inset 0 0 0 1px rgb(255 255 255 / 3%);
  }

  .search-input span {
    font-size: 0.68rem;
    text-transform: uppercase;
    letter-spacing: 0.16em;
    color: var(--paper-200);
  }

  .search-input input {
    width: 100%;
    border: 2px solid rgb(112 255 227 / 42%);
    border-radius: 0.86rem;
    padding: clamp(0.9rem, 1.6vw, 1.05rem) clamp(0.9rem, 1.7vw, 1.1rem);
    background: rgb(11 15 24 / 94%);
    color: var(--paper-100);
    font-family: var(--display-font);
    letter-spacing: 0.04em;
    font-size: clamp(1.24rem, 3.5vw, 2.2rem);
    text-transform: uppercase;
    transition: border-color 180ms ease, box-shadow 180ms ease, transform 180ms ease;
  }

  .search-input input::placeholder {
    color: rgb(221 213 198 / 58%);
  }

  .search-input input:focus-visible {
    outline: none;
    border-color: var(--surge);
    box-shadow:
      0 0 0 1px rgb(255 78 166 / 45%),
      0 0 0 5px rgb(255 78 166 / 12%);
    transform: translateY(-1px);
  }

  .chip-stack {
    display: grid;
    gap: 0.75rem;
  }

  .chip-group {
    border: 1px solid rgb(246 241 231 / 22%);
    border-radius: 0.86rem;
    padding: 0.74rem;
    background: rgb(8 11 19 / 62%);
    display: grid;
    gap: 0.54rem;
  }

  .chip-group header {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    justify-content: space-between;
    gap: 0.42rem;
  }

  .chip-group h3 {
    font-size: 0.82rem;
    letter-spacing: 0.1em;
  }

  .chip-group p {
    font-size: 0.72rem;
    color: var(--paper-200);
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 0.46rem;
  }

  .chip {
    border: 1px solid rgb(246 241 231 / 34%);
    border-radius: 999px;
    padding: 0.35rem 0.68rem;
    background: rgb(246 241 231 / 4%);
    color: var(--paper-100);
    font-size: 0.66rem;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    cursor: pointer;
    transition: transform 130ms ease, border-color 130ms ease, background 130ms ease;
  }

  .chip:hover {
    transform: translateY(-1px);
    border-color: rgb(246 241 231 / 58%);
  }

  .chip--active {
    border-color: rgb(112 255 227 / 74%);
    background: rgb(112 255 227 / 14%);
    color: var(--pulse);
  }

  .chip-empty {
    font-size: 0.68rem;
    color: var(--paper-200);
    letter-spacing: 0.08em;
    text-transform: uppercase;
    align-self: center;
  }

  .result-summary {
    font-family: var(--display-font);
    letter-spacing: 0.04em;
    font-size: clamp(1rem, 2.2vw, 1.28rem);
    color: var(--paper-100);
  }

  .panel__error {
    color: #ff8f8f;
    border: 1px solid rgb(255 143 143 / 55%);
    border-radius: 0.7rem;
    padding: 0.55rem 0.8rem;
    background: rgb(69 18 21 / 56%);
    font-size: 0.85rem;
  }

  .empty-state {
    border: 1px dashed rgb(246 241 231 / 35%);
    border-radius: 0.95rem;
    background:
      linear-gradient(145deg, rgb(9 13 20 / 74%), rgb(22 26 38 / 58%)),
      radial-gradient(circle at 12% 22%, rgb(112 255 227 / 12%), transparent 40%);
    padding: 1rem;
    display: grid;
    gap: 0.62rem;
  }

  .empty-state h3 {
    font-size: 1rem;
    letter-spacing: 0.08em;
  }

  .empty-state p {
    color: var(--paper-200);
    font-size: 0.84rem;
    max-width: 60ch;
  }

  .results-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: 0.9rem;
    align-content: start;
  }

  @media (width <= 760px) {
    .chip-group header {
      display: grid;
      justify-items: start;
    }

    .results-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
