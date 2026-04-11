//#region src/lib/api.ts
function isRecord(value) {
	return typeof value === "object" && value !== null;
}
function isCaptureRecord(value) {
	if (typeof value !== "object" || value === null) return false;
	const capture = value;
	return typeof capture.id === "number" && typeof capture.timestamp === "string" && (typeof capture.app_name === "string" || capture.app_name === null || capture.app_name === void 0) && (typeof capture.window_title === "string" || capture.window_title === null || capture.window_title === void 0);
}
function isCaptureListResponse(value) {
	if (typeof value !== "object" || value === null) return false;
	const payload = value;
	return Array.isArray(payload.captures) && payload.captures.every((capture) => isCaptureRecord(capture)) && typeof payload.limit === "number" && typeof payload.offset === "number";
}
function isExtractionRecord(value) {
	if (!isRecord(value)) return false;
	return typeof value.id === "number" && typeof value.capture_id === "number" && typeof value.batch_id === "string" && (typeof value.activity_type === "string" || value.activity_type === null) && (typeof value.description === "string" || value.description === null) && (typeof value.app_context === "string" || value.app_context === null) && (typeof value.project === "string" || value.project === null) && Array.isArray(value.topics) && value.topics.every((topic) => typeof topic === "string") && Array.isArray(value.people) && value.people.every((person) => typeof person === "string") && (typeof value.key_content === "string" || value.key_content === null) && (typeof value.sentiment === "string" || value.sentiment === null) && typeof value.created_at === "string";
}
function isCaptureDetailResponse(value) {
	if (!isRecord(value)) return false;
	return isCaptureRecord(value.capture) && (value.extraction === null || isExtractionRecord(value.extraction));
}
function isSystemStats(value) {
	if (!isRecord(value)) return false;
	return typeof value.capture_count === "number" && typeof value.captures_today === "number" && typeof value.storage_bytes === "number" && typeof value.uptime_secs === "number";
}
function isHealthResponse(value) {
	if (!isRecord(value)) return false;
	return typeof value.status === "string" && typeof value.uptime_secs === "number";
}
function isAppUsage(value) {
	if (!isRecord(value)) return false;
	return typeof value.app_name === "string" && typeof value.capture_count === "number";
}
function isAppsResponse(value) {
	if (!isRecord(value)) return false;
	return Array.isArray(value.apps) && value.apps.every((app) => isAppUsage(app));
}
function isInsightRecord(value) {
	if (!isRecord(value) || !isRecord(value.data)) return false;
	return typeof value.id === "number" && typeof value.insight_type === "string" && (typeof value.narrative === "string" || value.narrative === null || value.narrative === void 0) && (typeof value.window_start === "string" || value.window_start === null || value.window_start === void 0) && (typeof value.window_end === "string" || value.window_end === null || value.window_end === void 0) && (typeof value.tokens_used === "number" || value.tokens_used === null || value.tokens_used === void 0) && (typeof value.cost_cents === "number" || value.cost_cents === null || value.cost_cents === void 0);
}
function isInsightListResponse(value) {
	if (!isRecord(value)) return false;
	return Array.isArray(value.insights) && value.insights.every((insight) => isInsightRecord(insight));
}
function isDailyInsight(value) {
	return isInsightRecord(value);
}
function isProjectTimeAllocation(value) {
	if (!isRecord(value)) return false;
	return (typeof value.project === "string" || value.project === null || value.project === void 0) && typeof value.capture_count === "number";
}
function isProjectTimeAllocationResponse(value) {
	if (!isRecord(value)) return false;
	return Array.isArray(value.projects) && value.projects.every((entry) => isProjectTimeAllocation(entry));
}
function isTopicFrequency(value) {
	if (!isRecord(value)) return false;
	return typeof value.topic === "string" && typeof value.capture_count === "number";
}
function isTopicFrequencyResponse(value) {
	if (!isRecord(value)) return false;
	return Array.isArray(value.topics) && value.topics.every((entry) => isTopicFrequency(entry));
}
function isCostSummary(value) {
	if (!isRecord(value)) return false;
	return typeof value.tokens_used === "number" && typeof value.reported_cost_cents === "number";
}
function isDailyCostSummary(value) {
	if (!isRecord(value)) return false;
	return isCostSummary(value) && typeof value.date === "string";
}
function isCostBreakdown(value) {
	if (!isRecord(value)) return false;
	return isCostSummary(value.total) && isCostSummary(value.extraction) && isCostSummary(value.synthesis) && Array.isArray(value.by_day) && value.by_day.every((entry) => isDailyCostSummary(entry));
}
var EMPTY_STATS = {
	capture_count: 0,
	captures_today: 0,
	storage_bytes: 0,
	uptime_secs: 0
};
var EMPTY_HEALTH = {
	status: "offline",
	uptime_secs: 0
};
function normalizeDateParam(date) {
	const normalizedDate = date.trim();
	if (!normalizedDate) throw new Error("date is required");
	return normalizedDate;
}
function appendTimeRangeParams(params, options) {
	const from = options.from?.trim();
	if (from) params.set("from", from);
	const to = options.to?.trim();
	if (to) params.set("to", to);
}
async function listCaptures(limit = 60, offset = 0, options = {}) {
	const params = new URLSearchParams({
		limit: String(limit),
		offset: String(offset)
	});
	const from = options.from?.trim();
	if (from) params.set("from", from);
	const to = options.to?.trim();
	if (to) params.set("to", to);
	const app = options.app?.trim();
	if (app) params.set("app", app);
	const response = await fetch(`/api/captures?${params.toString()}`, { headers: { Accept: "application/json" } });
	if (!response.ok) throw new Error(`captures request failed (${response.status})`);
	const payload = await response.json();
	if (!isCaptureListResponse(payload)) throw new Error("Unexpected captures payload shape");
	return payload;
}
async function getCaptures(limit = 60, offset = 0, options = {}) {
	try {
		return (await listCaptures(limit, offset, options)).captures;
	} catch (error) {
		console.error("Failed to load captures", error);
		return [];
	}
}
async function getCaptureDetail(id) {
	try {
		const response = await fetch(`/api/captures/${id}`, { headers: { Accept: "application/json" } });
		if (response.status === 404) return null;
		if (!response.ok) throw new Error(`capture detail request failed (${response.status})`);
		const payload = await response.json();
		if (!isCaptureDetailResponse(payload)) throw new Error("Unexpected capture detail payload shape");
		return payload;
	} catch (error) {
		console.warn(`Failed to load capture detail for ${id}`, error);
		return null;
	}
}
async function getStats() {
	try {
		const response = await fetch("/api/stats", { headers: { Accept: "application/json" } });
		if (!response.ok) throw new Error(`stats request failed (${response.status})`);
		const payload = await response.json();
		if (!isSystemStats(payload)) {
			console.warn("Unexpected stats payload shape", payload);
			return EMPTY_STATS;
		}
		return payload;
	} catch (error) {
		console.error("Failed to load stats", error);
		return EMPTY_STATS;
	}
}
async function getHealth() {
	try {
		const response = await fetch("/api/health", { headers: { Accept: "application/json" } });
		if (!response.ok) throw new Error(`health request failed (${response.status})`);
		const payload = await response.json();
		if (!isHealthResponse(payload)) {
			console.warn("Unexpected health payload shape", payload);
			return EMPTY_HEALTH;
		}
		return payload;
	} catch (error) {
		console.error("Failed to load health", error);
		return EMPTY_HEALTH;
	}
}
async function getApps() {
	try {
		const response = await fetch("/api/apps", { headers: { Accept: "application/json" } });
		if (!response.ok) throw new Error(`apps request failed (${response.status})`);
		const payload = await response.json();
		if (!isAppsResponse(payload)) {
			console.warn("Unexpected apps payload shape", payload);
			return [];
		}
		return payload.apps;
	} catch (error) {
		console.error("Failed to load apps", error);
		return [];
	}
}
async function getCurrentInsight() {
	try {
		const response = await fetch("/api/insights/current", { headers: { Accept: "application/json" } });
		if (response.status === 404) return null;
		if (!response.ok) throw new Error(`current insight request failed (${response.status})`);
		const payload = await response.json();
		if (!isInsightRecord(payload)) {
			console.warn("Unexpected current insight payload shape", payload);
			return null;
		}
		return payload;
	} catch (error) {
		console.error("Failed to load current insight", error);
		return null;
	}
}
async function getHourlyInsights(date) {
	let normalizedDate;
	try {
		normalizedDate = normalizeDateParam(date);
	} catch (error) {
		console.warn("Skipping hourly insights request due to invalid date", error);
		return [];
	}
	const params = new URLSearchParams({ date: normalizedDate });
	try {
		const response = await fetch(`/api/insights/hourly?${params.toString()}`, { headers: { Accept: "application/json" } });
		if (response.status === 404) return [];
		if (!response.ok) throw new Error(`hourly insight request failed (${response.status})`);
		const payload = await response.json();
		if (!isInsightListResponse(payload)) {
			console.warn("Unexpected hourly insight payload shape", payload);
			return [];
		}
		return payload.insights;
	} catch (error) {
		console.error("Failed to load hourly insights", error);
		return [];
	}
}
async function getDailyInsight(date) {
	let normalizedDate;
	try {
		normalizedDate = normalizeDateParam(date);
	} catch (error) {
		console.warn("Skipping daily insight request due to invalid date", error);
		return null;
	}
	const params = new URLSearchParams({ date: normalizedDate });
	try {
		const response = await fetch(`/api/insights/daily?${params.toString()}`, { headers: { Accept: "application/json" } });
		if (response.status === 404) return null;
		if (!response.ok) throw new Error(`daily insight request failed (${response.status})`);
		const payload = await response.json();
		if (!isDailyInsight(payload)) {
			console.warn("Unexpected daily insight payload shape", payload);
			return null;
		}
		return payload;
	} catch (error) {
		console.error("Failed to load daily insight", error);
		return null;
	}
}
async function getDailyInsightsRange(options) {
	let normalizedFrom;
	let normalizedTo;
	try {
		normalizedFrom = normalizeDateParam(options.from);
		normalizedTo = normalizeDateParam(options.to);
	} catch (error) {
		console.warn("Skipping daily insights range request due to invalid date range", error);
		return [];
	}
	const params = new URLSearchParams({
		from: normalizedFrom,
		to: normalizedTo
	});
	try {
		const response = await fetch(`/api/insights/daily?${params.toString()}`, { headers: { Accept: "application/json" } });
		if (response.status === 404) return [];
		if (!response.ok) throw new Error(`daily insights range request failed (${response.status})`);
		const payload = await response.json();
		if (!isInsightListResponse(payload)) {
			console.warn("Unexpected daily insights range payload shape", payload);
			return [];
		}
		return payload.insights;
	} catch (error) {
		console.error("Failed to load daily insights range", error);
		return [];
	}
}
async function getProjectTimeAllocations(options = {}) {
	const params = new URLSearchParams();
	appendTimeRangeParams(params, options);
	const query = params.toString();
	try {
		const response = await fetch(`/api/insights/projects${query ? `?${query}` : ""}`, { headers: { Accept: "application/json" } });
		if (!response.ok) throw new Error(`project allocation request failed (${response.status})`);
		const payload = await response.json();
		if (!isProjectTimeAllocationResponse(payload)) {
			console.warn("Unexpected project allocation payload shape", payload);
			return [];
		}
		return payload.projects;
	} catch (error) {
		console.error("Failed to load project allocations", error);
		return [];
	}
}
async function getTopicFrequencies(options = {}) {
	const params = new URLSearchParams();
	appendTimeRangeParams(params, options);
	const query = params.toString();
	try {
		const response = await fetch(`/api/insights/topics${query ? `?${query}` : ""}`, { headers: { Accept: "application/json" } });
		if (!response.ok) throw new Error(`topic frequency request failed (${response.status})`);
		const payload = await response.json();
		if (!isTopicFrequencyResponse(payload)) {
			console.warn("Unexpected topic frequency payload shape", payload);
			return [];
		}
		return payload.topics;
	} catch (error) {
		console.error("Failed to load topic frequencies", error);
		return [];
	}
}
async function listCapturesInRange(options = {}, pageSize = 500, maxPages = 20) {
	const normalizedPageSize = Math.max(1, Math.min(500, Math.trunc(pageSize) || 500));
	const normalizedMaxPages = Math.max(1, Math.trunc(maxPages) || 1);
	try {
		const captures = [];
		let offset = 0;
		for (let page = 0; page < normalizedMaxPages; page += 1) {
			const payload = await listCaptures(normalizedPageSize, offset, options);
			captures.push(...payload.captures);
			if (payload.captures.length < normalizedPageSize) break;
			offset += payload.captures.length;
		}
		return captures;
	} catch (error) {
		console.error("Failed to load captures in range", error);
		return [];
	}
}
async function getCosts(options = {}) {
	const params = new URLSearchParams();
	appendTimeRangeParams(params, options);
	const query = params.toString();
	try {
		const response = await fetch(`/api/costs${query ? `?${query}` : ""}`, { headers: { Accept: "application/json" } });
		if (response.status === 404) return null;
		if (!response.ok) throw new Error(`costs request failed (${response.status})`);
		const payload = await response.json();
		if (!isCostBreakdown(payload)) {
			console.warn("Unexpected cost payload shape", payload);
			return null;
		}
		return payload;
	} catch (error) {
		console.error("Failed to load costs", error);
		return null;
	}
}
//#endregion
export { getCurrentInsight as a, getHealth as c, getStats as d, getTopicFrequencies as f, getCosts as i, getHourlyInsights as l, getCaptureDetail as n, getDailyInsight as o, listCapturesInRange as p, getCaptures as r, getDailyInsightsRange as s, getApps as t, getProjectTimeAllocations as u };
