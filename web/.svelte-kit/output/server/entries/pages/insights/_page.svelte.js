import { Q as escape_html, X as attr, at as fallback, f as ensure_array_like, l as bind_props, p as head, s as attr_style } from "../../../chunks/index-server.js";
import "../../../chunks/api.js";
//#region src/lib/components/DailySummarySection.svelte
function DailySummarySection($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let maxProjectMinutes, focusBlockTotal;
		let loading = fallback($$props["loading"], false);
		let summary = fallback($$props["summary"], null);
		let selectedDate = fallback($$props["selectedDate"], "");
		function projectWidth(totalMinutes) {
			if (maxProjectMinutes <= 0) return 0;
			return Math.max(10, Math.round(totalMinutes / maxProjectMinutes * 100));
		}
		function focusWidth(durationMinutes) {
			if (focusBlockTotal <= 0) return 0;
			return Math.max(7, Math.round(durationMinutes / focusBlockTotal * 100));
		}
		$: maxProjectMinutes = summary?.projectBreakdown.reduce((max, project) => Math.max(max, project.totalMinutes), 0) ?? 0;
		$: focusBlockTotal = summary?.focusBlocks.reduce((total, block) => total + Math.max(block.durationMinutes, 0), 0) ?? 0;
		$$renderer.push(`<section class="card svelte-z3cj86"${attr("aria-busy", loading)}><header class="card__header svelte-z3cj86"><p class="card__eyebrow svelte-z3cj86">Daily summary</p> <h3 class="svelte-z3cj86">${escape_html(summary?.date ?? selectedDate)}</h3></header> `);
		if (loading) {
			$$renderer.push("<!--[0-->");
			$$renderer.push(`<p class="card__state svelte-z3cj86">Compiling daily summary…</p>`);
		} else if (!summary) {
			$$renderer.push("<!--[1-->");
			$$renderer.push(`<p class="card__state svelte-z3cj86">No summary exists for this day.</p>`);
		} else {
			$$renderer.push("<!--[-1-->");
			$$renderer.push(`<div class="topline svelte-z3cj86"><p class="svelte-z3cj86">Active time: <strong class="svelte-z3cj86">${escape_html(summary.totalActiveHours === null ? "—" : `${summary.totalActiveHours.toFixed(1)}h`)}</strong></p> <p class="svelte-z3cj86">Focus blocks: <strong class="svelte-z3cj86">${escape_html(summary.focusBlocks.length)}</strong></p></div> <section class="svelte-z3cj86"><p class="card__eyebrow svelte-z3cj86">Project breakdown</p> `);
			if (summary.projectBreakdown.length === 0) {
				$$renderer.push("<!--[0-->");
				$$renderer.push(`<p class="card__state svelte-z3cj86">No project breakdown generated.</p>`);
			} else {
				$$renderer.push("<!--[-1-->");
				$$renderer.push(`<ul class="projects svelte-z3cj86"><!--[-->`);
				const each_array = ensure_array_like(summary.projectBreakdown);
				for (let $$index = 0, $$length = each_array.length; $$index < $$length; $$index++) {
					let project = each_array[$$index];
					$$renderer.push(`<li class="svelte-z3cj86"><div class="projects__top svelte-z3cj86"><strong>${escape_html(project.name)}</strong> <span>${escape_html(project.totalMinutes)}m</span></div> <div class="projects__track svelte-z3cj86" role="presentation"><div class="projects__fill svelte-z3cj86"${attr_style(`width:${projectWidth(project.totalMinutes)}%`)}></div></div></li>`);
				}
				$$renderer.push(`<!--]--></ul>`);
			}
			$$renderer.push(`<!--]--></section> <section class="svelte-z3cj86"><p class="card__eyebrow svelte-z3cj86">Time allocation</p> `);
			if (summary.timeAllocation.length === 0) {
				$$renderer.push("<!--[0-->");
				$$renderer.push(`<p class="card__state svelte-z3cj86">No time allocation available.</p>`);
			} else {
				$$renderer.push("<!--[-1-->");
				$$renderer.push(`<ul class="allocation svelte-z3cj86"><!--[-->`);
				const each_array_1 = ensure_array_like(summary.timeAllocation);
				for (let $$index_1 = 0, $$length = each_array_1.length; $$index_1 < $$length; $$index_1++) {
					let allocation = each_array_1[$$index_1];
					$$renderer.push(`<li class="svelte-z3cj86"><span>${escape_html(allocation.label)}</span> <strong class="svelte-z3cj86">${escape_html(allocation.value)}</strong></li>`);
				}
				$$renderer.push(`<!--]--></ul>`);
			}
			$$renderer.push(`<!--]--></section> <section class="svelte-z3cj86"><p class="card__eyebrow svelte-z3cj86">Focus blocks</p> `);
			if (summary.focusBlocks.length === 0) {
				$$renderer.push("<!--[0-->");
				$$renderer.push(`<p class="card__state svelte-z3cj86">No deep-focus periods detected.</p>`);
			} else {
				$$renderer.push("<!--[-1-->");
				$$renderer.push(`<div class="focus-track svelte-z3cj86" aria-label="Focus blocks timeline"><!--[-->`);
				const each_array_2 = ensure_array_like(summary.focusBlocks);
				for (let $$index_2 = 0, $$length = each_array_2.length; $$index_2 < $$length; $$index_2++) {
					let block = each_array_2[$$index_2];
					$$renderer.push(`<div class="focus-track__segment svelte-z3cj86"${attr_style(`--segment-tint:${block.tint};width:${focusWidth(block.durationMinutes)}%`)}${attr("title", `${block.project} · ${block.quality} · ${block.start}–${block.end}`)}><span>${escape_html(block.project)}</span></div>`);
				}
				$$renderer.push(`<!--]--></div>`);
			}
			$$renderer.push(`<!--]--></section> <section class="svelte-z3cj86"><p class="card__eyebrow svelte-z3cj86">Open threads</p> `);
			if (summary.openThreads.length === 0) {
				$$renderer.push("<!--[0-->");
				$$renderer.push(`<p class="card__state svelte-z3cj86">No open threads captured.</p>`);
			} else {
				$$renderer.push("<!--[-1-->");
				$$renderer.push(`<ul class="threads svelte-z3cj86"><!--[-->`);
				const each_array_3 = ensure_array_like(summary.openThreads);
				for (let $$index_3 = 0, $$length = each_array_3.length; $$index_3 < $$length; $$index_3++) {
					let thread = each_array_3[$$index_3];
					$$renderer.push(`<li class="svelte-z3cj86">${escape_html(thread)}</li>`);
				}
				$$renderer.push(`<!--]--></ul>`);
			}
			$$renderer.push(`<!--]--></section> `);
			if (summary.narrative) {
				$$renderer.push("<!--[0-->");
				$$renderer.push(`<p class="narrative svelte-z3cj86">${escape_html(summary.narrative)}</p>`);
			} else $$renderer.push("<!--[-1-->");
			$$renderer.push(`<!--]-->`);
		}
		$$renderer.push(`<!--]--></section>`);
		bind_props($$props, {
			loading,
			summary,
			selectedDate
		});
	});
}
//#endregion
//#region src/lib/components/RollingContextCard.svelte
function RollingContextCard($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let loading = fallback($$props["loading"], false);
		let context = fallback($$props["context"], null);
		$$renderer.push(`<article class="card svelte-osti7m"${attr("aria-busy", loading)}><header class="card__header svelte-osti7m"><p class="card__eyebrow svelte-osti7m">Rolling context</p> <h3 class="svelte-osti7m">Current trajectory</h3></header> `);
		if (loading) {
			$$renderer.push("<!--[0-->");
			$$renderer.push(`<p class="card__state svelte-osti7m">Loading current focus…</p>`);
		} else if (!context) {
			$$renderer.push("<!--[1-->");
			$$renderer.push(`<p class="card__state svelte-osti7m">No rolling context available right now.</p>`);
		} else {
			$$renderer.push("<!--[-1-->");
			$$renderer.push(`<section class="focus svelte-osti7m"><p class="focus__label svelte-osti7m">Current focus</p> <p class="focus__value svelte-osti7m">${escape_html(context.currentFocus)}</p> <p class="focus__project svelte-osti7m">Active project: ${escape_html(context.activeProject ?? "Unassigned")}</p></section> <section><p class="card__eyebrow svelte-osti7m">Apps used</p> `);
			if (context.appsUsed.length === 0) {
				$$renderer.push("<!--[0-->");
				$$renderer.push(`<p class="card__state svelte-osti7m">No app activity for this window.</p>`);
			} else {
				$$renderer.push("<!--[-1-->");
				$$renderer.push(`<ul class="apps svelte-osti7m"><!--[-->`);
				const each_array = ensure_array_like(context.appsUsed);
				for (let $$index = 0, $$length = each_array.length; $$index < $$length; $$index++) {
					let app = each_array[$$index];
					$$renderer.push(`<li class="svelte-osti7m"><span>${escape_html(app.name)}</span> <strong class="svelte-osti7m">${escape_html(app.share)}</strong></li>`);
				}
				$$renderer.push(`<!--]--></ul>`);
			}
			$$renderer.push(`<!--]--></section>`);
		}
		$$renderer.push(`<!--]--></article>`);
		bind_props($$props, {
			loading,
			context
		});
	});
}
//#endregion
//#region src/routes/Insights.svelte
function Insights($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let hasNoInsights;
		let selectedDate = formatLocalDate(/* @__PURE__ */ new Date());
		let loading = true;
		let rollingContext = null;
		let dailySummary = null;
		function formatLocalDate(value) {
			return `${value.getFullYear()}-${String(value.getMonth() + 1).padStart(2, "0")}-${String(value.getDate()).padStart(2, "0")}`;
		}
		$: hasNoInsights = false;
		head("1vc7f7p", $$renderer, ($$renderer) => {
			$$renderer.title(($$renderer) => {
				$$renderer.push(`<title>Screencap · Insights</title>`);
			});
		});
		$$renderer.push(`<section class="insights svelte-1vc7f7p"${attr("aria-busy", loading)}><header class="insights__header svelte-1vc7f7p"><p class="insights__eyebrow svelte-1vc7f7p">Insights</p> <h2 class="svelte-1vc7f7p">Signal atlas</h2> <p class="insights__summary svelte-1vc7f7p">Rolling context, hour-by-hour synthesis, and the daily picture in one place.</p> <label class="insights__date svelte-1vc7f7p"><span>Date</span> <input type="date"${attr("value", selectedDate)} class="svelte-1vc7f7p"/></label></header> `);
		$$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--> `);
		RollingContextCard($$renderer, {
			loading,
			context: rollingContext
		});
		$$renderer.push(`<!----> <section class="insights__section svelte-1vc7f7p"><div class="insights__section-header svelte-1vc7f7p"><h3 class="svelte-1vc7f7p">Hourly digests</h3> <p class="svelte-1vc7f7p">${escape_html(selectedDate)}</p></div> `);
		{
			$$renderer.push("<!--[0-->");
			$$renderer.push(`<div class="insights__skeleton-grid svelte-1vc7f7p" aria-hidden="true"><!--[-->`);
			const each_array = ensure_array_like(Array.from({ length: 3 }, (_, index) => index));
			for (let $$index = 0, $$length = each_array.length; $$index < $$length; $$index++) {
				each_array[$$index];
				$$renderer.push(`<article class="insights__skeleton svelte-1vc7f7p"></article>`);
			}
			$$renderer.push(`<!--]--></div>`);
		}
		$$renderer.push(`<!--]--></section> `);
		DailySummarySection($$renderer, {
			loading,
			summary: dailySummary,
			selectedDate
		});
		$$renderer.push(`<!----> `);
		if (hasNoInsights) {
			$$renderer.push("<!--[0-->");
			$$renderer.push(`<p class="insights__empty insights__empty--global svelte-1vc7f7p">No insights available for this day.</p>`);
		} else $$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--></section>`);
	});
}
//#endregion
//#region src/routes/insights/+page.svelte
function _page($$renderer) {
	Insights($$renderer, {});
}
//#endregion
export { _page as default };
