import { Q as escape_html, X as attr, at as fallback, f as ensure_array_like, l as bind_props, o as attr_class, p as head, r as onDestroy, t as createEventDispatcher } from "../../../chunks/index-server.js";
import "../../../chunks/api.js";
import { t as CaptureDetailsModal } from "../../../chunks/CaptureDetailsModal.js";
//#region src/lib/components/Search/SearchResultCard.svelte
function SearchResultCard($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let capturedAt, hasTimestamp, timestampLabel, screenshotSrc, appLabel, projectLabel, activityLabel, descriptionLabel, topics;
		let result = $$props["result"];
		let position = fallback($$props["position"], 1);
		createEventDispatcher();
		const timestampFormatter = new Intl.DateTimeFormat(void 0, {
			dateStyle: "medium",
			timeStyle: "short"
		});
		let imageFailed = false;
		let previousCaptureId = null;
		function buildScreenshotSrc() {
			if (result.capture.screenshot_url?.trim()) return result.capture.screenshot_url;
			if (!result.capture.screenshot_path?.trim()) return null;
			const normalizedPath = result.capture.screenshot_path.replace(/^\/+/, "");
			return `/api/screenshots/${encodeURIComponent(normalizedPath)}`;
		}
		function humanizeActivity(activityType) {
			if (!activityType?.trim()) return "Unclassified";
			return activityType.split("_").filter(Boolean).map((word) => word.charAt(0).toUpperCase() + word.slice(1)).join(" ");
		}
		$: if (result.capture.id !== previousCaptureId) {
			imageFailed = false;
			previousCaptureId = result.capture.id;
		}
		$: capturedAt = new Date(result.capture.timestamp);
		$: hasTimestamp = Number.isFinite(capturedAt.getTime());
		$: timestampLabel = hasTimestamp ? timestampFormatter.format(capturedAt) : "Timestamp unavailable";
		$: screenshotSrc = imageFailed ? null : buildScreenshotSrc();
		$: appLabel = result.capture.app_name?.trim() || "Unknown app";
		$: projectLabel = result.extraction.project?.trim() || "Unassigned project";
		$: activityLabel = humanizeActivity(result.extraction.activity_type);
		$: descriptionLabel = result.extraction.description?.trim() || result.extraction.key_content?.trim() || result.batchNarrative?.trim() || "No extraction description available.";
		$: topics = result.extraction.topics;
		$$renderer.push(`<button class="search-result svelte-116ub32" type="button"${attr("aria-label", `Open search result ${position}`)}><div class="search-result__thumb-wrap svelte-116ub32">`);
		if (screenshotSrc) {
			$$renderer.push("<!--[0-->");
			$$renderer.push(`<img class="search-result__thumb svelte-116ub32"${attr("src", screenshotSrc)}${attr("alt", `Screenshot from ${appLabel} at ${timestampLabel}`)} loading="lazy"/>`);
		} else {
			$$renderer.push("<!--[-1-->");
			$$renderer.push(`<div class="search-result__thumb search-result__thumb--fallback svelte-116ub32" role="img" aria-label="Screenshot unavailable">Screenshot unavailable</div>`);
		}
		$$renderer.push(`<!--]--></div> <div class="search-result__meta svelte-116ub32"><p class="search-result__rank svelte-116ub32">#${escape_html(position)}</p> <p class="search-result__time svelte-116ub32">${escape_html(timestampLabel)}</p></div> <h3 class="search-result__summary svelte-116ub32"${attr("title", descriptionLabel)}>${escape_html(descriptionLabel)}</h3> <dl class="search-result__facts svelte-116ub32"><div class="svelte-116ub32"><dt class="svelte-116ub32">App</dt> <dd class="svelte-116ub32">${escape_html(appLabel)}</dd></div> <div class="svelte-116ub32"><dt class="svelte-116ub32">Project</dt> <dd class="svelte-116ub32">${escape_html(projectLabel)}</dd></div> <div class="svelte-116ub32"><dt class="svelte-116ub32">Activity</dt> <dd class="svelte-116ub32">${escape_html(activityLabel)}</dd></div></dl> `);
		if (topics.length > 0) {
			$$renderer.push("<!--[0-->");
			$$renderer.push(`<div class="search-result__topics svelte-116ub32" aria-label="Topics"><!--[-->`);
			const each_array = ensure_array_like(topics.slice(0, 6));
			for (let $$index = 0, $$length = each_array.length; $$index < $$length; $$index++) {
				let topic = each_array[$$index];
				$$renderer.push(`<span class="svelte-116ub32">${escape_html(topic)}</span>`);
			}
			$$renderer.push(`<!--]--></div>`);
		} else $$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--></button>`);
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
		let facets, activityChips, appChips, projectChips, queryPreview, normalizedSelectedActivity, filteredResults, visibleResults, hasMoreResults;
		const datePresetOptions = [
			{
				id: "all",
				label: "All time",
				hours: null
			},
			{
				id: "24h",
				label: "24h",
				hours: 24
			},
			{
				id: "7d",
				label: "7d",
				hours: 168
			},
			{
				id: "30d",
				label: "30d",
				hours: 720
			}
		];
		const quickPrompts = [
			"sprint planning",
			"incident follow-up",
			"debugging",
			"PR review"
		];
		const RESULTS_STEP = 18;
		let query = "";
		let loading = false;
		let selectedApp = null;
		let selectedProject = null;
		let selectedActivity = null;
		let selectedPreset = "7d";
		let fromDate = "";
		let toDate = "";
		let results = [];
		let appSuggestions = [];
		let projectSuggestions = [];
		let visibleLimit = RESULTS_STEP;
		let paginationFingerprint = "";
		let selectedCapture = null;
		let selectedExtraction = null;
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
			return [...merged].slice(0, 14);
		}
		function collectActivityTypes(entries) {
			const values = /* @__PURE__ */ new Set();
			for (const entry of entries) {
				const activity = entry.extraction.activity_type?.trim();
				if (activity) values.add(activity);
			}
			return [...values].sort((left, right) => left.localeCompare(right));
		}
		$: facets = collectFacetValues(results);
		$: activityChips = collectActivityTypes(results);
		$: appChips = mergeFacetSuggestions(facets.apps, appSuggestions, selectedApp);
		$: projectChips = mergeFacetSuggestions(facets.projects, projectSuggestions, selectedProject);
		$: queryPreview = query.trim();
		$: normalizedSelectedActivity = selectedActivity?.trim().toLowerCase() ?? "";
		$: filteredResults = results.filter((result) => {
			if (!normalizedSelectedActivity) return true;
			return (result.extraction.activity_type?.trim().toLowerCase() ?? "") === normalizedSelectedActivity;
		});
		$: {
			const nextPaginationFingerprint = [
				queryPreview,
				selectedApp ?? "",
				selectedProject ?? "",
				selectedActivity ?? "",
				fromDate,
				toDate,
				String(results.length)
			].join("::");
			if (nextPaginationFingerprint !== paginationFingerprint) {
				paginationFingerprint = nextPaginationFingerprint;
				visibleLimit = RESULTS_STEP;
			}
		}
		$: visibleResults = filteredResults.slice(0, visibleLimit);
		$: hasMoreResults = visibleResults.length < filteredResults.length;
		$: [
			queryPreview,
			selectedApp ?? "",
			selectedProject ?? "",
			fromDate,
			toDate
		].join("::");
		$$renderer.push(`<section class="panel svelte-1t1sd7y"${attr("aria-busy", loading)}><header class="panel__header svelte-1t1sd7y"><p class="panel__section svelte-1t1sd7y">Search</p> <h2 class="svelte-1t1sd7y">Memory retrieval deck</h2> <p class="panel__summary svelte-1t1sd7y">Full-text query across indexed extraction descriptions, projects, and topics. Refine by app, project, activity type,
      and date range.</p></header> <label class="search-input svelte-1t1sd7y" for="search-query"><span class="svelte-1t1sd7y">Search captures</span> <input id="search-query" name="q" type="search"${attr("value", query)} autocomplete="off" placeholder="Find moments, tasks, topics, or people" aria-label="Search captures" class="svelte-1t1sd7y"/></label> <section class="chip-stack svelte-1t1sd7y" aria-label="Search filters"><article class="chip-group svelte-1t1sd7y"><header class="svelte-1t1sd7y"><h3 class="svelte-1t1sd7y">Date range</h3> <p class="svelte-1t1sd7y">Preset windows or custom from/to boundaries.</p></header> <div class="chips svelte-1t1sd7y"><!--[-->`);
		const each_array = ensure_array_like(datePresetOptions);
		for (let $$index = 0, $$length = each_array.length; $$index < $$length; $$index++) {
			let option = each_array[$$index];
			$$renderer.push(`<button type="button"${attr_class("chip svelte-1t1sd7y", void 0, { "chip--active": selectedPreset === option.id })}>${escape_html(option.label)}</button>`);
		}
		$$renderer.push(`<!--]--></div> <div class="date-controls svelte-1t1sd7y"><label class="svelte-1t1sd7y"><span class="svelte-1t1sd7y">From</span> <input type="date"${attr("value", fromDate)} class="svelte-1t1sd7y"/></label> <label class="svelte-1t1sd7y"><span class="svelte-1t1sd7y">To</span> <input type="date"${attr("value", toDate)} class="svelte-1t1sd7y"/></label></div></article> <article class="chip-group svelte-1t1sd7y"><header class="svelte-1t1sd7y"><h3 class="svelte-1t1sd7y">Apps</h3> <p class="svelte-1t1sd7y">Refine by frontmost application.</p></header> <div class="chips svelte-1t1sd7y"><button type="button"${attr_class("chip svelte-1t1sd7y", void 0, { "chip--active": true })}>All apps</button> `);
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
		$$renderer.push(`<!--]--></div></article> <article class="chip-group svelte-1t1sd7y"><header class="svelte-1t1sd7y"><h3 class="svelte-1t1sd7y">Projects</h3> <p class="svelte-1t1sd7y">Limit retrieval to project context.</p></header> <div class="chips svelte-1t1sd7y"><button type="button"${attr_class("chip svelte-1t1sd7y", void 0, { "chip--active": true })}>All projects</button> `);
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
		$$renderer.push(`<!--]--></div></article> <article class="chip-group svelte-1t1sd7y"><header class="svelte-1t1sd7y"><h3 class="svelte-1t1sd7y">Activity</h3> <p class="svelte-1t1sd7y">Client-side filter when API activity filtering is unavailable.</p></header> <div class="chips svelte-1t1sd7y"><button type="button"${attr_class("chip svelte-1t1sd7y", void 0, { "chip--active": true })}>All activities</button> `);
		if (activityChips.length === 0) {
			$$renderer.push("<!--[0-->");
			$$renderer.push(`<span class="chip-empty svelte-1t1sd7y">No activity types in results</span>`);
		} else {
			$$renderer.push("<!--[-1-->");
			$$renderer.push(`<!--[-->`);
			const each_array_3 = ensure_array_like(activityChips);
			for (let $$index_3 = 0, $$length = each_array_3.length; $$index_3 < $$length; $$index_3++) {
				let activity = each_array_3[$$index_3];
				$$renderer.push(`<button type="button"${attr_class("chip svelte-1t1sd7y", void 0, { "chip--active": selectedActivity === activity })}>${escape_html(activity.replaceAll("_", " "))}</button>`);
			}
			$$renderer.push(`<!--]-->`);
		}
		$$renderer.push(`<!--]--></div></article></section> `);
		if (queryPreview) {
			$$renderer.push("<!--[0-->");
			$$renderer.push(`<p class="result-summary svelte-1t1sd7y">`);
			$$renderer.push("<!--[-1-->");
			$$renderer.push(`Showing ${escape_html(visibleResults.length)} of ${escape_html(filteredResults.length)} result${escape_html(filteredResults.length === 1 ? "" : "s")} for
        “${escape_html(queryPreview)}”.`);
			$$renderer.push(`<!--]--></p>`);
		} else $$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--> `);
		$$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--> `);
		if (!queryPreview) {
			$$renderer.push("<!--[0-->");
			$$renderer.push(`<section class="empty-state svelte-1t1sd7y" aria-label="Search suggestions"><h3 class="svelte-1t1sd7y">Start with a prompt</h3> <p class="svelte-1t1sd7y">Search debounces requests automatically. Try one of these prompts or type your own.</p> <div class="chips svelte-1t1sd7y"><!--[-->`);
			const each_array_4 = ensure_array_like(quickPrompts);
			for (let $$index_4 = 0, $$length = each_array_4.length; $$index_4 < $$length; $$index_4++) {
				let prompt = each_array_4[$$index_4];
				$$renderer.push(`<button type="button" class="chip svelte-1t1sd7y">${escape_html(prompt)}</button>`);
			}
			$$renderer.push(`<!--]--></div></section>`);
		} else {
			$$renderer.push("<!--[-1-->");
			$$renderer.push(`<div class="results-grid svelte-1t1sd7y" aria-live="polite"><!--[-->`);
			const each_array_5 = ensure_array_like(visibleResults);
			for (let index = 0, $$length = each_array_5.length; index < $$length; index++) {
				let result = each_array_5[index];
				SearchResultCard($$renderer, {
					result,
					position: index + 1
				});
			}
			$$renderer.push(`<!--]--></div> `);
			if (hasMoreResults) {
				$$renderer.push("<!--[0-->");
				$$renderer.push(`<div class="load-more-wrap svelte-1t1sd7y"><button class="load-more svelte-1t1sd7y" type="button">Load more</button></div>`);
			} else $$renderer.push("<!--[-1-->");
			$$renderer.push(`<!--]-->`);
		}
		$$renderer.push(`<!--]--> `);
		$$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--> `);
		CaptureDetailsModal($$renderer, {
			open: selectedCapture !== null,
			capture: selectedCapture,
			extraction: selectedExtraction
		});
		$$renderer.push(`<!----></section>`);
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
