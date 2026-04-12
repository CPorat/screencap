<script lang="ts">
  import { onDestroy, onMount } from 'svelte';

  import { getApps, getCaptureDetail, type CaptureRecord, type ExtractionRecord } from '$lib/api';
  import CaptureDetailsModal from '$lib/components/CaptureDetailsModal.svelte';

  import SearchResultCard from './SearchResultCard.svelte';
  import {
    collectFacetValues,
    listProjectFilters,
    searchCaptures,
    searchSemanticCaptures,
    type SearchResult,
  } from './search-client';

  type DatePreset = 'all' | '24h' | '7d' | '30d' | 'custom';
  type SearchMode = 'keyword' | 'semantic';

  interface DatePresetOption {
    id: Exclude<DatePreset, 'custom'>;
    label: string;
    hours: number | null;
  }

  interface SearchModeOption {
    id: SearchMode;
    label: string;
    description: string;
  }

  const datePresetOptions: DatePresetOption[] = [
    { id: 'all', label: 'All time', hours: null },
    { id: '24h', label: '24h', hours: 24 },
    { id: '7d', label: '7d', hours: 168 },
    { id: '30d', label: '30d', hours: 720 },
  ];

  const searchModeOptions: SearchModeOption[] = [
    {
      id: 'keyword',
      label: 'Keyword',
      description: 'FTS-ranked results with app/project/activity filters.',
    },
    {
      id: 'semantic',
      label: 'Semantic',
      description: 'LLM-grounded answer plus capture references.',
    },
  ];

  const quickPrompts = ['sprint planning', 'incident follow-up', 'debugging', 'PR review'];
  const RESULTS_STEP = 18;

  let searchInput: HTMLInputElement | null = null;
  let query = '';
  let loading = false;
  let hasSearched = false;
  let errorMessage = '';

  let searchMode: SearchMode = 'keyword';
  let selectedApp: string | null = null;
  let selectedProject: string | null = null;
  let selectedActivity: string | null = null;

  let selectedPreset: DatePreset = '7d';
  let fromDate = '';
  let toDate = '';

  let results: SearchResult[] = [];
  let semanticAnswer: string | null = null;
  let semanticTokensUsed: number | null = null;
  let semanticCostCents: number | null = null;

  let appSuggestions: string[] = [];
  let projectSuggestions: string[] = [];

  let debounceHandle: ReturnType<typeof setTimeout> | null = null;
  let currentFingerprint = '';
  let requestCounter = 0;
  let activeSearchController: AbortController | null = null;
  let mounted = false;

  let visibleLimit = RESULTS_STEP;
  let paginationFingerprint = '';

  let selectedCapture: CaptureRecord | null = null;
  let selectedExtraction: ExtractionRecord | null = null;
  let detailLoading = false;
  let detailRequestCounter = 0;

  $: keywordMode = searchMode === 'keyword';
  $: facets = collectFacetValues(results);
  $: activityChips = collectActivityTypes(results);
  $: appChips = mergeFacetSuggestions(facets.apps, appSuggestions, selectedApp);
  $: projectChips = mergeFacetSuggestions(facets.projects, projectSuggestions, selectedProject);
  $: queryPreview = query.trim();
  $: semanticCostLabel = formatSemanticCost(semanticCostCents);
  $: visibleResults = results.slice(0, visibleLimit);
  $: hasMoreResults = visibleResults.length < results.length;

  $: fingerprint = [
    queryPreview,
    searchMode,
    keywordMode ? selectedApp ?? '' : '',
    keywordMode ? selectedProject ?? '' : '',
    keywordMode ? selectedActivity ?? '' : '',
    fromDate,
    toDate,
  ].join('::');
  $: if (mounted && fingerprint !== currentFingerprint) {
    currentFingerprint = fingerprint;
    queueSearch();
  }

  $: {
    const nextPaginationFingerprint = [
      queryPreview,
      searchMode,
      keywordMode ? selectedApp ?? '' : '',
      keywordMode ? selectedProject ?? '' : '',
      keywordMode ? selectedActivity ?? '' : '',
      fromDate,
      toDate,
      String(results.length),
    ].join('::');

    if (nextPaginationFingerprint !== paginationFingerprint) {
      paginationFingerprint = nextPaginationFingerprint;
      visibleLimit = RESULTS_STEP;
    }
  }

  onMount(async () => {
    mounted = true;
    applyDatePreset('7d');
    searchInput?.focus({ preventScroll: true });

    try {
      const [apps, projects] = await Promise.all([getApps(), listProjectFilters(hoursAgoIso(720))]);

      appSuggestions = apps
        .map((app) => app.app_name.trim())
        .filter((appName) => appName.length > 0)
        .slice(0, 12);

      projectSuggestions = projects.slice(0, 12);
    } catch (error) {
      console.warn('Failed to load search filter suggestions', error);
    }
  });

  onDestroy(() => {
    if (debounceHandle) {
      clearTimeout(debounceHandle);
    }
    activeSearchController?.abort();
    activeSearchController = null;
  });

  function isAbortError(error: unknown): boolean {
    if (error instanceof DOMException) {
      return error.name === 'AbortError';
    }

    return error instanceof Error && error.name === 'AbortError';
  }

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

    return [...merged].slice(0, 14);
  }

  function collectActivityTypes(entries: SearchResult[]): string[] {
    const values = new Set<string>();

    for (const entry of entries) {
      const activity = entry.primaryActivityType?.trim();
      if (activity) {
        values.add(activity);
      }
    }

    return [...values].sort((left, right) => left.localeCompare(right));
  }

  function toInputDate(value: Date): string {
    const year = value.getFullYear();
    const month = String(value.getMonth() + 1).padStart(2, '0');
    const day = String(value.getDate()).padStart(2, '0');
    return `${year}-${month}-${day}`;
  }

  function parseInputDate(value: string): Date | null {
    if (!/^\d{4}-\d{2}-\d{2}$/.test(value)) {
      return null;
    }

    const [yearRaw, monthRaw, dayRaw] = value.split('-').map((part) => Number(part));
    const parsed = new Date(yearRaw, monthRaw - 1, dayRaw, 0, 0, 0, 0);

    if (!Number.isFinite(parsed.getTime())) {
      return null;
    }

    return parsed;
  }

  function startOfDayIso(dateInput: string): string | null {
    const parsed = parseInputDate(dateInput);
    if (!parsed) {
      return null;
    }

    parsed.setHours(0, 0, 0, 0);
    return parsed.toISOString();
  }

  function nextDayStartIso(dateInput: string): string | null {
    const parsed = parseInputDate(dateInput);
    if (!parsed) {
      return null;
    }

    parsed.setDate(parsed.getDate() + 1);
    parsed.setHours(0, 0, 0, 0);
    return parsed.toISOString();
  }

  function hoursAgoIso(hours: number): string {
    return new Date(Date.now() - hours * 60 * 60 * 1000).toISOString();
  }

  function applyDatePreset(preset: DatePreset): void {
    selectedPreset = preset;

    if (preset === 'custom') {
      return;
    }

    if (preset === 'all') {
      fromDate = '';
      toDate = '';
      return;
    }

    const option = datePresetOptions.find((candidate) => candidate.id === preset);
    if (!option || option.hours === null) {
      return;
    }

    const now = new Date();
    const from = new Date(now.getTime() - option.hours * 60 * 60 * 1000);

    fromDate = toInputDate(from);
    toDate = toInputDate(now);
  }

  function normalizeDateRange(): { from: string | null; to: string | null; error: string | null } {
    const from = fromDate ? startOfDayIso(fromDate) : null;
    const to = toDate ? nextDayStartIso(toDate) : null;

    if ((fromDate && !from) || (toDate && !to)) {
      return {
        from: null,
        to: null,
        error: 'Enter valid dates before searching.',
      };
    }

    if (from && to && new Date(from).getTime() >= new Date(to).getTime()) {
      return {
        from: null,
        to: null,
        error: 'Date range is invalid: “From” must be before “To”.',
      };
    }

    return { from, to, error: null };
  }

  function queueSearch(): void {
    if (debounceHandle) {
      clearTimeout(debounceHandle);
    }

    if (!queryPreview) {
      activeSearchController?.abort();
      activeSearchController = null;
      loading = false;
      hasSearched = false;
      errorMessage = '';
      results = [];
      semanticAnswer = null;
      semanticTokensUsed = null;
      semanticCostCents = null;
      return;
    }

    debounceHandle = setTimeout(() => {
      void executeSearch();
    }, 300);
  }

  async function executeSearch(): Promise<void> {
    const trimmedQuery = query.trim();
    if (!trimmedQuery) {
      return;
    }

    const dateRange = normalizeDateRange();
    if (dateRange.error) {
      loading = false;
      hasSearched = true;
      errorMessage = dateRange.error;
      results = [];
      semanticAnswer = null;
      semanticTokensUsed = null;
      semanticCostCents = null;
      return;
    }

    loading = true;
    errorMessage = '';
    hasSearched = true;

    const requestId = ++requestCounter;
    activeSearchController?.abort();
    const controller = new AbortController();
    activeSearchController = controller;

    try {
      if (searchMode === 'semantic') {
        const semanticResult = await searchSemanticCaptures({
          query: trimmedQuery,
          from: dateRange.from,
          to: dateRange.to,
          limit: 120,
        }, controller.signal);

        if (requestId !== requestCounter) {
          return;
        }

        results = semanticResult.references;
        semanticAnswer = semanticResult.answer || null;
        semanticTokensUsed = semanticResult.tokensUsed;
        semanticCostCents = semanticResult.costCents;
        selectedActivity = null;
        return;
      }

      const nextResults = await searchCaptures({
        query: trimmedQuery,
        app: selectedApp,
        project: selectedProject,
        activityType: selectedActivity,
        from: dateRange.from,
        to: dateRange.to,
        limit: 120,
      }, controller.signal);

      if (requestId !== requestCounter) {
        return;
      }

      results = nextResults;
      semanticAnswer = null;
      semanticTokensUsed = null;
      semanticCostCents = null;
    } catch (error) {
      if (isAbortError(error)) {
        return;
      }

      if (requestId !== requestCounter) {
        return;
      }

      console.error('Failed to search captures', error);
      results = [];
      semanticAnswer = null;
      semanticTokensUsed = null;
      semanticCostCents = null;
      errorMessage = 'Could not load search results. Please try again.';
    } finally {
      if (activeSearchController === controller) {
        activeSearchController = null;
      }
      if (requestId === requestCounter) {
        loading = false;
      }
    }
  }

  function setSearchMode(mode: SearchMode): void {
    if (mode === searchMode) {
      return;
    }

    searchMode = mode;
    errorMessage = '';
    results = [];
    semanticAnswer = null;
    semanticTokensUsed = null;
    semanticCostCents = null;

    if (mode === 'semantic') {
      selectedApp = null;
      selectedProject = null;
      selectedActivity = null;
    }
  }

  function toggleApp(appName: string): void {
    if (!keywordMode) {
      return;
    }

    selectedApp = selectedApp === appName ? null : appName;
  }

  function toggleProject(projectName: string): void {
    if (!keywordMode) {
      return;
    }

    selectedProject = selectedProject === projectName ? null : projectName;
  }

  function toggleActivity(activity: string): void {
    if (!keywordMode) {
      return;
    }

    selectedActivity = selectedActivity === activity ? null : activity;
  }

  function applyQuickPrompt(prompt: string): void {
    query = prompt;
    if (keywordMode) {
      selectedActivity = null;
    }
  }

  function updateFromDate(value: string): void {
    fromDate = value;
    selectedPreset = 'custom';
  }

  function updateToDate(value: string): void {
    toDate = value;
    selectedPreset = 'custom';
  }

  function loadMore(): void {
    visibleLimit += RESULTS_STEP;
  }

  function formatSemanticCost(costCents: number | null): string | null {
    if (costCents === null || !Number.isFinite(costCents)) {
      return null;
    }

    return `${costCents.toFixed(2)}¢`;
  }

  async function openResultDetails(result: SearchResult): Promise<void> {
    if (result.sourceType !== 'extraction' || !result.capture) {
      return;
    }

    selectedCapture = result.capture;
    selectedExtraction = result.extraction;
    detailLoading = true;

    const requestId = ++detailRequestCounter;

    try {
      const detail = await getCaptureDetail(result.capture.id);
      if (requestId !== detailRequestCounter || selectedCapture?.id !== result.capture.id) {
        return;
      }

      selectedExtraction = detail?.extraction ?? result.extraction;
    } catch (error) {
      if (requestId !== detailRequestCounter) {
        return;
      }

      console.warn(`Failed to load capture detail for ${result.capture.id}`, error);
      selectedExtraction = result.extraction;
    } finally {
      if (requestId === detailRequestCounter) {
        detailLoading = false;
      }
    }
  }

  function closeDetailsModal(): void {
    detailRequestCounter += 1;
    detailLoading = false;
    selectedCapture = null;
    selectedExtraction = null;
  }
</script>

<section class="panel" aria-busy={loading}>
  <header class="panel__header">
    <p class="panel__section">Search</p>
    <h2>Memory retrieval deck</h2>
    <p class="panel__summary">
      Switch between ranked keyword search and semantic Q&A, then refine by date and keyword-mode facets.
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
        <h3>Mode</h3>
        <p>Keyword search uses FTS ranking; semantic mode asks the model and returns grounded references.</p>
      </header>
      <div class="chips">
        {#each searchModeOptions as option (option.id)}
          <button
            type="button"
            class="chip"
            class:chip--active={searchMode === option.id}
            on:click={() => setSearchMode(option.id)}
          >
            {option.label}
          </button>
        {/each}
      </div>
      <p class="chip-note">
        {searchModeOptions.find((option) => option.id === searchMode)?.description}
      </p>
    </article>

    <article class="chip-group">
      <header>
        <h3>Date range</h3>
        <p>Preset windows or custom from/to boundaries.</p>
      </header>

      <div class="chips">
        {#each datePresetOptions as option (option.id)}
          <button
            type="button"
            class="chip"
            class:chip--active={selectedPreset === option.id}
            on:click={() => applyDatePreset(option.id)}
          >
            {option.label}
          </button>
        {/each}
      </div>

      <div class="date-controls">
        <label>
          <span>From</span>
          <input
            type="date"
            value={fromDate}
            on:input={(event) => updateFromDate((event.currentTarget as HTMLInputElement).value)}
          />
        </label>
        <label>
          <span>To</span>
          <input
            type="date"
            value={toDate}
            on:input={(event) => updateToDate((event.currentTarget as HTMLInputElement).value)}
          />
        </label>
      </div>
    </article>

    <article class="chip-group">
      <header>
        <h3>Apps</h3>
        <p>Refine keyword mode by frontmost application.</p>
      </header>
      <div class="chips">
        <button
          type="button"
          class="chip"
          class:chip--active={!selectedApp}
          on:click={() => (selectedApp = null)}
          disabled={!keywordMode}
        >
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
              disabled={!keywordMode}
            >
              {appName}
            </button>
          {/each}
        {/if}
      </div>
      {#if !keywordMode}
        <p class="chip-note">App filters are available in keyword mode only.</p>
      {/if}
    </article>

    <article class="chip-group">
      <header>
        <h3>Projects</h3>
        <p>Limit keyword retrieval to project context.</p>
      </header>
      <div class="chips">
        <button
          type="button"
          class="chip"
          class:chip--active={!selectedProject}
          on:click={() => (selectedProject = null)}
          disabled={!keywordMode}
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
              disabled={!keywordMode}
            >
              {projectName}
            </button>
          {/each}
        {/if}
      </div>
      {#if !keywordMode}
        <p class="chip-note">Project filters are available in keyword mode only.</p>
      {/if}
    </article>

    <article class="chip-group">
      <header>
        <h3>Activity</h3>
        <p>Send activity type to the keyword API for server-side filtering.</p>
      </header>
      <div class="chips">
        <button
          type="button"
          class="chip"
          class:chip--active={!selectedActivity}
          on:click={() => (selectedActivity = null)}
          disabled={!keywordMode}
        >
          All activities
        </button>
        {#if activityChips.length === 0}
          <span class="chip-empty">No activity types in results</span>
        {:else}
          {#each activityChips as activity (activity)}
            <button
              type="button"
              class="chip"
              class:chip--active={selectedActivity === activity}
              on:click={() => toggleActivity(activity)}
              disabled={!keywordMode}
            >
              {activity.replaceAll('_', ' ')}
            </button>
          {/each}
        {/if}
      </div>
      {#if !keywordMode}
        <p class="chip-note">Semantic mode does not apply activity chips.</p>
      {/if}
    </article>
  </section>

  {#if queryPreview}
    <p class="result-summary">
      {#if loading}
        {searchMode === 'semantic' ? `Analyzing “${queryPreview}”…` : `Searching “${queryPreview}”…`}
      {:else if searchMode === 'semantic'}
        Showing {visibleResults.length} of {results.length} grounded reference{results.length === 1 ? '' : 's'} for “{queryPreview}”.
      {:else}
        Showing {visibleResults.length} of {results.length} result{results.length === 1 ? '' : 's'} for “{queryPreview}”.
      {/if}
    </p>
  {/if}

  {#if errorMessage}
    <p class="panel__error" role="alert">{errorMessage}</p>
  {/if}

  {#if searchMode === 'semantic' && semanticAnswer && !loading}
    <section class="semantic-answer" aria-label="Semantic answer">
      <h3>Semantic answer</h3>
      <p>{semanticAnswer}</p>
      {#if semanticTokensUsed !== null || semanticCostLabel}
        <p class="semantic-answer__meta">
          {#if semanticTokensUsed !== null}
            Tokens: {semanticTokensUsed}
          {/if}
          {#if semanticTokensUsed !== null && semanticCostLabel}
            ·
          {/if}
          {#if semanticCostLabel}
            Cost: {semanticCostLabel}
          {/if}
        </p>
      {/if}
    </section>
  {/if}

  {#if !queryPreview}
    <section class="empty-state" aria-label="Search suggestions">
      <h3>Start with a prompt</h3>
      <p>Search debounces requests automatically. Try one of these prompts or type your own.</p>
      <div class="chips">
        {#each quickPrompts as prompt (prompt)}
          <button type="button" class="chip" on:click={() => applyQuickPrompt(prompt)}>
            {prompt}
          </button>
        {/each}
      </div>
    </section>
  {:else if loading}
    <p class="panel__state">{searchMode === 'semantic' ? 'Analyzing activity context…' : 'Searching indexed history…'}</p>
  {:else if hasSearched && results.length === 0}
    <section class="empty-state" aria-label="No search results">
      <h3>No matches for this filter set</h3>
      <p>
        {searchMode === 'semantic'
          ? 'Try broadening date bounds or switching to keyword mode.'
          : 'Try broadening date bounds or clearing app/project/activity chips.'}
      </p>
    </section>
  {:else}
    <div class="results-grid" aria-live="polite">
      {#each visibleResults as result, index (`${result.sourceType}-${result.capture?.id ?? result.insight?.id ?? index}`)}
        <SearchResultCard {result} position={index + 1} on:open={(event) => void openResultDetails(event.detail.result)} />
      {/each}
    </div>

    {#if hasMoreResults}
      <div class="load-more-wrap">
        <button class="load-more" type="button" on:click={loadMore}>Load more</button>
      </div>
    {/if}
  {/if}

  {#if detailLoading && selectedCapture}
    <p class="panel__state">Loading full extraction payload…</p>
  {/if}

  <CaptureDetailsModal
    open={selectedCapture !== null}
    capture={selectedCapture}
    extraction={selectedExtraction}
    on:close={closeDetailsModal}
  />
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
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.75rem;
  }

  .chip-group {
    border: 1px solid rgb(246 241 231 / 22%);
    border-radius: 0.86rem;
    padding: 0.74rem;
    background: rgb(8 11 19 / 62%);
    display: grid;
    gap: 0.54rem;
    align-content: start;
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

  .chip-note {
    font-size: 0.68rem;
    color: var(--paper-200);
    letter-spacing: 0.08em;
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

  .chip:hover:not(:disabled) {
    transform: translateY(-1px);
    border-color: rgb(246 241 231 / 58%);
  }

  .chip--active {
    border-color: rgb(112 255 227 / 74%);
    background: rgb(112 255 227 / 14%);
    color: var(--pulse);
  }

  .chip:disabled {
    cursor: not-allowed;
    opacity: 0.5;
    transform: none;
  }

  .chip-empty {
    font-size: 0.68rem;
    color: var(--paper-200);
    letter-spacing: 0.08em;
    text-transform: uppercase;
    align-self: center;
  }

  .date-controls {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.5rem;
  }

  .date-controls label {
    display: grid;
    gap: 0.28rem;
  }

  .date-controls span {
    font-size: 0.62rem;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--paper-200);
  }

  .date-controls input {
    width: 100%;
    border: 1px solid rgb(246 241 231 / 36%);
    border-radius: 0.6rem;
    background: rgb(14 18 28 / 94%);
    color: var(--paper-100);
    padding: 0.36rem 0.45rem;
    font: inherit;
    font-size: 0.75rem;
  }

  .date-controls input:focus-visible {
    outline: 2px solid var(--pulse);
    outline-offset: 1px;
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

  .semantic-answer {
    border: 1px solid rgb(112 255 227 / 38%);
    border-radius: 0.86rem;
    padding: 0.8rem 0.9rem;
    background: rgb(10 17 25 / 78%);
    display: grid;
    gap: 0.45rem;
  }

  .semantic-answer h3 {
    font-size: 0.82rem;
    letter-spacing: 0.11em;
    text-transform: uppercase;
    color: var(--pulse);
  }

  .semantic-answer p {
    color: var(--paper-100);
    font-size: 0.88rem;
    line-height: 1.45;
    margin: 0;
  }

  .semantic-answer__meta {
    color: var(--paper-200);
    font-size: 0.74rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
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

  .load-more-wrap {
    display: flex;
    justify-content: center;
  }

  .load-more {
    border: 1px solid rgb(255 179 71 / 54%);
    border-radius: 0.74rem;
    background: rgb(255 179 71 / 11%);
    color: var(--paper-100);
    font: inherit;
    font-size: 0.74rem;
    text-transform: uppercase;
    letter-spacing: 0.13em;
    padding: 0.58rem 0.92rem;
    cursor: pointer;
    transition: transform 150ms ease, border-color 150ms ease, background 150ms ease;
  }

  .load-more:hover,
  .load-more:focus-visible {
    transform: translateY(-1px);
    border-color: rgb(255 179 71 / 86%);
    background: rgb(255 179 71 / 18%);
    outline: none;
  }

  @media (width <= 960px) {
    .chip-stack {
      grid-template-columns: 1fr;
    }
  }

  @media (width <= 760px) {
    .chip-group header {
      display: grid;
      justify-items: start;
    }

    .date-controls {
      grid-template-columns: 1fr;
    }

    .results-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
