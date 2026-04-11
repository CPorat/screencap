export type ExtractionStatus = 'pending' | 'processed' | 'failed';

export interface CaptureRecord {
  id: number;
  timestamp: string;
  app_name: string | null;
  window_title: string | null;
  bundle_id?: string | null;
  display_id?: number | null;
  screenshot_url: string | null;
  screenshot_path?: string | null;
  extraction_status?: ExtractionStatus | null;
  extraction_id?: number | null;
  created_at?: string;
  primary_activity?: string | null;
  narrative?: string | null;
  batch_narrative?: string | null;
}

export interface ExtractionRecord {
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

export interface CaptureDetailResponse {
  capture: CaptureRecord;
  extraction: ExtractionRecord | null;
}

export interface CaptureListResponse {
  captures: CaptureRecord[];
  limit: number;
  offset: number;
}

interface SearchHitResponse {
  capture: CaptureRecord;
  extraction: {
    description?: string | null;
    key_content?: string | null;
  };
  batch_narrative?: string | null;
}

interface SearchResponse {
  results: SearchHitResponse[];
}

export interface SystemStats {
  capture_count: number;
  captures_today: number;
  storage_bytes: number;
  uptime_secs: number;
}

export interface HealthResponse {
  status: string;
  uptime_secs: number;
}

export interface AppUsage {
  app_name: string;
  capture_count: number;
}

interface AppsResponse {
  apps: AppUsage[];
}

export interface DailyInsight {
  id: number;
  narrative?: string | null;
  data: Record<string, unknown>;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function isCaptureRecord(value: unknown): value is CaptureRecord {
  if (typeof value !== 'object' || value === null) {
    return false;
  }

  const capture = value as Record<string, unknown>;
  return (
    typeof capture.id === 'number' &&
    typeof capture.timestamp === 'string' &&
    (typeof capture.app_name === 'string' || capture.app_name === null || capture.app_name === undefined) &&
    (typeof capture.window_title === 'string' ||
      capture.window_title === null ||
      capture.window_title === undefined)
  );
}

function isCaptureListResponse(value: unknown): value is CaptureListResponse {
  if (typeof value !== 'object' || value === null) {
    return false;
  }

  const payload = value as Record<string, unknown>;
  return (
    Array.isArray(payload.captures) &&
    payload.captures.every((capture) => isCaptureRecord(capture)) &&
    typeof payload.limit === 'number' &&
    typeof payload.offset === 'number'
  );
}

function isExtractionRecord(value: unknown): value is ExtractionRecord {
  if (!isRecord(value)) {
    return false;
  }

  return (
    typeof value.id === 'number' &&
    typeof value.capture_id === 'number' &&
    typeof value.batch_id === 'string' &&
    (typeof value.activity_type === 'string' || value.activity_type === null) &&
    (typeof value.description === 'string' || value.description === null) &&
    (typeof value.app_context === 'string' || value.app_context === null) &&
    (typeof value.project === 'string' || value.project === null) &&
    Array.isArray(value.topics) &&
    value.topics.every((topic) => typeof topic === 'string') &&
    Array.isArray(value.people) &&
    value.people.every((person) => typeof person === 'string') &&
    (typeof value.key_content === 'string' || value.key_content === null) &&
    (typeof value.sentiment === 'string' || value.sentiment === null) &&
    typeof value.created_at === 'string'
  );
}

function isCaptureDetailResponse(value: unknown): value is CaptureDetailResponse {
  if (!isRecord(value)) {
    return false;
  }

  return isCaptureRecord(value.capture) && (value.extraction === null || isExtractionRecord(value.extraction));
}

function isSearchHitResponse(value: unknown): value is SearchHitResponse {
  if (typeof value !== 'object' || value === null) {
    return false;
  }

  const hit = value as Record<string, unknown>;
  if (!isCaptureRecord(hit.capture)) {
    return false;
  }

  if (typeof hit.extraction !== 'object' || hit.extraction === null) {
    return false;
  }

  return true;
}

function isSearchResponse(value: unknown): value is SearchResponse {
  if (typeof value !== 'object' || value === null) {
    return false;
  }

  const payload = value as Record<string, unknown>;
  return Array.isArray(payload.results) && payload.results.every((result) => isSearchHitResponse(result));
}

function isSystemStats(value: unknown): value is SystemStats {
  if (!isRecord(value)) {
    return false;
  }

  return (
    typeof value.capture_count === 'number' &&
    typeof value.captures_today === 'number' &&
    typeof value.storage_bytes === 'number' &&
    typeof value.uptime_secs === 'number'
  );
}

function isHealthResponse(value: unknown): value is HealthResponse {
  if (!isRecord(value)) {
    return false;
  }

  return typeof value.status === 'string' && typeof value.uptime_secs === 'number';
}

function isAppUsage(value: unknown): value is AppUsage {
  if (!isRecord(value)) {
    return false;
  }

  return typeof value.app_name === 'string' && typeof value.capture_count === 'number';
}

function isAppsResponse(value: unknown): value is AppsResponse {
  if (!isRecord(value)) {
    return false;
  }

  return Array.isArray(value.apps) && value.apps.every((app) => isAppUsage(app));
}

function isDailyInsight(value: unknown): value is DailyInsight {
  if (!isRecord(value) || !isRecord(value.data)) {
    return false;
  }

  return (
    typeof value.id === 'number' &&
    (typeof value.narrative === 'string' || value.narrative === null || value.narrative === undefined)
  );
}

const EMPTY_STATS: SystemStats = {
  capture_count: 0,
  captures_today: 0,
  storage_bytes: 0,
  uptime_secs: 0,
};

const EMPTY_HEALTH: HealthResponse = {
  status: 'offline',
  uptime_secs: 0,
};

function mapSearchHitToCaptureRecord(hit: SearchHitResponse): CaptureRecord {
  const { extraction } = hit;

  return {
    ...hit.capture,
    primary_activity: extraction.description ?? null,
    narrative: extraction.key_content ?? null,
    batch_narrative: hit.batch_narrative ?? null,
  };
}

export async function listCaptures(limit = 60, offset = 0): Promise<CaptureListResponse> {
  const params = new URLSearchParams({
    limit: String(limit),
    offset: String(offset),
  });

  const response = await fetch(`/api/captures?${params.toString()}`, {
    headers: {
      Accept: 'application/json',
    },
  });

  if (!response.ok) {
    throw new Error(`captures request failed (${response.status})`);
  }

  const payload: unknown = await response.json();
  if (!isCaptureListResponse(payload)) {
    throw new Error('Unexpected captures payload shape');
  }

  return payload;
}

export async function getCaptures(limit = 60, offset = 0): Promise<CaptureRecord[]> {
  try {
    const payload = await listCaptures(limit, offset);
    return payload.captures;
  } catch (error) {
    console.error('Failed to load captures', error);
    return [];
  }
}

export async function getCaptureDetail(id: number): Promise<CaptureDetailResponse | null> {
  try {
    const response = await fetch(`/api/captures/${id}`, {
      headers: {
        Accept: 'application/json',
      },
    });

    if (response.status === 404) {
      return null;
    }

    if (!response.ok) {
      throw new Error(`capture detail request failed (${response.status})`);
    }

    const payload: unknown = await response.json();
    if (!isCaptureDetailResponse(payload)) {
      throw new Error('Unexpected capture detail payload shape');
    }

    return payload;
  } catch (error) {
    console.warn(`Failed to load capture detail for ${id}`, error);
    return null;
  }
}

export async function getStats(): Promise<SystemStats> {
  try {
    const response = await fetch('/api/stats', {
      headers: {
        Accept: 'application/json',
      },
    });

    if (!response.ok) {
      throw new Error(`stats request failed (${response.status})`);
    }

    const payload: unknown = await response.json();
    if (!isSystemStats(payload)) {
      console.warn('Unexpected stats payload shape', payload);
      return EMPTY_STATS;
    }

    return payload;
  } catch (error) {
    console.error('Failed to load stats', error);
    return EMPTY_STATS;
  }
}

export async function getHealth(): Promise<HealthResponse> {
  try {
    const response = await fetch('/api/health', {
      headers: {
        Accept: 'application/json',
      },
    });

    if (!response.ok) {
      throw new Error(`health request failed (${response.status})`);
    }

    const payload: unknown = await response.json();
    if (!isHealthResponse(payload)) {
      console.warn('Unexpected health payload shape', payload);
      return EMPTY_HEALTH;
    }

    return payload;
  } catch (error) {
    console.error('Failed to load health', error);
    return EMPTY_HEALTH;
  }
}

export async function getApps(): Promise<AppUsage[]> {
  try {
    const response = await fetch('/api/apps', {
      headers: {
        Accept: 'application/json',
      },
    });

    if (!response.ok) {
      throw new Error(`apps request failed (${response.status})`);
    }

    const payload: unknown = await response.json();
    if (!isAppsResponse(payload)) {
      console.warn('Unexpected apps payload shape', payload);
      return [];
    }

    return payload.apps;
  } catch (error) {
    console.error('Failed to load apps', error);
    return [];
  }
}

export async function getDailyInsight(date: string): Promise<DailyInsight | null> {
  const normalizedDate = date.trim();
  if (!normalizedDate) {
    return null;
  }

  const params = new URLSearchParams({
    date: normalizedDate,
  });

  try {
    const response = await fetch(`/api/insights/daily?${params.toString()}`, {
      headers: {
        Accept: 'application/json',
      },
    });

    if (response.status === 404) {
      return null;
    }

    if (!response.ok) {
      throw new Error(`daily insight request failed (${response.status})`);
    }

    const payload: unknown = await response.json();
    if (!isDailyInsight(payload)) {
      console.warn('Unexpected daily insight payload shape', payload);
      return null;
    }

    return payload;
  } catch (error) {
    console.error('Failed to load daily insight', error);
    return null;
  }
}

export async function searchExtractions(
  query: string,
  limit = 50,
  offset = 0
): Promise<CaptureRecord[]> {
  const trimmedQuery = query.trim();
  if (!trimmedQuery) {
    return [];
  }

  const normalizedLimit = Math.max(1, Math.trunc(limit));
  const normalizedOffset = Math.max(0, Math.trunc(offset));
  const upstreamLimit = normalizedLimit + normalizedOffset;

  const params = new URLSearchParams({
    q: trimmedQuery,
    limit: String(upstreamLimit),
    offset: String(normalizedOffset),
  });

  const response = await fetch(`/api/search?${params.toString()}`, {
    headers: {
      Accept: 'application/json',
    },
  });

  if (!response.ok) {
    throw new Error(`search request failed (${response.status})`);
  }

  const payload: unknown = await response.json();
  if (!isSearchResponse(payload)) {
    throw new Error('Unexpected search payload shape');
  }

  const mappedResults = payload.results.map(mapSearchHitToCaptureRecord);
  return mappedResults.slice(normalizedOffset, normalizedOffset + normalizedLimit);
}
