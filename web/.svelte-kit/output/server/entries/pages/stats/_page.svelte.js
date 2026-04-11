import { Y as attr, Z as escape_html, f as head } from "../../../chunks/index-server.js";
import "../../../chunks/api.js";
//#region src/routes/stats/+page.svelte
function _page($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let topApps;
		const EMPTY_STATS = {
			capture_count: 0,
			captures_today: 0,
			storage_bytes: 0,
			uptime_secs: 0
		};
		let loading = true;
		let stats = EMPTY_STATS;
		let apps = [];
		let dailyInsight = null;
		let today = formatLocalDate(/* @__PURE__ */ new Date());
		function formatLocalDate(value) {
			return `${value.getFullYear()}-${String(value.getMonth() + 1).padStart(2, "0")}-${String(value.getDate()).padStart(2, "0")}`;
		}
		function formatBytes(bytes) {
			if (!Number.isFinite(bytes) || bytes <= 0) return "0 MB";
			const units = [
				"B",
				"KB",
				"MB",
				"GB",
				"TB"
			];
			const exponent = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
			const value = bytes / 1024 ** exponent;
			const precision = exponent >= 2 ? 1 : 0;
			return `${value.toFixed(precision)} ${units[exponent]}`;
		}
		function formatUptime(totalSeconds) {
			const safeSeconds = Number.isFinite(totalSeconds) ? Math.max(0, Math.floor(totalSeconds)) : 0;
			const days = Math.floor(safeSeconds / 86400);
			const hours = Math.floor(safeSeconds % 86400 / 3600);
			const minutes = Math.floor(safeSeconds % 3600 / 60);
			if (days > 0) return `${days}d ${hours}h ${minutes}m`;
			if (hours > 0) return `${hours}h ${minutes}m`;
			return `${Math.max(minutes, 1)}m`;
		}
		function extractDailySummary(insight) {
			if (!insight) return null;
			const data = insight.data;
			if (typeof data.narrative === "string" && data.narrative.trim()) return data.narrative;
			if (typeof data.summary === "string" && data.summary.trim()) return data.summary;
			if (typeof insight.narrative === "string" && insight.narrative.trim()) return insight.narrative;
			return null;
		}
		$: topApps = apps.slice(0, 8);
		$: topApps.reduce((max, app) => Math.max(max, app.capture_count), 0);
		$: extractDailySummary(dailyInsight);
		head("16pwk6k", $$renderer, ($$renderer) => {
			$$renderer.title(($$renderer) => {
				$$renderer.push(`<title>Screencap · Stats</title>`);
			});
		});
		$$renderer.push(`<section class="panel svelte-16pwk6k"${attr("aria-busy", loading)}><header class="panel__header svelte-16pwk6k"><p class="panel__section svelte-16pwk6k">Insights</p> <h2 class="svelte-16pwk6k">Daily signal board</h2> <p class="panel__summary svelte-16pwk6k">System health, app activity concentration, and the generated day summary.</p></header> <div class="stats-grid svelte-16pwk6k"><article class="stat-card svelte-16pwk6k"><p class="svelte-16pwk6k">Total captures</p> <strong class="svelte-16pwk6k">${escape_html(stats.capture_count.toLocaleString())}</strong> <small class="svelte-16pwk6k">${escape_html(stats.captures_today.toLocaleString())} captured today</small></article> <article class="stat-card svelte-16pwk6k"><p class="svelte-16pwk6k">Storage used</p> <strong class="svelte-16pwk6k">${escape_html(formatBytes(stats.storage_bytes))}</strong> <small class="svelte-16pwk6k">Screenshot and metadata footprint</small></article> <article class="stat-card svelte-16pwk6k"><p class="svelte-16pwk6k">Daemon uptime</p> <strong class="svelte-16pwk6k">${escape_html(formatUptime(stats.uptime_secs))}</strong> <small class="svelte-16pwk6k">${escape_html(stats.uptime_secs.toLocaleString())} seconds online</small></article></div> <div class="panel-grid svelte-16pwk6k"><article class="panel-card svelte-16pwk6k"><header class="svelte-16pwk6k"><h3 class="svelte-16pwk6k">Top apps</h3> <p class="svelte-16pwk6k">Most frequent app captures</p></header> `);
		$$renderer.push("<!--[0-->");
		$$renderer.push(`<p class="panel__state svelte-16pwk6k">Loading app activity…</p>`);
		$$renderer.push(`<!--]--></article> <article class="panel-card svelte-16pwk6k"><header class="svelte-16pwk6k"><h3 class="svelte-16pwk6k">Daily summary</h3> <p class="svelte-16pwk6k">${escape_html(today)}</p></header> `);
		$$renderer.push("<!--[0-->");
		$$renderer.push(`<p class="panel__state svelte-16pwk6k">Checking summary status…</p>`);
		$$renderer.push(`<!--]--></article></div></section>`);
	});
}
//#endregion
export { _page as default };
