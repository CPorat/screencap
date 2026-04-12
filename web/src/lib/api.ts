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

export interface CaptureListOptions {
  from?: string;
  to?: string;
  app?: string;
  project?: string;
  activityType?: string;
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

export interface InsightRecord {
  id: number;
  insight_type: string;
  data: Record<string, unknown>;
  narrative?: string | null;
  window_start?: string;
  window_end?: string;
  tokens_used?: number | null;
  cost_cents?: number | null;
}

interface InsightListResponse {
  insights: InsightRecord[];
}

export interface DailyInsight extends InsightRecord {}

export interface TimeRangeOptions {
  from?: string;
  to?: string;
}

export interface DateRangeOptions {
  from: string;
  to: string;
}

export interface ProjectTimeAllocation {
  project: string | null;
  capture_count: number;
}

interface ProjectTimeAllocationResponse {
  projects: ProjectTimeAllocation[];
}

export interface TopicFrequency {
  topic: string;
  capture_count: number;
}

interface TopicFrequencyResponse {
  topics: TopicFrequency[];
}

export interface CostSummary {
  tokens_used: number;
  reported_cost_cents: number;
}

export interface DailyCostSummary extends CostSummary {
  date: string;
}

export interface CostBreakdown {
  total: CostSummary;
  extraction: CostSummary;
  synthesis: CostSummary;
  by_day: DailyCostSummary[];
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

function isInsightRecord(value: unknown): value is InsightRecord {
  if (!isRecord(value) || !isRecord(value.data)) {
    return false;
  }

  return (
    typeof value.id === 'number' &&
    typeof value.insight_type === 'string' &&
    (typeof value.narrative === 'string' || value.narrative === null || value.narrative === undefined) &&
    (typeof value.window_start === 'string' ||
      value.window_start === null ||
      value.window_start === undefined) &&
    (typeof value.window_end === 'string' || value.window_end === null || value.window_end === undefined) &&
    (typeof value.tokens_used === 'number' || value.tokens_used === null || value.tokens_used === undefined) &&
    (typeof value.cost_cents === 'number' || value.cost_cents === null || value.cost_cents === undefined)
  );
}

function isInsightListResponse(value: unknown): value is InsightListResponse {
  if (!isRecord(value)) {
    return false;
  }

  return Array.isArray(value.insights) && value.insights.every((insight) => isInsightRecord(insight));
}

function isDailyInsight(value: unknown): value is DailyInsight {
  return isInsightRecord(value);
}

function isProjectTimeAllocation(value: unknown): value is ProjectTimeAllocation {
  if (!isRecord(value)) {
    return false;
  }

  return (
    (typeof value.project === 'string' || value.project === null || value.project === undefined) &&
    typeof value.capture_count === 'number'
  );
}

function isProjectTimeAllocationResponse(value: unknown): value is ProjectTimeAllocationResponse {
  if (!isRecord(value)) {
    return false;
  }

  return Array.isArray(value.projects) && value.projects.every((entry) => isProjectTimeAllocation(entry));
}

function isTopicFrequency(value: unknown): value is TopicFrequency {
  if (!isRecord(value)) {
    return false;
  }

  return typeof value.topic === 'string' && typeof value.capture_count === 'number';
}

function isTopicFrequencyResponse(value: unknown): value is TopicFrequencyResponse {
  if (!isRecord(value)) {
    return false;
  }

  return Array.isArray(value.topics) && value.topics.every((entry) => isTopicFrequency(entry));
}

function isCostSummary(value: unknown): value is CostSummary {
  if (!isRecord(value)) {
    return false;
  }

  return typeof value.tokens_used === 'number' && typeof value.reported_cost_cents === 'number';
}

function isDailyCostSummary(value: unknown): value is DailyCostSummary {
  if (!isRecord(value)) {
    return false;
  }

  return isCostSummary(value) && typeof value.date === 'string';
}

function isCostBreakdown(value: unknown): value is CostBreakdown {
  if (!isRecord(value)) {
    return false;
  }

  return (
    isCostSummary(value.total) &&
    isCostSummary(value.extraction) &&
    isCostSummary(value.synthesis) &&
    Array.isArray(value.by_day) &&
    value.by_day.every((entry) => isDailyCostSummary(entry))
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

function normalizeDateParam(date: string): string {
  const normalizedDate = date.trim();
  if (!normalizedDate) {
    throw new Error('date is required');
  }

  return normalizedDate;
}

function appendTimeRangeParams(params: URLSearchParams, options: TimeRangeOptions): void {
  const from = options.from?.trim();
  if (from) {
    params.set('from', from);
  }

  const to = options.to?.trim();
  if (to) {
    params.set('to', to);
  }
}

function mapSearchHitToCaptureRecord(hit: SearchHitResponse): CaptureRecord {
  const { extraction } = hit;

  return {
    ...hit.capture,
    primary_activity: extraction.description ?? null,
    narrative: extraction.key_content ?? null,
    batch_narrative: hit.batch_narrative ?? null,
  };
}

export async function listCaptures(
  limit = 60,
  offset = 0,
  options: CaptureListOptions = {}
): Promise<CaptureListResponse> {
  const params = new URLSearchParams({
    limit: String(limit),
    offset: String(offset),
  });

  const from = options.from?.trim();
  if (from) {
    params.set('from', from);
  }

  const to = options.to?.trim();
  if (to) {
    params.set('to', to);
  }

  const app = options.app?.trim();
  if (app) {
    params.set('app', app);
  }

  const project = options.project?.trim();
  if (project) {
    params.set('project', project);
  }

  const activityType = options.activityType?.trim();
  if (activityType) {
    params.set('activity_type', activityType);
  }

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

export async function getCaptures(
  limit = 60,
  offset = 0,
  options: CaptureListOptions = {}
): Promise<CaptureRecord[]> {
  try {
    const payload = await listCaptures(limit, offset, options);
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

export async function getCurrentInsight(): Promise<InsightRecord | null> {
  try {
    const response = await fetch('/api/insights/current', {
      headers: {
        Accept: 'application/json',
      },
    });

    if (response.status === 404) {
      return null;
    }

    if (!response.ok) {
      throw new Error(`current insight request failed (${response.status})`);
    }

    const payload: unknown = await response.json();
    if (!isInsightRecord(payload)) {
      console.warn('Unexpected current insight payload shape', payload);
      return null;
    }

    return payload;
  } catch (error) {
    console.error('Failed to load current insight', error);
    return null;
  }
}

export async function getHourlyInsights(date: string): Promise<InsightRecord[]> {
  let normalizedDate: string;
  try {
    normalizedDate = normalizeDateParam(date);
  } catch (error) {
    console.warn('Skipping hourly insights request due to invalid date', error);
    return [];
  }

  const params = new URLSearchParams({
    date: normalizedDate,
  });

  try {
    const response = await fetch(`/api/insights/hourly?${params.toString()}`, {
      headers: {
        Accept: 'application/json',
      },
    });

    if (response.status === 404) {
      return [];
    }

    if (!response.ok) {
      throw new Error(`hourly insight request failed (${response.status})`);
    }

    const payload: unknown = await response.json();
    if (!isInsightListResponse(payload)) {
      console.warn('Unexpected hourly insight payload shape', payload);
      return [];
    }

    return payload.insights;
  } catch (error) {
    console.error('Failed to load hourly insights', error);
    return [];
  }
}

export async function getDailyInsight(date: string): Promise<DailyInsight | null> {
  let normalizedDate: string;
  try {
    normalizedDate = normalizeDateParam(date);
  } catch (error) {
    console.warn('Skipping daily insight request due to invalid date', error);
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

export async function getDailyInsightsRange(options: DateRangeOptions): Promise<DailyInsight[]> {
  let normalizedFrom: string;
  let normalizedTo: string;

  try {
    normalizedFrom = normalizeDateParam(options.from);
    normalizedTo = normalizeDateParam(options.to);
  } catch (error) {
    console.warn('Skipping daily insights range request due to invalid date range', error);
    return [];
  }

  const params = new URLSearchParams({
    from: normalizedFrom,
    to: normalizedTo,
  });

  try {
    const response = await fetch(`/api/insights/daily?${params.toString()}`, {
      headers: {
        Accept: 'application/json',
      },
    });

    if (response.status === 404) {
      return [];
    }

    if (!response.ok) {
      throw new Error(`daily insights range request failed (${response.status})`);
    }

    const payload: unknown = await response.json();
    if (!isInsightListResponse(payload)) {
      console.warn('Unexpected daily insights range payload shape', payload);
      return [];
    }

    return payload.insights;
  } catch (error) {
    console.error('Failed to load daily insights range', error);
    return [];
  }
}

export async function getProjectTimeAllocations(
  options: TimeRangeOptions = {}
): Promise<ProjectTimeAllocation[]> {
  const params = new URLSearchParams();
  appendTimeRangeParams(params, options);
  const query = params.toString();

  try {
    const response = await fetch(`/api/insights/projects${query ? `?${query}` : ''}`, {
      headers: {
        Accept: 'application/json',
      },
    });

    if (!response.ok) {
      throw new Error(`project allocation request failed (${response.status})`);
    }

    const payload: unknown = await response.json();
    if (!isProjectTimeAllocationResponse(payload)) {
      console.warn('Unexpected project allocation payload shape', payload);
      return [];
    }

    return payload.projects;
  } catch (error) {
    console.error('Failed to load project allocations', error);
    return [];
  }
}

export async function getTopicFrequencies(options: TimeRangeOptions = {}): Promise<TopicFrequency[]> {
  const params = new URLSearchParams();
  appendTimeRangeParams(params, options);
  const query = params.toString();

  try {
    const response = await fetch(`/api/insights/topics${query ? `?${query}` : ''}`, {
      headers: {
        Accept: 'application/json',
      },
    });

    if (!response.ok) {
      throw new Error(`topic frequency request failed (${response.status})`);
    }

    const payload: unknown = await response.json();
    if (!isTopicFrequencyResponse(payload)) {
      console.warn('Unexpected topic frequency payload shape', payload);
      return [];
    }

    return payload.topics;
  } catch (error) {
    console.error('Failed to load topic frequencies', error);
    return [];
  }
}

export async function listCapturesInRange(
  options: CaptureListOptions = {},
  pageSize = 500,
  maxPages = 20
): Promise<CaptureRecord[]> {
  const normalizedPageSize = Math.max(1, Math.min(500, Math.trunc(pageSize) || 500));
  const normalizedMaxPages = Math.max(1, Math.trunc(maxPages) || 1);

  try {
    const captures: CaptureRecord[] = [];
    let offset = 0;

    for (let page = 0; page < normalizedMaxPages; page += 1) {
      const payload = await listCaptures(normalizedPageSize, offset, options);
      captures.push(...payload.captures);

      if (payload.captures.length < normalizedPageSize) {
        break;
      }

      offset += payload.captures.length;
    }

    return captures;
  } catch (error) {
    console.error('Failed to load captures in range', error);
    return [];
  }
}

export async function getCosts(options: TimeRangeOptions = {}): Promise<CostBreakdown | null> {
  const params = new URLSearchParams();
  appendTimeRangeParams(params, options);
  const query = params.toString();

  try {
    const response = await fetch(`/api/costs${query ? `?${query}` : ''}`, {
      headers: {
        Accept: 'application/json',
      },
    });

    if (response.status === 404) {
      return null;
    }

    if (!response.ok) {
      throw new Error(`costs request failed (${response.status})`);
    }

    const payload: unknown = await response.json();
    if (!isCostBreakdown(payload)) {
      console.warn('Unexpected cost payload shape', payload);
      return null;
    }

    return payload;
  } catch (error) {
    console.error('Failed to load costs', error);
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
