import { Q as escape_html, X as attr, at as fallback, f as ensure_array_like, l as bind_props, p as head, s as attr_style } from "../../../chunks/index-server.js";
//#region src/lib/components/Insights/DailySummaryCard.svelte
function DailySummaryCard($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let strongestProjectMinutes;
		let loading = fallback($$props["loading"], false);
		let summary = fallback($$props["summary"], null);
		let selectedDate = fallback($$props["selectedDate"], "");
		function segmentWidth(minutes) {
			if (!summary || summary.totalMinutes <= 0) return 0;
			return Math.max(6, Math.round(minutes / summary.totalMinutes * 100));
		}
		function projectWidth(minutes) {
			if (strongestProjectMinutes <= 0) return 0;
			return Math.max(8, Math.round(minutes / strongestProjectMinutes * 100));
		}
		$: strongestProjectMinutes = summary?.projects.reduce((max, project) => Math.max(max, project.minutes), 0) ?? 0;
		$$renderer.push(`<article class="card svelte-qg4gwb"${attr("aria-busy", loading)}><header class="svelte-qg4gwb"><p class="eyebrow svelte-qg4gwb">Daily synthesis</p> <h3 class="svelte-qg4gwb">${escape_html(selectedDate)}</h3></header> `);
		if (loading) {
			$$renderer.push("<!--[0-->");
			$$renderer.push(`<p class="state svelte-qg4gwb">Compiling today’s synthesized activity…</p>`);
		} else if (!summary) {
			$$renderer.push("<!--[1-->");
			$$renderer.push(`<p class="state svelte-qg4gwb">No summary exists for this date.</p>`);
		} else {
			$$renderer.push("<!--[-1-->");
			$$renderer.push(`<div class="kpis svelte-qg4gwb"><div class="kpi svelte-qg4gwb"><p class="svelte-qg4gwb">Total active time</p> <strong class="svelte-qg4gwb">${escape_html(summary.totalLabel)}</strong></div> <div class="kpi svelte-qg4gwb"><p class="svelte-qg4gwb">Focus score</p> <strong class="svelte-qg4gwb">${escape_html(summary.focusScoreLabel)}</strong></div></div> <section class="svelte-qg4gwb"><div class="section__heading svelte-qg4gwb"><h4 class="svelte-qg4gwb">Project breakdown</h4></div> `);
			if (summary.projects.length === 0) {
				$$renderer.push("<!--[0-->");
				$$renderer.push(`<p class="state svelte-qg4gwb">No project allocation available.</p>`);
			} else {
				$$renderer.push("<!--[-1-->");
				$$renderer.push(`<ul class="projects svelte-qg4gwb"><!--[-->`);
				const each_array = ensure_array_like(summary.projects);
				for (let $$index = 0, $$length = each_array.length; $$index < $$length; $$index++) {
					let project = each_array[$$index];
					$$renderer.push(`<li class="svelte-qg4gwb"><div class="projects__label svelte-qg4gwb"><span>${escape_html(project.name)}</span> <strong class="svelte-qg4gwb">${escape_html(project.durationLabel)}</strong></div> <div class="projects__track svelte-qg4gwb" role="presentation"><div class="projects__bar svelte-qg4gwb"${attr_style(`width: ${projectWidth(project.minutes)}%`)}></div></div></li>`);
				}
				$$renderer.push(`<!--]--></ul>`);
			}
			$$renderer.push(`<!--]--></section> <section class="svelte-qg4gwb"><div class="section__heading svelte-qg4gwb"><h4 class="svelte-qg4gwb">Key moments</h4></div> `);
			if (summary.keyMoments.length === 0) {
				$$renderer.push("<!--[0-->");
				$$renderer.push(`<p class="state svelte-qg4gwb">No key moments captured for this date.</p>`);
			} else {
				$$renderer.push("<!--[-1-->");
				$$renderer.push(`<ul class="moments svelte-qg4gwb"><!--[-->`);
				const each_array_1 = ensure_array_like(summary.keyMoments);
				for (let $$index_1 = 0, $$length = each_array_1.length; $$index_1 < $$length; $$index_1++) {
					let moment = each_array_1[$$index_1];
					$$renderer.push(`<li class="svelte-qg4gwb">${escape_html(moment)}</li>`);
				}
				$$renderer.push(`<!--]--></ul>`);
			}
			$$renderer.push(`<!--]--></section> <section class="svelte-qg4gwb"><div class="section__heading svelte-qg4gwb"><h4 class="svelte-qg4gwb">Focus blocks</h4></div> `);
			if (summary.focusBlocks.length === 0) {
				$$renderer.push("<!--[0-->");
				$$renderer.push(`<p class="state svelte-qg4gwb">No focus blocks generated.</p>`);
			} else {
				$$renderer.push("<!--[-1-->");
				$$renderer.push(`<div class="focus-track svelte-qg4gwb" aria-label="Focus block timeline"><!--[-->`);
				const each_array_2 = ensure_array_like(summary.focusBlocks);
				for (let $$index_2 = 0, $$length = each_array_2.length; $$index_2 < $$length; $$index_2++) {
					let block = each_array_2[$$index_2];
					$$renderer.push(`<div class="focus-segment svelte-qg4gwb"${attr_style(`width:${segmentWidth(block.minutes)}%; --focus-tint:${block.tint};`)}${attr("title", `${block.project} · ${block.label} · ${block.quality}`)}><span class="svelte-qg4gwb">${escape_html(block.project)}</span></div>`);
				}
				$$renderer.push(`<!--]--></div>`);
			}
			$$renderer.push(`<!--]--></section> <section class="svelte-qg4gwb"><div class="section__heading svelte-qg4gwb"><h4 class="svelte-qg4gwb">Open threads</h4></div> `);
			if (summary.openThreads.length === 0) {
				$$renderer.push("<!--[0-->");
				$$renderer.push(`<p class="state svelte-qg4gwb">No open threads tracked.</p>`);
			} else {
				$$renderer.push("<!--[-1-->");
				$$renderer.push(`<ul class="threads svelte-qg4gwb"><!--[-->`);
				const each_array_3 = ensure_array_like(summary.openThreads);
				for (let $$index_3 = 0, $$length = each_array_3.length; $$index_3 < $$length; $$index_3++) {
					let thread = each_array_3[$$index_3];
					$$renderer.push(`<li class="svelte-qg4gwb">${escape_html(thread)}</li>`);
				}
				$$renderer.push(`<!--]--></ul>`);
			}
			$$renderer.push(`<!--]--></section> `);
			if (summary.narrative) {
				$$renderer.push("<!--[0-->");
				$$renderer.push(`<p class="narrative svelte-qg4gwb">${escape_html(summary.narrative)}</p>`);
			} else $$renderer.push("<!--[-1-->");
			$$renderer.push(`<!--]-->`);
		}
		$$renderer.push(`<!--]--></article>`);
		bind_props($$props, {
			loading,
			summary,
			selectedDate
		});
	});
}
//#endregion
//#region src/lib/components/Insights/RollingContextCard.svelte
function RollingContextCard($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let loading = fallback($$props["loading"], false);
		let context = fallback($$props["context"], null);
		$$renderer.push(`<article class="card svelte-1tzlzis"${attr("aria-busy", loading)}><header class="svelte-1tzlzis"><p class="eyebrow svelte-1tzlzis">Rolling context</p> <h3 class="svelte-1tzlzis">What am I doing right now?</h3></header> `);
		if (loading) {
			$$renderer.push("<!--[0-->");
			$$renderer.push(`<p class="state svelte-1tzlzis">Reading the latest 30-minute synthesis…</p>`);
		} else if (!context) {
			$$renderer.push("<!--[1-->");
			$$renderer.push(`<p class="state svelte-1tzlzis">No rolling context available yet.</p>`);
		} else {
			$$renderer.push("<!--[-1-->");
			$$renderer.push(`<div class="focus svelte-1tzlzis"><p class="focus__label svelte-1tzlzis">Current focus</p> <p class="focus__value svelte-1tzlzis">${escape_html(context.currentFocus)}</p> `);
			if (context.activeProject) {
				$$renderer.push("<!--[0-->");
				$$renderer.push(`<p class="focus__project svelte-1tzlzis">Project · ${escape_html(context.activeProject)}</p>`);
			} else $$renderer.push("<!--[-1-->");
			$$renderer.push(`<!--]--></div> <div class="meta-grid svelte-1tzlzis"><div><p class="meta-label svelte-1tzlzis">Mood</p> <p class="meta-value svelte-1tzlzis">${escape_html(context.mood ?? "Unspecified")}</p></div> <div><p class="meta-label svelte-1tzlzis">Apps in rotation</p> `);
			if (context.appsUsed.length === 0) {
				$$renderer.push("<!--[0-->");
				$$renderer.push(`<p class="meta-value svelte-1tzlzis">No app context in this window.</p>`);
			} else {
				$$renderer.push("<!--[-1-->");
				$$renderer.push(`<ul class="svelte-1tzlzis"><!--[-->`);
				const each_array = ensure_array_like(context.appsUsed);
				for (let $$index = 0, $$length = each_array.length; $$index < $$length; $$index++) {
					let app = each_array[$$index];
					$$renderer.push(`<li class="svelte-1tzlzis"><span>${escape_html(app.name)}</span> <strong class="svelte-1tzlzis">${escape_html(app.share)}</strong></li>`);
				}
				$$renderer.push(`<!--]--></ul>`);
			}
			$$renderer.push(`<!--]--></div></div> `);
			if (context.summary) {
				$$renderer.push("<!--[0-->");
				$$renderer.push(`<p class="summary svelte-1tzlzis">${escape_html(context.summary)}</p>`);
			} else $$renderer.push("<!--[-1-->");
			$$renderer.push(`<!--]-->`);
		}
		$$renderer.push(`<!--]--></article>`);
		bind_props($$props, {
			loading,
			context
		});
	});
}
//#endregion
//#region src/lib/components/Insights/InsightsDashboard.svelte
function InsightsDashboard($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let selectedDate = formatDateInput(/* @__PURE__ */ new Date());
		let loading = true;
		let rollingContext = null;
		let dailySummary = null;
		function formatDateInput(date) {
			return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
		}
		$$renderer.push(`<section class="panel svelte-xz2sxb"${attr("aria-busy", loading)}><header class="panel__header svelte-xz2sxb"><p class="panel__section svelte-xz2sxb">Insights</p> <h2 class="svelte-xz2sxb">Daily cognition board</h2> <p class="panel__summary svelte-xz2sxb">Synthesized activity for any day plus rolling context from the latest capture window.</p></header> <form class="date-picker svelte-xz2sxb"><label for="insights-date" class="svelte-xz2sxb">Day</label> <input id="insights-date" name="insights-date" type="date"${attr("max", formatDateInput(/* @__PURE__ */ new Date()))}${attr("value", selectedDate)} class="svelte-xz2sxb"/></form> `);
		$$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--> <div class="insights-grid svelte-xz2sxb">`);
		RollingContextCard($$renderer, {
			context: rollingContext,
			loading
		});
		$$renderer.push(`<!----> `);
		DailySummaryCard($$renderer, {
			summary: dailySummary,
			selectedDate,
			loading
		});
		$$renderer.push(`<!----></div></section>`);
	});
}
//#endregion
//#region src/routes/insights/+page.svelte
function _page($$renderer) {
	head("u6zn5i", $$renderer, ($$renderer) => {
		$$renderer.title(($$renderer) => {
			$$renderer.push(`<title>Screencap · Insights</title>`);
		});
	});
	InsightsDashboard($$renderer, {});
}
//#endregion
export { _page as default };
