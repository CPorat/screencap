import { Y as attr, Z as escape_html, a as attr_class, d as ensure_array_like, f as head } from "../../../chunks/index-server.js";
import "../../../chunks/api.js";
//#region src/lib/components/Settings/SettingsControlDeck.svelte
function SettingsControlDeck($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let daemonOnline, storageUsed, daemonUptime, activeToday;
		const CLI_COMMANDS = [{
			label: "Prune stale captures",
			command: "screencap prune --older-than 30d",
			detail: "Reclaims screenshot + extraction storage older than your retention window."
		}, {
			label: "Export daily summary",
			command: "screencap export --date $(date +%F)",
			detail: "Generates markdown output for today’s synthesis and prints to stdout."
		}];
		const EMPTY_STATS = {
			capture_count: 0,
			captures_today: 0,
			storage_bytes: 0,
			uptime_secs: 0
		};
		const EMPTY_HEALTH = {
			status: "offline",
			uptime_secs: 0
		};
		let loading = true;
		let refreshing = false;
		let stats = EMPTY_STATS;
		let health = EMPTY_HEALTH;
		let lastUpdated = null;
		function secondsSinceLocalMidnight() {
			const now = /* @__PURE__ */ new Date();
			const midnight = new Date(now);
			midnight.setHours(0, 0, 0, 0);
			return Math.max(0, Math.floor((now.getTime() - midnight.getTime()) / 1e3));
		}
		function formatDuration(totalSeconds) {
			const safeSeconds = Number.isFinite(totalSeconds) ? Math.max(0, Math.floor(totalSeconds)) : 0;
			const hours = Math.floor(safeSeconds / 3600);
			const minutes = Math.floor(safeSeconds % 3600 / 60);
			if (hours > 0) return `${hours}h ${minutes}m`;
			return `${Math.max(minutes, 0)}m`;
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
			return `${(bytes / 1024 ** exponent).toFixed(exponent >= 2 ? 1 : 0)} ${units[exponent]}`;
		}
		function formatLastUpdated(value) {
			if (!value) return "—";
			return new Intl.DateTimeFormat(void 0, {
				hour: "2-digit",
				minute: "2-digit",
				second: "2-digit"
			}).format(value);
		}
		$: daemonOnline = health.status.toLowerCase() === "ok";
		$: storageUsed = formatBytes(stats.storage_bytes);
		$: daemonUptime = formatDuration(health.uptime_secs);
		$: activeToday = formatDuration(daemonOnline ? Math.min(health.uptime_secs, secondsSinceLocalMidnight()) : 0);
		$$renderer.push(`<section class="deck svelte-10fhd2f"${attr("aria-busy", loading)}><header class="deck__header svelte-10fhd2f"><div><p class="deck__eyebrow svelte-10fhd2f">Operations</p> <h2 class="svelte-10fhd2f">Settings command deck</h2> <p class="deck__summary svelte-10fhd2f">Live daemon telemetry from <code class="svelte-10fhd2f">/api/health</code> and <code class="svelte-10fhd2f">/api/stats</code>, plus quick
        maintenance commands.</p></div> <button class="deck__refresh svelte-10fhd2f" type="button"${attr("disabled", refreshing, true)}>${escape_html("Refresh signal")}</button></header> `);
		$$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--> <div class="stats-grid svelte-10fhd2f"><article class="metric svelte-10fhd2f"><p class="svelte-10fhd2f">Status</p> <strong${attr_class("svelte-10fhd2f", void 0, { "offline": !daemonOnline })}>${escape_html(daemonOnline ? "Online" : "Offline")}</strong> <small class="svelte-10fhd2f">Health: ${escape_html(health.status)}</small></article> <article class="metric svelte-10fhd2f"><p class="svelte-10fhd2f">Total storage</p> <strong class="svelte-10fhd2f">${escape_html(storageUsed)}</strong> <small class="svelte-10fhd2f">${escape_html(stats.storage_bytes.toLocaleString())} bytes on disk</small></article> <article class="metric svelte-10fhd2f"><p class="svelte-10fhd2f">Active time today</p> <strong class="svelte-10fhd2f">${escape_html(activeToday)}</strong> <small class="svelte-10fhd2f">Daemon online window since local midnight</small></article> <article class="metric svelte-10fhd2f"><p class="svelte-10fhd2f">Daemon uptime</p> <strong class="svelte-10fhd2f">${escape_html(daemonUptime)}</strong> <small class="svelte-10fhd2f">Last updated ${escape_html(formatLastUpdated(lastUpdated))}</small></article></div> <div class="detail-grid svelte-10fhd2f"><article class="detail-card svelte-10fhd2f"><header class="svelte-10fhd2f"><h3 class="svelte-10fhd2f">Edit config.toml</h3> <p class="svelte-10fhd2f">Applied at daemon start</p></header> <ol class="svelte-10fhd2f"><li>Open <code class="svelte-10fhd2f">~/.screencap/config.toml</code> in your editor.</li> <li>Adjust capture cadence, exclusions, provider keys, or retention limits.</li> <li>Restart daemon so new values are loaded.</li></ol> <div class="detail-card__commands svelte-10fhd2f"><code class="svelte-10fhd2f">open -e ~/.screencap/config.toml</code> <code class="svelte-10fhd2f">screencap stop &amp;&amp; screencap start</code></div></article> <article class="detail-card detail-card--commands svelte-10fhd2f"><header class="svelte-10fhd2f"><h3 class="svelte-10fhd2f">CLI maintenance</h3> <p class="svelte-10fhd2f">Operational shortcuts</p></header> <ul class="svelte-10fhd2f"><!--[-->`);
		const each_array = ensure_array_like(CLI_COMMANDS);
		for (let $$index = 0, $$length = each_array.length; $$index < $$length; $$index++) {
			let item = each_array[$$index];
			$$renderer.push(`<li class="svelte-10fhd2f"><div><strong class="svelte-10fhd2f">${escape_html(item.label)}</strong> <p class="svelte-10fhd2f">${escape_html(item.detail)}</p> <code class="svelte-10fhd2f">${escape_html(item.command)}</code></div> <button type="button" class="svelte-10fhd2f">Copy</button></li>`);
		}
		$$renderer.push(`<!--]--></ul></article></div> <footer class="deck__footer svelte-10fhd2f"><p>Captures today <strong class="svelte-10fhd2f">${escape_html(stats.captures_today.toLocaleString())}</strong></p> <p>Lifetime captures <strong class="svelte-10fhd2f">${escape_html(stats.capture_count.toLocaleString())}</strong></p></footer> `);
		$$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--></section>`);
	});
}
//#endregion
//#region src/routes/settings/+page.svelte
function _page($$renderer) {
	head("1i19ct2", $$renderer, ($$renderer) => {
		$$renderer.title(($$renderer) => {
			$$renderer.push(`<title>Screencap · Settings</title>`);
		});
	});
	SettingsControlDeck($$renderer, {});
}
//#endregion
export { _page as default };
