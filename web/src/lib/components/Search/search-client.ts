import type { CaptureRecord } from '$lib/api';

interface SearchApiExtraction {
  id: number;
  capture_id: number;
  batch_id: string;
  activity_type: string | null;
  description: string | null;
  app_context: string | null;
  project: string | null;
  topics: string[];
  people: string[];
  key_content: string | null;
  sentiment: string | null;
  created_at: string;
}

interface SearchApiInsight {
  id: number;
  insight_type: string;
  window_start: string;
  window_end: string;
  narrative: string;
}

interface SearchApiExtractionHit {
  source_type: 'extraction';
  timestamp: string;
  rank: number;
  capture: CaptureRecord;
  extraction: SearchApiExtraction;
  batch_narrative: string | null;
}

interface SearchApiInsightHit {
  source_type: 'insight';
  timestamp: string;
  rank: number;
  primary_project: string | null;
  primary_activity_type: string | null;
  insight: SearchApiInsight;
}

type SearchApiHit = SearchApiExtractionHit | SearchApiInsightHit;

interface SearchApiResponse {
  results: SearchApiHit[];
}

interface SemanticSearchReference {
  capture: CaptureRecord;
  extraction: SearchApiExtraction;
}

interface SemanticSearchApiResponse {
  answer: string;
  references: SemanticSearchReference[];
  cost_cents: number | null;
  tokens_used: number | null;
}

export interface SearchRequest {
  query: string;
  app?: string | null;
  project?: string | null;
  activityType?: string | null;
  from?: string | null;
  to?: string | null;
  limit?: number;
}

export interface SemanticSearchRequest {
  query: string;
  from?: string | null;
  to?: string | null;
  limit?: number;
}

export interface SearchResult {
  sourceType: 'extraction' | 'insight';
  timestamp: string;
  rank: number;
  relevance: number;
  primaryProject: string | null;
  primaryActivityType: string | null;
  narrative: string | null;
  batchNarrative: string | null;
  capture: CaptureRecord | null;
  extraction: SearchApiExtraction | null;
  insight: SearchApiInsight | null;
}

export interface SemanticSearchResult {
  answer: string;
  references: SearchResult[];
  costCents: number | null;
  tokensUsed: number | null;
}

type SearchResultBase = Omit<SearchResult, 'relevance'>;

interface ProjectsResponse {
  projects: Array<{
    project: string | null;
    capture_count: number;
  }>;
}

const DEFAULT_LIMIT = 80;
const MAX_LIMIT = 200;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function isStringOrNull(value: unknown): value is string | null {
  return typeof value === 'string' || value === null;
}

function isFiniteNumberOrNull(value: unknown): value is number | null {
  return (typeof value === 'number' && Number.isFinite(value)) || value === null;
}

function isStringArray(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((entry) => typeof entry === 'string');
}

function isCaptureRecord(value: unknown): value is CaptureRecord {
  if (!isRecord(value)) {
    return false;
  }

  return (
    typeof value.id === 'number' &&
    typeof value.timestamp === 'string' &&
    (typeof value.app_name === 'string' || value.app_name === null || value.app_name === undefined) &&
    (typeof value.window_title === 'string' || value.window_title === null || value.window_title === undefined)
  );
}

function isSearchApiExtraction(value: unknown): value is SearchApiExtraction {
  if (!isRecord(value)) {
    return false;
  }

  return (
    typeof value.id === 'number' &&
    typeof value.capture_id === 'number' &&
    typeof value.batch_id === 'string' &&
    isStringOrNull(value.activity_type) &&
    isStringOrNull(value.description) &&
    isStringOrNull(value.app_context) &&
    isStringOrNull(value.project) &&
    isStringArray(value.topics) &&
    isStringArray(value.people) &&
    isStringOrNull(value.key_content) &&
    isStringOrNull(value.sentiment) &&
    typeof value.created_at === 'string'
  );
}

function isSearchApiInsight(value: unknown): value is SearchApiInsight {
  if (!isRecord(value)) {
    return false;
  }

  return (
    typeof value.id === 'number' &&
    typeof value.insight_type === 'string' &&
    typeof value.window_start === 'string' &&
    typeof value.window_end === 'string' &&
    typeof value.narrative === 'string'
  );
}

function isSearchApiHit(value: unknown): value is SearchApiHit {
  if (
    !isRecord(value) ||
    typeof value.source_type !== 'string' ||
    typeof value.timestamp !== 'string' ||
    typeof value.rank !== 'number'
  ) {
    return false;
  }

  if (value.source_type === 'extraction') {
    return (
      isCaptureRecord(value.capture) &&
      isSearchApiExtraction(value.extraction) &&
      (typeof value.batch_narrative === 'string' || value.batch_narrative === null || value.batch_narrative === undefined)
    );
  }

  if (value.source_type === 'insight') {
    return (
      isStringOrNull(value.primary_project) &&
      isStringOrNull(value.primary_activity_type) &&
      isSearchApiInsight(value.insight)
    );
  }

  return false;
}

function isSearchApiResponse(value: unknown): value is SearchApiResponse {
  if (!isRecord(value)) {
    return false;
  }

  return Array.isArray(value.results) && value.results.every((result) => isSearchApiHit(result));
}

function isSemanticSearchReference(value: unknown): value is SemanticSearchReference {
  if (!isRecord(value)) {
    return false;
  }

  return isCaptureRecord(value.capture) && isSearchApiExtraction(value.extraction);
}

function isSemanticSearchApiResponse(value: unknown): value is SemanticSearchApiResponse {
  if (!isRecord(value)) {
    return false;
  }

  return (
    typeof value.answer === 'string' &&
    Array.isArray(value.references) &&
    value.references.every((reference) => isSemanticSearchReference(reference)) &&
    isFiniteNumberOrNull(value.cost_cents) &&
    isFiniteNumberOrNull(value.tokens_used)
  );
}

function normalizeLimit(limit: number | undefined): number {
  if (limit === undefined) {
    return DEFAULT_LIMIT;
  }

  if (!Number.isFinite(limit)) {
    return DEFAULT_LIMIT;
  }

  return Math.max(1, Math.min(MAX_LIMIT, Math.trunc(limit)));
}

function normalizeHit(hit: SearchApiHit): SearchResultBase {
  if (hit.source_type === 'extraction') {
    return {
      sourceType: 'extraction',
      timestamp: hit.timestamp,
      rank: hit.rank,
      primaryProject: hit.extraction.project?.trim() || null,
      primaryActivityType: hit.extraction.activity_type?.trim() || null,
      narrative: hit.extraction.description?.trim() || hit.extraction.key_content?.trim() || hit.batch_narrative?.trim() || null,
      batchNarrative: hit.batch_narrative ?? null,
      capture: hit.capture,
      extraction: hit.extraction,
      insight: null,
    };
  }

  return {
    sourceType: 'insight',
    timestamp: hit.timestamp,
    rank: hit.rank,
    primaryProject: hit.primary_project?.trim() || null,
    primaryActivityType: hit.primary_activity_type?.trim() || null,
    narrative: hit.insight.narrative?.trim() || null,
    batchNarrative: null,
    capture: null,
    extraction: null,
    insight: hit.insight,
  };
}

function normalizeSemanticReference(reference: SemanticSearchReference, rank: number): SearchResultBase {
  return {
    sourceType: 'extraction',
    timestamp: reference.capture.timestamp || reference.extraction.created_at,
    rank,
    primaryProject: reference.extraction.project?.trim() || null,
    primaryActivityType: reference.extraction.activity_type?.trim() || null,
    narrative: reference.extraction.description?.trim() || reference.extraction.key_content?.trim() || null,
    batchNarrative: null,
    capture: reference.capture,
    extraction: reference.extraction,
    insight: null,
  };
}

function computeRelevance(sortedByRank: SearchResultBase[]): SearchResult[] {
  if (sortedByRank.length === 0) {
    return [];
  }

  const bestRank = sortedByRank[0].rank;
  const worstRank = sortedByRank[sortedByRank.length - 1].rank;
  const rankSpread = Math.max(0, worstRank - bestRank);

  return sortedByRank.map((hit, index) => {
    const relevance =
      rankSpread <= Number.EPSILON
        ? Math.max(60, 100 - index * 4)
        : 100 - ((hit.rank - bestRank) / rankSpread) * 45;

    return {
      ...hit,
      relevance: Math.max(1, Math.min(100, Math.round(relevance))),
    } satisfies SearchResult;
  });
}

export async function searchCaptures(
  request: SearchRequest,
  signal?: AbortSignal
): Promise<SearchResult[]> {
  const query = request.query.trim();
  if (!query) {
    return [];
  }

  const params = new URLSearchParams({
    q: query,
    limit: String(normalizeLimit(request.limit)),
  });

  if (request.app?.trim()) {
    params.set('app', request.app.trim());
  }

  if (request.project?.trim()) {
    params.set('project', request.project.trim());
  }

  if (request.activityType?.trim()) {
    params.set('activity_type', request.activityType.trim());
  }

  if (request.from?.trim()) {
    params.set('from', request.from.trim());
  }

  if (request.to?.trim()) {
    params.set('to', request.to.trim());
  }

  const response = await fetch(`/api/search?${params.toString()}`, {
    headers: {
      Accept: 'application/json',
    },
    signal,
  });

  if (!response.ok) {
    throw new Error(`search request failed (${response.status})`);
  }

  const payload: unknown = await response.json();
  if (!isSearchApiResponse(payload)) {
    throw new Error('Unexpected search payload shape');
  }

  const sorted = payload.results
    .map(normalizeHit)
    .sort((left, right) => {
      if (left.rank !== right.rank) {
        return left.rank - right.rank;
      }

      const leftTime = new Date(left.timestamp).getTime();
      const rightTime = new Date(right.timestamp).getTime();
      return rightTime - leftTime;
    });

  return computeRelevance(sorted);
}

export async function searchSemanticCaptures(
  request: SemanticSearchRequest,
  signal?: AbortSignal
): Promise<SemanticSearchResult> {
  const query = request.query.trim();
  if (!query) {
    return {
      answer: '',
      references: [],
      costCents: null,
      tokensUsed: null,
    };
  }

  const params = new URLSearchParams({
    q: query,
    limit: String(normalizeLimit(request.limit)),
  });

  if (request.from?.trim()) {
    params.set('from', request.from.trim());
  }

  if (request.to?.trim()) {
    params.set('to', request.to.trim());
  }

  const response = await fetch(`/api/search/semantic?${params.toString()}`, {
    headers: {
      Accept: 'application/json',
    },
    signal,
  });

  if (!response.ok) {
    throw new Error(`semantic search request failed (${response.status})`);
  }

  const payload: unknown = await response.json();
  if (!isSemanticSearchApiResponse(payload)) {
    throw new Error('Unexpected semantic search payload shape');
  }

  const references = computeRelevance(
    payload.references.map((reference, index) => normalizeSemanticReference(reference, index + 1))
  );

  return {
    answer: payload.answer.trim(),
    references,
    costCents: payload.cost_cents,
    tokensUsed: payload.tokens_used,
  };
}

export function collectFacetValues(results: SearchResult[]): { apps: string[]; projects: string[] } {
  const apps = new Set<string>();
  const projects = new Set<string>();

  for (const result of results) {
    const appName = result.capture?.app_name?.trim();
    if (appName) {
      apps.add(appName);
    }

    const project = result.primaryProject?.trim();
    if (project) {
      projects.add(project);
    }
  }

  return {
    apps: [...apps],
    projects: [...projects],
  };
}

function isProjectsResponse(value: unknown): value is ProjectsResponse {
  if (!isRecord(value) || !Array.isArray(value.projects)) {
    return false;
  }

  return value.projects.every((entry) => {
    if (!isRecord(entry)) {
      return false;
    }

    return (
      (typeof entry.project === 'string' || entry.project === null || entry.project === undefined) &&
      typeof entry.capture_count === 'number'
    );
  });
}

export async function listProjectFilters(from: string | null): Promise<string[]> {
  const params = new URLSearchParams();
  if (from) {
    params.set('from', from);
  }

  const response = await fetch(`/api/insights/projects?${params.toString()}`, {
    headers: {
      Accept: 'application/json',
    },
  });

  if (!response.ok) {
    throw new Error(`projects request failed (${response.status})`);
  }

  const payload: unknown = await response.json();
  if (!isProjectsResponse(payload)) {
    throw new Error('Unexpected projects payload shape');
  }

  return payload.projects
    .map((entry) => entry.project?.trim() ?? '')
    .filter((entry): entry is string => Boolean(entry));
}
