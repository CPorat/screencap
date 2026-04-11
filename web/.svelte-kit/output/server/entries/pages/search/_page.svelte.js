import { Y as attr, Z as escape_html, a as attr_class, c as bind_props, d as ensure_array_like, f as head, it as fallback, n as onDestroy, o as attr_style } from "../../../chunks/index-server.js";
import "../../../chunks/api.js";
//#region src/lib/components/Search/SearchResultCard.svelte
function SearchResultCard($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let capturedAt, hasTimestamp, timeLabel, dateLabel, appLabel, projectLabel, appGlyph, hue, activityLabel, titleLabel, summaryLabel, screenshotSrc;
		let result = $$props["result"];
		let position = fallback($$props["position"], 1);
		const timeFormatter = new Intl.DateTimeFormat(void 0, {
			hour: "numeric",
			minute: "2-digit"
		});
		const dateFormatter = new Intl.DateTimeFormat(void 0, {
			weekday: "short",
			month: "short",
			day: "numeric"
		});
		let expanded = false;
		function toMonogram(appName) {
			const parts = appName.split(/\s+/).map((part) => part.trim()).filter(Boolean);
			if (parts.length === 0) return "??";
			return parts.slice(0, 2).map((part) => part.charAt(0).toUpperCase()).join("");
		}
		function iconHue(appName, project) {
			let hash = 0;
			const seed = `${appName}:${project}`;
			for (let index = 0; index < seed.length; index += 1) hash = (hash * 31 + seed.charCodeAt(index)) % 360;
			return hash;
		}
		function humanizeActivity(activityType) {
			return activityType.split("_").filter(Boolean).map((word) => word.charAt(0).toUpperCase() + word.slice(1)).join(" ");
		}
		function buildScreenshotSrc() {
			if (result.capture.screenshot_url?.trim()) return result.capture.screenshot_url;
			if (!result.capture.screenshot_path?.trim()) return null;
			const normalizedPath = result.capture.screenshot_path.replace(/^\/+/, "");
			return `/api/screenshots/${encodeURIComponent(normalizedPath)}`;
		}
		$: capturedAt = new Date(result.capture.timestamp);
		$: hasTimestamp = Number.isFinite(capturedAt.getTime());
		$: timeLabel = hasTimestamp ? timeFormatter.format(capturedAt) : "Unknown time";
		$: dateLabel = hasTimestamp ? dateFormatter.format(capturedAt) : "Timestamp unavailable";
		$: appLabel = result.capture.app_name?.trim() || "Unknown app";
		$: projectLabel = result.extraction.project?.trim() || "Unassigned";
		$: appGlyph = toMonogram(appLabel);
		$: hue = iconHue(appLabel, projectLabel);
		$: activityLabel = result.extraction.activity_type ? humanizeActivity(result.extraction.activity_type) : result.extraction.description?.trim() || "Unclassified";
		$: titleLabel = result.capture.window_title?.trim() || "Untitled window";
		$: summaryLabel = result.extraction.key_content?.trim() || result.extraction.description?.trim() || result.batchNarrative?.trim() || "No summary returned for this result.";
		$: result.extraction.topics;
		$: result.extraction.people;
		$: screenshotSrc = buildScreenshotSrc();
		$$renderer.push(`<article class="capture-card svelte-116ub32"${attr("aria-label", `Search hit ${position} from ${timeLabel}`)}><header class="capture-card__header svelte-116ub32"><div><p class="capture-card__rank svelte-116ub32">#${escape_html(position)}</p> <p class="capture-card__score svelte-116ub32">${escape_html(result.relevance)}% relevance</p></div> <div class="capture-card__time-group svelte-116ub32"><p class="capture-card__time svelte-116ub32">${escape_html(timeLabel)}</p> <p class="capture-card__date svelte-116ub32">${escape_html(dateLabel)}</p></div></header> <div class="capture-card__app-chip svelte-116ub32"><span class="capture-card__glyph svelte-116ub32" aria-hidden="true"${attr_style(`--hue:${hue}`)}>${escape_html(appGlyph)}</span> <div class="capture-card__app-copy svelte-116ub32"><p class="svelte-116ub32">${escape_html(appLabel)}</p> <small class="svelte-116ub32">${escape_html(projectLabel)}</small></div></div> <h4 class="svelte-116ub32">${escape_html(titleLabel)}</h4> <dl class="capture-card__signals svelte-116ub32"><div class="svelte-116ub32"><dt class="svelte-116ub32">Activity</dt> <dd class="svelte-116ub32">${escape_html(activityLabel)}</dd></div> <div class="svelte-116ub32"><dt class="svelte-116ub32">Summary</dt> <dd class="svelte-116ub32">${escape_html(summaryLabel)}</dd></div></dl> `);
		if (screenshotSrc) {
			$$renderer.push("<!--[0-->");
			$$renderer.push(`<img class="capture-card__thumb svelte-116ub32"${attr("src", screenshotSrc)}${attr("alt", `Thumbnail for ${titleLabel}`)} loading="lazy"/>`);
		} else {
			$$renderer.push("<!--[-1-->");
			$$renderer.push(`<div class="capture-card__placeholder svelte-116ub32" role="img" aria-label="Screenshot unavailable">Screenshot unavailable</div>`);
		}
		$$renderer.push(`<!--]--> <button class="capture-card__toggle svelte-116ub32" type="button"${attr("aria-expanded", expanded)}>${escape_html("Expand details")}</button> `);
		$$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--></article>`);
		bind_props($$props, {
			result,
			position
		});
	});
}
function collectFacetValues(results) {
	const apps = /* @__PURE__ */ new Set();
	const projects = /* @__PURE__ */ new Set();
	for (const result of results) {
		const appName = result.capture.app_name?.trim();
		if (appName) apps.add(appName);
		const project = result.extraction.project?.trim();
		if (project) projects.add(project);
	}
	return {
		apps: [...apps],
		projects: [...projects]
	};
}
//#endregion
//#region src/lib/components/Search/SearchWorkspace.svelte
function SearchWorkspace($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let facets, appChips, projectChips, queryPreview;
		const timeRangeOptions = [
			{
				id: "all",
				label: "All time",
				description: "Search across every indexed extraction.",
				hours: null
			},
			{
				id: "24h",
				label: "24h",
				description: "Only the most recent day of activity.",
				hours: 24
			},
			{
				id: "72h",
				label: "3d",
				description: "Last 72 hours of captures and extraction notes.",
				hours: 72
			},
			{
				id: "168h",
				label: "7d",
				description: "Last week of indexed context.",
				hours: 168
			},
			{
				id: "720h",
				label: "30d",
				description: "Search up to one month back.",
				hours: 720
			}
		];
		const quickPrompts = [
			"sprint planning",
			"incident follow-up",
			"debugging",
			"PR review"
		];
		let query = "";
		let loading = false;
		let selectedApp = null;
		let selectedProject = null;
		let selectedWindow = "168h";
		let results = [];
		let appSuggestions = [];
		let projectSuggestions = [];
		onDestroy(() => {});
		function mergeFacetSuggestions(primary, secondary, selected) {
			const merged = /* @__PURE__ */ new Set();
			if (selected?.trim()) merged.add(selected.trim());
			for (const value of primary) {
				const trimmed = value.trim();
				if (trimmed) merged.add(trimmed);
			}
			for (const value of secondary) {
				const trimmed = value.trim();
				if (trimmed) merged.add(trimmed);
			}
			return [...merged].slice(0, 12);
		}
		$: facets = collectFacetValues(results);
		$: appChips = mergeFacetSuggestions(facets.apps, appSuggestions, selectedApp);
		$: projectChips = mergeFacetSuggestions(facets.projects, projectSuggestions, selectedProject);
		$: queryPreview = query.trim();
		$: [
			queryPreview,
			selectedApp ?? "",
			selectedProject ?? "",
			selectedWindow
		].join("::");
		$$renderer.push(`<section class="panel svelte-1t1sd7y"${attr("aria-busy", loading)}><header class="panel__header svelte-1t1sd7y"><p class="panel__section svelte-1t1sd7y">Search</p> <h2 class="svelte-1t1sd7y">Memory retrieval deck</h2> <p class="panel__summary svelte-1t1sd7y">Live FTS across extraction summaries, projects, topics, and app context. Ranked hits surface your most relevant captures first.</p></header> <label class="search-input svelte-1t1sd7y" for="search-query"><span class="svelte-1t1sd7y">Search captures</span> <input id="search-query" name="q" type="search"${attr("value", query)} autocomplete="off" placeholder="Find moments, tasks, topics, or people" aria-label="Search captures" class="svelte-1t1sd7y"/></label> <section class="chip-stack svelte-1t1sd7y" aria-label="Search filters"><article class="chip-group svelte-1t1sd7y"><header class="svelte-1t1sd7y"><h3 class="svelte-1t1sd7y">Time range</h3> <p class="svelte-1t1sd7y">${escape_html(timeRangeOptions.find((option) => option.id === selectedWindow)?.description)}</p></header> <div class="chips svelte-1t1sd7y"><!--[-->`);
		const each_array = ensure_array_like(timeRangeOptions);
		for (let $$index = 0, $$length = each_array.length; $$index < $$length; $$index++) {
			let range = each_array[$$index];
			$$renderer.push(`<button type="button"${attr_class("chip svelte-1t1sd7y", void 0, { "chip--active": selectedWindow === range.id })}>${escape_html(range.label)}</button>`);
		}
		$$renderer.push(`<!--]--></div></article> <article class="chip-group svelte-1t1sd7y"><header class="svelte-1t1sd7y"><h3 class="svelte-1t1sd7y">Apps</h3> <p class="svelte-1t1sd7y">Refine by frontmost app name.</p></header> <div class="chips svelte-1t1sd7y"><button type="button"${attr_class("chip svelte-1t1sd7y", void 0, { "chip--active": true })}>All apps</button> `);
		if (appChips.length === 0) {
			$$renderer.push("<!--[0-->");
			$$renderer.push(`<span class="chip-empty svelte-1t1sd7y">No app facets yet</span>`);
		} else {
			$$renderer.push("<!--[-1-->");
			$$renderer.push(`<!--[-->`);
			const each_array_1 = ensure_array_like(appChips);
			for (let $$index_1 = 0, $$length = each_array_1.length; $$index_1 < $$length; $$index_1++) {
				let appName = each_array_1[$$index_1];
				$$renderer.push(`<button type="button"${attr_class("chip svelte-1t1sd7y", void 0, { "chip--active": selectedApp === appName })}>${escape_html(appName)}</button>`);
			}
			$$renderer.push(`<!--]-->`);
		}
		$$renderer.push(`<!--]--></div></article> <article class="chip-group svelte-1t1sd7y"><header class="svelte-1t1sd7y"><h3 class="svelte-1t1sd7y">Projects</h3> <p class="svelte-1t1sd7y">Slice relevance ranking by project context.</p></header> <div class="chips svelte-1t1sd7y"><button type="button"${attr_class("chip svelte-1t1sd7y", void 0, { "chip--active": true })}>All projects</button> `);
		if (projectChips.length === 0) {
			$$renderer.push("<!--[0-->");
			$$renderer.push(`<span class="chip-empty svelte-1t1sd7y">No project facets yet</span>`);
		} else {
			$$renderer.push("<!--[-1-->");
			$$renderer.push(`<!--[-->`);
			const each_array_2 = ensure_array_like(projectChips);
			for (let $$index_2 = 0, $$length = each_array_2.length; $$index_2 < $$length; $$index_2++) {
				let projectName = each_array_2[$$index_2];
				$$renderer.push(`<button type="button"${attr_class("chip svelte-1t1sd7y", void 0, { "chip--active": selectedProject === projectName })}>${escape_html(projectName)}</button>`);
			}
			$$renderer.push(`<!--]-->`);
		}
		$$renderer.push(`<!--]--></div></article></section> `);
		if (queryPreview) {
			$$renderer.push("<!--[0-->");
			$$renderer.push(`<p class="result-summary svelte-1t1sd7y">`);
			$$renderer.push("<!--[-1-->");
			$$renderer.push(`${escape_html(results.length)} ranked result${escape_html(results.length === 1 ? "" : "s")} for “${escape_html(queryPreview)}”.`);
			$$renderer.push(`<!--]--></p>`);
		} else $$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--> `);
		$$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--> `);
		if (!queryPreview) {
			$$renderer.push("<!--[0-->");
			$$renderer.push(`<section class="empty-state svelte-1t1sd7y" aria-label="Search suggestions"><h3 class="svelte-1t1sd7y">Start with a prompt</h3> <p class="svelte-1t1sd7y">Search waits for your input and debounces requests automatically. Try one of these prompts or type your own.</p> <div class="chips svelte-1t1sd7y"><!--[-->`);
			const each_array_3 = ensure_array_like(quickPrompts);
			for (let $$index_3 = 0, $$length = each_array_3.length; $$index_3 < $$length; $$index_3++) {
				let prompt = each_array_3[$$index_3];
				$$renderer.push(`<button type="button" class="chip svelte-1t1sd7y">${escape_html(prompt)}</button>`);
			}
			$$renderer.push(`<!--]--></div></section>`);
		} else {
			$$renderer.push("<!--[-1-->");
			$$renderer.push(`<div class="results-grid svelte-1t1sd7y" aria-live="polite"><!--[-->`);
			const each_array_4 = ensure_array_like(results);
			for (let index = 0, $$length = each_array_4.length; index < $$length; index++) {
				let result = each_array_4[index];
				SearchResultCard($$renderer, {
					result,
					position: index + 1
				});
			}
			$$renderer.push(`<!--]--></div>`);
		}
		$$renderer.push(`<!--]--></section>`);
	});
}
//#endregion
//#region src/routes/search/+page.svelte
function _page($$renderer) {
	head("e12qt1", $$renderer, ($$renderer) => {
		$$renderer.title(($$renderer) => {
			$$renderer.push(`<title>Screencap · Search</title>`);
		});
	});
	SearchWorkspace($$renderer, {});
}
//#endregion
export { _page as default };
