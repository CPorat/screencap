import type { CaptureRecord } from '$lib/api';

interface SearchApiExtraction {
  activity_type: string | null;
  description: string | null;
  app_context: string | null;
  project: string | null;
  topics: string[];
  people: string[];
  key_content: string | null;
  sentiment: string | null;
}

interface SearchApiHit {
  capture: CaptureRecord;
  extraction: SearchApiExtraction;
  batch_narrative: string | null;
  rank: number;
}

interface SearchApiResponse {
  results: SearchApiHit[];
}

export interface SearchRequest {
  query: string;
  app?: string | null;
  project?: string | null;
  from?: string | null;
  to?: string | null;
  limit?: number;

}

export interface SearchResult {
  capture: CaptureRecord;
  extraction: SearchApiExtraction;
  batchNarrative: string | null;
  rank: number;
  relevance: number;
}

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
    isStringOrNull(value.activity_type) &&
    isStringOrNull(value.description) &&
    isStringOrNull(value.app_context) &&
    isStringOrNull(value.project) &&
    isStringArray(value.topics) &&
    isStringArray(value.people) &&
    isStringOrNull(value.key_content) &&
    isStringOrNull(value.sentiment)
  );
}

function isSearchApiHit(value: unknown): value is SearchApiHit {
  if (!isRecord(value)) {
    return false;
  }

  return (
    isCaptureRecord(value.capture) &&
    isSearchApiExtraction(value.extraction) &&
    (typeof value.batch_narrative === 'string' || value.batch_narrative === null || value.batch_narrative === undefined) &&
    typeof value.rank === 'number'
  );
}

function isSearchApiResponse(value: unknown): value is SearchApiResponse {
  if (!isRecord(value)) {
    return false;
  }

  return Array.isArray(value.results) && value.results.every((result) => isSearchApiHit(result));
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

function computeRelevance(sortedByRank: SearchApiHit[]): SearchResult[] {
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
      capture: hit.capture,
      extraction: hit.extraction,
      batchNarrative: hit.batch_narrative ?? null,
      rank: hit.rank,
      relevance: Math.max(1, Math.min(100, Math.round(relevance))),
    } satisfies SearchResult;
  });
}

export async function searchCaptures(request: SearchRequest): Promise<SearchResult[]> {
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
  });

  if (!response.ok) {
    throw new Error(`search request failed (${response.status})`);
  }

  const payload: unknown = await response.json();
  if (!isSearchApiResponse(payload)) {
    throw new Error('Unexpected search payload shape');
  }

  const sorted = [...payload.results].sort((left, right) => {
    if (left.rank !== right.rank) {
      return left.rank - right.rank;
    }

    const leftTime = new Date(left.capture.timestamp).getTime();
    const rightTime = new Date(right.capture.timestamp).getTime();
    return rightTime - leftTime;
  });

  return computeRelevance(sorted);
}

export function collectFacetValues(results: SearchResult[]): { apps: string[]; projects: string[] } {
  const apps = new Set<string>();
  const projects = new Set<string>();

  for (const result of results) {
    const appName = result.capture.app_name?.trim();
    if (appName) {
      apps.add(appName);
    }

    const project = result.extraction.project?.trim();
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
