import { Q as escape_html, X as attr, p as head } from "../../../chunks/index-server.js";
import "../../../chunks/api.js";
import { Chart, registerables } from "chart.js";
//#region src/routes/Stats.svelte
function Stats($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		Chart.register(...registerables);
		const DAY_MS = 864e5;
		const EMPTY_STATS = {
			capture_count: 0,
			captures_today: 0,
			storage_bytes: 0,
			uptime_secs: 0
		};
		const EMPTY_COST_VIEW = {
			totalTokens: 0,
			totalCents: 0,
			extractionTokens: 0,
			extractionCents: 0,
			synthesisTokens: 0,
			synthesisCents: 0,
			perDayCents: 0,
			monthCents: 0,
			byDay: []
		};
		new Intl.DateTimeFormat(void 0, {
			month: "short",
			day: "numeric"
		});
		const CURRENCY = new Intl.NumberFormat(void 0, {
			style: "currency",
			currency: "USD",
			minimumFractionDigits: 2,
			maximumFractionDigits: 2
		});
		let fromDate = formatLocalDate(/* @__PURE__ */ new Date(Date.now() - 13 * DAY_MS));
		let toDate = formatLocalDate(/* @__PURE__ */ new Date());
		let loading = true;
		let stats = EMPTY_STATS;
		let projectSeries = [];
		let appSeries = [];
		let activitySeries = [];
		let heatmapSeries = [];
		let costView = EMPTY_COST_VIEW;
		let rangeCaptureCount = 0;
		let projectCanvas = null;
		let appCanvas = null;
		let activityCanvas = null;
		let projectChart = null;
		let appChart = null;
		let activityChart = null;
		function syncProjectChart(series, canvas) {
			if (!canvas || series.length === 0) {
				projectChart?.destroy();
				projectChart = null;
				return;
			}
			const config = {
				type: "bar",
				data: {
					labels: series.map((entry) => entry.label),
					datasets: [{
						label: "Captures",
						data: series.map((entry) => entry.value),
						backgroundColor: series.map(() => "rgba(112, 255, 227, 0.72)"),
						borderColor: series.map(() => "#70ffe3"),
						borderWidth: 1.2,
						borderRadius: 7
					}]
				},
				options: {
					responsive: true,
					maintainAspectRatio: false,
					animation: { duration: 280 },
					plugins: {
						legend: { display: false },
						tooltip: { callbacks: { label(context) {
							return `${Number(context.parsed.y ?? context.parsed.x ?? 0).toLocaleString()} captures`;
						} } }
					},
					scales: {
						x: {
							ticks: {
								color: "#f0e8db",
								maxRotation: 0,
								autoSkip: true,
								maxTicksLimit: 7
							},
							grid: { color: "rgba(246, 241, 231, 0.08)" }
						},
						y: {
							beginAtZero: true,
							ticks: {
								color: "#f0e8db",
								precision: 0
							},
							grid: { color: "rgba(246, 241, 231, 0.08)" }
						}
					}
				}
			};
			if (!projectChart) {
				projectChart = new Chart(canvas, config);
				return;
			}
			projectChart.data = config.data;
			projectChart.options = config.options ?? {};
			projectChart.update();
		}
		function syncAppChart(series, canvas) {
			if (!canvas || series.length === 0) {
				appChart?.destroy();
				appChart = null;
				return;
			}
			const palette = [
				"#70ffe3",
				"#ff4ea6",
				"#ffb347",
				"#7ea6ff",
				"#be8bff",
				"#7ff8a4",
				"#ffd86a",
				"#86d6ff",
				"#f589ff",
				"#ffa16c"
			];
			const config = {
				type: "doughnut",
				data: {
					labels: series.map((entry) => entry.label),
					datasets: [{
						data: series.map((entry) => entry.value),
						backgroundColor: series.map((_, index) => palette[index % palette.length]),
						borderColor: "#0b1018",
						borderWidth: 1.4
					}]
				},
				options: {
					responsive: true,
					maintainAspectRatio: false,
					animation: { duration: 280 },
					plugins: {
						legend: {
							labels: {
								color: "#f0e8db",
								boxWidth: 10
							},
							position: "bottom"
						},
						tooltip: { callbacks: { label(context) {
							const value = Number(context.raw ?? 0);
							return `${context.label}: ${value.toLocaleString()} captures`;
						} } }
					}
				}
			};
			if (!appChart) {
				appChart = new Chart(canvas, config);
				return;
			}
			appChart.data = config.data;
			appChart.options = config.options ?? {};
			appChart.update();
		}
		function syncActivityChart(series, canvas) {
			if (!canvas || series.length === 0) {
				activityChart?.destroy();
				activityChart = null;
				return;
			}
			const config = {
				type: "bar",
				data: {
					labels: series.map((entry) => entry.label),
					datasets: [{
						label: "Occurrences",
						data: series.map((entry) => entry.value),
						backgroundColor: series.map((_, index) => index % 2 === 0 ? "rgba(255, 78, 166, 0.7)" : "rgba(255, 179, 71, 0.7)"),
						borderColor: series.map((_, index) => index % 2 === 0 ? "#ff4ea6" : "#ffb347"),
						borderWidth: 1.2,
						borderRadius: 7
					}]
				},
				options: {
					indexAxis: "y",
					responsive: true,
					maintainAspectRatio: false,
					animation: { duration: 280 },
					plugins: {
						legend: { display: false },
						tooltip: { callbacks: { label(context) {
							return `${Number(context.parsed.x ?? context.parsed.y ?? 0).toLocaleString()} events`;
						} } }
					},
					scales: {
						x: {
							beginAtZero: true,
							ticks: {
								color: "#f0e8db",
								precision: 0
							},
							grid: { color: "rgba(246, 241, 231, 0.08)" }
						},
						y: {
							ticks: { color: "#f0e8db" },
							grid: { color: "rgba(246, 241, 231, 0.04)" }
						}
					}
				}
			};
			if (!activityChart) {
				activityChart = new Chart(canvas, config);
				return;
			}
			activityChart.data = config.data;
			activityChart.options = config.options ?? {};
			activityChart.update();
		}
		function buildHeatmapSlots(cells) {
			if (cells.length === 0) return [];
			const firstDate = parseDateOnly(cells[0].date);
			const leading = firstDate ? firstDate.getDay() : 0;
			return [...Array.from({ length: leading }, () => null), ...cells];
		}
		function formatLocalDate(value) {
			return `${value.getFullYear()}-${String(value.getMonth() + 1).padStart(2, "0")}-${String(value.getDate()).padStart(2, "0")}`;
		}
		function parseDateOnly(value) {
			const [yearRaw, monthRaw, dayRaw] = value.split("-").map((part) => Number(part));
			if (!Number.isFinite(yearRaw) || !Number.isFinite(monthRaw) || !Number.isFinite(dayRaw)) return null;
			const date = new Date(yearRaw, monthRaw - 1, dayRaw, 0, 0, 0, 0);
			return Number.isFinite(date.getTime()) ? date : null;
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
		function formatCost(cents) {
			const safeCents = Number.isFinite(cents) ? cents : 0;
			return CURRENCY.format(safeCents / 100);
		}
		$: syncProjectChart(projectSeries, projectCanvas);
		$: syncAppChart(appSeries, appCanvas);
		$: syncActivityChart(activitySeries, activityCanvas);
		$: buildHeatmapSlots(heatmapSeries);
		head("1lg4gi9", $$renderer, ($$renderer) => {
			$$renderer.title(($$renderer) => {
				$$renderer.push(`<title>Screencap · Stats</title>`);
			});
		});
		$$renderer.push(`<section class="stats svelte-1lg4gi9"${attr("aria-busy", loading)}><header class="stats__header svelte-1lg4gi9"><p class="stats__eyebrow svelte-1lg4gi9">Telemetry Deck</p> <h2 class="svelte-1lg4gi9">Ops pulse matrix</h2> <p class="stats__summary svelte-1lg4gi9">Project allocation, app usage, activity distribution, heatmap intensity, and cost telemetry in one
      range-controlled board.</p> <form class="stats__range svelte-1lg4gi9"><label class="svelte-1lg4gi9"><span>From</span> <input type="date"${attr("value", fromDate)} class="svelte-1lg4gi9"/></label> <label class="svelte-1lg4gi9"><span>To</span> <input type="date"${attr("value", toDate)} class="svelte-1lg4gi9"/></label> <button type="submit" class="svelte-1lg4gi9">Apply</button></form> <div class="stats__quick-range svelte-1lg4gi9"><button type="button" class="svelte-1lg4gi9">7D</button> <button type="button" class="svelte-1lg4gi9">30D</button> <button type="button" class="svelte-1lg4gi9">90D</button></div></header> `);
		$$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--> <section class="stats__summary-grid svelte-1lg4gi9"><article class="stat-card svelte-1lg4gi9"><p class="svelte-1lg4gi9">Total captures</p> <strong class="svelte-1lg4gi9">${escape_html(stats.capture_count.toLocaleString())}</strong> <small class="svelte-1lg4gi9">${escape_html(rangeCaptureCount.toLocaleString())} in selected range</small></article> <article class="stat-card svelte-1lg4gi9"><p class="svelte-1lg4gi9">Captured today</p> <strong class="svelte-1lg4gi9">${escape_html(stats.captures_today.toLocaleString())}</strong> <small class="svelte-1lg4gi9">daemon uptime: ${escape_html(formatUptime(stats.uptime_secs))}</small></article> <article class="stat-card svelte-1lg4gi9"><p class="svelte-1lg4gi9">Storage used</p> <strong class="svelte-1lg4gi9">${escape_html(formatBytes(stats.storage_bytes))}</strong> <small class="svelte-1lg4gi9">local screenshot + metadata footprint</small></article> <article class="stat-card svelte-1lg4gi9"><p class="svelte-1lg4gi9">Token volume</p> <strong class="svelte-1lg4gi9">${escape_html(costView.totalTokens.toLocaleString())}</strong> <small class="svelte-1lg4gi9">${escape_html(formatCost(costView.totalCents))} total reported spend</small></article></section> <section class="stats__chart-grid svelte-1lg4gi9"><article class="panel-card svelte-1lg4gi9"><header class="svelte-1lg4gi9"><h3 class="svelte-1lg4gi9">Time per project</h3> <p class="svelte-1lg4gi9">capture count proxy</p></header> <div class="chart-shell svelte-1lg4gi9">`);
		$$renderer.push("<!--[0-->");
		$$renderer.push(`<p class="panel-card__state svelte-1lg4gi9">Loading project allocation…</p>`);
		$$renderer.push(`<!--]--></div></article> <article class="panel-card svelte-1lg4gi9"><header class="svelte-1lg4gi9"><h3 class="svelte-1lg4gi9">Time per app</h3> <p class="svelte-1lg4gi9">capture distribution</p></header> <div class="chart-shell svelte-1lg4gi9">`);
		$$renderer.push("<!--[0-->");
		$$renderer.push(`<p class="panel-card__state svelte-1lg4gi9">Loading app usage…</p>`);
		$$renderer.push(`<!--]--></div></article> <article class="panel-card svelte-1lg4gi9"><header class="svelte-1lg4gi9"><h3 class="svelte-1lg4gi9">Activity breakdown</h3> <p class="svelte-1lg4gi9">daily activity signatures</p></header> <div class="chart-shell chart-shell--wide svelte-1lg4gi9">`);
		$$renderer.push("<!--[0-->");
		$$renderer.push(`<p class="panel-card__state svelte-1lg4gi9">Loading activity categories…</p>`);
		$$renderer.push(`<!--]--></div></article></section> <section class="panel-card panel-card--heatmap svelte-1lg4gi9"><header class="svelte-1lg4gi9"><h3 class="svelte-1lg4gi9">Daily active hours heatmap</h3> <p class="svelte-1lg4gi9">GitHub-style cadence map</p></header> `);
		$$renderer.push("<!--[0-->");
		$$renderer.push(`<p class="panel-card__state svelte-1lg4gi9">Rendering active-hours field…</p>`);
		$$renderer.push(`<!--]--></section> <section class="stats__cost-grid svelte-1lg4gi9"><article class="panel-card svelte-1lg4gi9"><header class="svelte-1lg4gi9"><h3 class="svelte-1lg4gi9">Cost tracking</h3> <p class="svelte-1lg4gi9">reported model cost</p></header> `);
		$$renderer.push("<!--[0-->");
		$$renderer.push(`<p class="panel-card__state svelte-1lg4gi9">Loading cost telemetry…</p>`);
		$$renderer.push(`<!--]--></article> <article class="panel-card svelte-1lg4gi9"><header class="svelte-1lg4gi9"><h3 class="svelte-1lg4gi9">Cost by day</h3> <p class="svelte-1lg4gi9">rolling daily spend</p></header> `);
		$$renderer.push("<!--[0-->");
		$$renderer.push(`<p class="panel-card__state svelte-1lg4gi9">Loading daily cost points…</p>`);
		$$renderer.push(`<!--]--></article></section></section>`);
	});
}
//#endregion
//#region src/routes/stats/+page.svelte
function _page($$renderer) {
	Stats($$renderer, {});
}
//#endregion
export { _page as default };
