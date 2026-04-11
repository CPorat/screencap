import { X as attr, f as ensure_array_like, p as head } from "../../chunks/index-server.js";
import "../../chunks/api.js";
import { t as CaptureDetailsModal } from "../../chunks/CaptureDetailsModal.js";
//#endregion
//#region src/routes/Timeline.svelte
function Timeline($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let normalizedAppFilter, normalizedActivityFilter, filteredItems;
		const HOUR_HEADING_FORMATTER = new Intl.DateTimeFormat(void 0, {
			hour: "numeric",
			minute: "2-digit"
		});
		let selectedDate = formatLocalDate(/* @__PURE__ */ new Date());
		let appFilter = "";
		let activityFilter = "";
		let loading = true;
		let timelineItems = [];
		let selectedItem = null;
		function formatLocalDate(value) {
			return `${value.getFullYear()}-${String(value.getMonth() + 1).padStart(2, "0")}-${String(value.getDate()).padStart(2, "0")}`;
		}
		function buildHourBuckets(items) {
			const grouped = /* @__PURE__ */ new Map();
			for (const item of items) {
				const capturedAt = new Date(item.capture.timestamp);
				if (!Number.isFinite(capturedAt.getTime())) continue;
				const hourStart = new Date(capturedAt);
				hourStart.setMinutes(0, 0, 0);
				const key = hourStart.toISOString();
				const existing = grouped.get(key);
				if (existing) {
					existing.captures.push(item);
					continue;
				}
				grouped.set(key, {
					key,
					heading: HOUR_HEADING_FORMATTER.format(hourStart),
					captures: [item]
				});
			}
			return Array.from(grouped.values()).sort((left, right) => {
				const leftTime = new Date(left.key).getTime();
				return new Date(right.key).getTime() - leftTime;
			});
		}
		function normalizeFilterValue(value) {
			return value.trim().toLowerCase();
		}
		function activityText(item) {
			return [
				item.extraction?.activity_type,
				item.extraction?.description,
				item.capture.primary_activity,
				item.capture.narrative,
				item.capture.batch_narrative
			].filter((value) => typeof value === "string" && value.trim().length > 0).join(" ").toLowerCase();
		}
		$: normalizedAppFilter = normalizeFilterValue(appFilter);
		$: normalizedActivityFilter = normalizeFilterValue(activityFilter);
		$: filteredItems = timelineItems.filter((item) => {
			const appName = item.capture.app_name?.toLowerCase() ?? "";
			const appMatches = !normalizedAppFilter || appName.includes(normalizedAppFilter);
			const activityMatches = !normalizedActivityFilter || activityText(item).includes(normalizedActivityFilter);
			return appMatches && activityMatches;
		});
		$: buildHourBuckets(filteredItems);
		$$renderer.push(`<section class="timeline svelte-l9yiin"${attr("aria-busy", loading)}><header class="timeline__header svelte-l9yiin"><p class="timeline__eyebrow svelte-l9yiin">Timeline</p> <h2 class="svelte-l9yiin">Capture Chronicle</h2> <p class="timeline__summary svelte-l9yiin">Select a day, filter captures, and inspect each frame's extraction payload.</p></header> <div class="timeline__controls svelte-l9yiin" role="group" aria-label="Timeline filters"><label class="svelte-l9yiin"><span class="svelte-l9yiin">Date</span> <input type="date"${attr("value", selectedDate)} class="svelte-l9yiin"/></label> <label class="svelte-l9yiin"><span class="svelte-l9yiin">App name</span> <input type="text"${attr("value", appFilter)} placeholder="Filter by app" autocomplete="off" class="svelte-l9yiin"/></label> <label class="svelte-l9yiin"><span class="svelte-l9yiin">Activity type</span> <input type="text"${attr("value", activityFilter)} placeholder="coding, reading, meeting..." autocomplete="off" class="svelte-l9yiin"/></label></div> `);
		{
			$$renderer.push("<!--[0-->");
			$$renderer.push(`<div class="timeline__grid svelte-l9yiin" aria-hidden="true"><!--[-->`);
			const each_array = ensure_array_like(Array.from({ length: 6 }, (_, index) => index));
			for (let $$index = 0, $$length = each_array.length; $$index < $$length; $$index++) {
				each_array[$$index];
				$$renderer.push(`<article class="timeline__skeleton svelte-l9yiin"><div class="timeline__skeleton-image svelte-l9yiin"></div> <div class="timeline__skeleton-line timeline__skeleton-line--short svelte-l9yiin"></div> <div class="timeline__skeleton-line svelte-l9yiin"></div></article>`);
			}
			$$renderer.push(`<!--]--></div>`);
		}
		$$renderer.push(`<!--]--> `);
		CaptureDetailsModal($$renderer, {
			open: selectedItem !== null,
			capture: selectedItem?.capture ?? null,
			extraction: selectedItem?.extraction ?? null
		});
		$$renderer.push(`<!----></section>`);
	});
}
//#endregion
//#region src/routes/+page.svelte
function _page($$renderer) {
	head("1uha8ag", $$renderer, ($$renderer) => {
		$$renderer.title(($$renderer) => {
			$$renderer.push(`<title>Screencap · Timeline</title>`);
		});
	});
	Timeline($$renderer, {});
}
//#endregion
export { _page as default };
