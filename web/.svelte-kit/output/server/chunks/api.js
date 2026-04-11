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
function isDailyInsight(value) {
	if (!isRecord(value) || !isRecord(value.data)) return false;
	return typeof value.id === "number" && (typeof value.narrative === "string" || value.narrative === null || value.narrative === void 0);
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
async function listCaptures(limit = 60, offset = 0) {
	const params = new URLSearchParams({
		limit: String(limit),
		offset: String(offset)
	});
	const response = await fetch(`/api/captures?${params.toString()}`, { headers: { Accept: "application/json" } });
	if (!response.ok) throw new Error(`captures request failed (${response.status})`);
	const payload = await response.json();
	if (!isCaptureListResponse(payload)) throw new Error("Unexpected captures payload shape");
	return payload;
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
async function getDailyInsight(date) {
	const normalizedDate = date.trim();
	if (!normalizedDate) return null;
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
//#endregion
export { getStats as a, getHealth as i, getCaptureDetail as n, listCaptures as o, getDailyInsight as r, getApps as t };
