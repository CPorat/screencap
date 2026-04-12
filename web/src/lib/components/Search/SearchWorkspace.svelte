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
  }

  const datePresetOptions: DatePresetOption[] = [
    { id: 'all', label: 'All time', hours: null },
    { id: '24h', label: '24h', hours: 24 },
    { id: '7d', label: '7d', hours: 168 },
    { id: '30d', label: '30d', hours: 720 },
  ];

  const searchModeOptions: SearchModeOption[] = [
    { id: 'keyword', label: 'Keyword' },
    { id: 'semantic', label: 'Semantic' },
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
    return parsed?.toISOString() ?? null;
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

<div class="h-full flex flex-col overflow-hidden" aria-busy={loading}>
  <!-- Search Command Center -->
  <div class="flex flex-col items-center justify-center pt-8 pb-6 px-8">
    <div class="w-full max-w-2xl relative">
      <div class="relative group">
        <div class="absolute inset-y-0 left-5 flex items-center pointer-events-none">
          <span class="material-symbols-outlined text-on-surface-variant group-focus-within:text-primary transition-colors">search</span>
        </div>
        <input
          id="search-query"
          name="q"
          type="search"
          bind:this={searchInput}
          bind:value={query}
          autocomplete="off"
          placeholder="Search your captures (e.g., 'billing flow from last Tuesday')"
          aria-label="Search captures"
          class="w-full h-14 pl-14 pr-32 bg-surface-container-lowest shadow-2xl shadow-outline/10 rounded-2xl border-none text-lg font-medium focus:ring-2 focus:ring-primary/20 transition-all placeholder:text-on-surface-variant"
        />
        <div class="absolute inset-y-0 right-4 flex items-center gap-2">
          <kbd class="px-2 py-1 bg-surface-container-high text-on-surface-variant rounded text-[10px] font-bold">&#x2318;K</kbd>
        </div>
      </div>

      <!-- Mode Toggle -->
      <div class="flex justify-center mt-5">
        <div class="bg-surface-container-high p-1 rounded-xl flex items-center gap-1">
          {#each searchModeOptions as option (option.id)}
            <button
              type="button"
              class="px-4 py-1.5 rounded-lg text-xs font-semibold transition-all {searchMode === option.id ? 'bg-surface-container-lowest text-primary shadow-sm' : 'text-on-surface-variant hover:text-on-surface'}"
              on:click={() => setSearchMode(option.id)}
            >
              {option.label}
            </button>
          {/each}
        </div>
      </div>
    </div>
  </div>

  <!-- Filter Bar -->
  <div class="px-8 py-3 flex items-center gap-3 overflow-x-auto no-scrollbar border-b border-surface-container-high/50">
    {#each datePresetOptions as option (option.id)}
      <button
        type="button"
        class="flex items-center gap-2 px-4 py-2 rounded-full text-xs font-semibold transition-colors {selectedPreset === option.id ? 'bg-primary-fixed text-primary' : 'bg-surface-container-lowest text-on-surface hover:bg-surface-container-low'}"
        on:click={() => applyDatePreset(option.id)}
      >
        {#if option.id === 'all'}
          <span class="material-symbols-outlined text-sm">calendar_today</span>
        {/if}
        {option.label}
      </button>
    {/each}

    {#if keywordMode}
      {#each appChips.slice(0, 4) as appName (appName)}
        <button
          type="button"
          class="flex items-center gap-2 px-4 py-2 rounded-full text-xs font-semibold transition-colors {selectedApp === appName ? 'bg-primary-fixed text-primary' : 'bg-surface-container-lowest text-on-surface hover:bg-surface-container-low'}"
          on:click={() => toggleApp(appName)}
        >
          <span class="material-symbols-outlined text-sm">apps</span>
          {appName}
        </button>
      {/each}

      {#each activityChips.slice(0, 3) as activity (activity)}
        <button
          type="button"
          class="flex items-center gap-2 px-4 py-2 rounded-full text-xs font-semibold transition-colors {selectedActivity === activity ? 'bg-primary-fixed text-primary' : 'bg-surface-container-lowest text-on-surface hover:bg-surface-container-low'}"
          on:click={() => toggleActivity(activity)}
        >
          <span class="material-symbols-outlined text-sm">label</span>
          {activity.replaceAll('_', ' ')}
        </button>
      {/each}
    {/if}

    {#if selectedApp || selectedProject || selectedActivity}
      <div class="h-6 w-px bg-outline-variant mx-2"></div>
      <button
        type="button"
        class="text-primary text-xs font-bold px-2 hover:underline"
        on:click={() => { selectedApp = null; selectedProject = null; selectedActivity = null; }}
      >
        Clear Filters
      </button>
    {/if}
  </div>

  <!-- Results Area -->
  <div class="flex-1 overflow-y-auto custom-scrollbar px-8 py-8">
    {#if errorMessage}
      <div class="bg-red-50 dark:bg-red-950/50 text-red-700 dark:text-red-300 rounded-2xl px-6 py-4 text-sm mb-6" role="alert">{errorMessage}</div>
    {/if}

    {#if searchMode === 'semantic' && semanticAnswer && !loading}
      <div class="bg-primary-fixed rounded-[24px] p-6 mb-6">
        <div class="flex items-center gap-2 mb-3">
          <span class="material-symbols-outlined text-primary">auto_awesome</span>
          <h3 class="text-sm font-bold text-primary uppercase tracking-widest">Semantic Answer</h3>
        </div>
        <p class="text-on-surface leading-relaxed">{semanticAnswer}</p>
        {#if semanticTokensUsed !== null || semanticCostLabel}
          <p class="mt-3 text-[10px] text-secondary font-bold uppercase tracking-wider">
            {#if semanticTokensUsed !== null}Tokens: {semanticTokensUsed}{/if}
            {#if semanticTokensUsed !== null && semanticCostLabel} · {/if}
            {#if semanticCostLabel}Cost: {semanticCostLabel}{/if}
          </p>
        {/if}
      </div>
    {/if}

    {#if !queryPreview}
      <div class="flex flex-col items-center justify-center py-16 text-center">
        <span class="material-symbols-outlined text-5xl text-on-surface-variant/40 mb-4">search</span>
        <h3 class="text-lg font-semibold text-on-surface mb-2">Search your captures</h3>
        <p class="text-sm text-secondary mb-6 max-w-md">Search debounces automatically. Try one of these prompts or type your own query.</p>
        <div class="flex flex-wrap gap-2 justify-center">
          {#each quickPrompts as prompt (prompt)}
            <button
              type="button"
              class="px-4 py-2 bg-surface-container-lowest rounded-full text-xs font-semibold text-on-surface hover:bg-surface-container-high transition-colors"
              on:click={() => applyQuickPrompt(prompt)}
            >
              {prompt}
            </button>
          {/each}
        </div>
      </div>
    {:else if loading}
      <div class="flex items-center justify-center py-16">
        <p class="text-sm text-secondary animate-pulse">
          {searchMode === 'semantic' ? 'Analyzing activity context...' : 'Searching indexed history...'}
        </p>
      </div>
    {:else if hasSearched && results.length === 0}
      <div class="flex flex-col items-center justify-center py-16 text-center">
        <span class="material-symbols-outlined text-5xl text-on-surface-variant/40 mb-4">search_off</span>
        <h3 class="text-lg font-semibold text-on-surface mb-2">No matches found</h3>
        <p class="text-sm text-secondary max-w-md">
          {searchMode === 'semantic'
            ? 'Try broadening date bounds or switching to keyword mode.'
            : 'Try broadening date bounds or clearing filters.'}
        </p>
      </div>
    {:else}
      <div class="mb-4 flex items-center justify-between">
        <h3 class="text-sm font-bold text-on-surface-variant uppercase tracking-widest">
          {searchMode === 'semantic' ? 'References' : 'Results'} ({results.length})
        </h3>
      </div>

      <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-6 pb-20" aria-live="polite">
        {#each visibleResults as result, index (`${result.sourceType}-${result.capture?.id ?? result.insight?.id ?? index}`)}
          <SearchResultCard {result} position={index + 1} on:open={(event) => void openResultDetails(event.detail.result)} />
        {/each}
      </div>

      {#if hasMoreResults}
        <div class="flex justify-center pt-4 pb-8">
          <button
            type="button"
            class="px-8 py-3 bg-surface-container-highest text-on-surface rounded-xl font-semibold text-sm hover:bg-surface-container-high transition-colors"
            on:click={loadMore}
          >
            Load more
          </button>
        </div>
      {/if}
    {/if}
  </div>

  <CaptureDetailsModal
    open={selectedCapture !== null}
    capture={selectedCapture}
    extraction={selectedExtraction}
    on:close={closeDetailsModal}
  />
</div>
