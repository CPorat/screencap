import { Y as attr, c as bind_props, d as ensure_array_like, f as head, it as fallback } from "../../chunks/index-server.js";
import "../../chunks/api.js";
//#endregion
//#region src/lib/components/Timeline/TimelineSkeleton.svelte
function TimelineSkeleton($$renderer, $$props) {
	let count, placeholders;
	let compact = fallback($$props["compact"], false);
	$: count = compact ? 2 : 6;
	$: placeholders = Array.from({ length: count }, (_, index) => index);
	$$renderer.push(`<div class="skeleton svelte-y87wi4"${attr("data-compact", compact)} aria-hidden="true"><!--[-->`);
	const each_array = ensure_array_like(placeholders);
	for (let $$index = 0, $$length = each_array.length; $$index < $$length; $$index++) {
		each_array[$$index];
		$$renderer.push(`<article class="skeleton__card svelte-y87wi4"><div class="skeleton__meta svelte-y87wi4"></div> <div class="skeleton__line skeleton__line--short svelte-y87wi4"></div> <div class="skeleton__line svelte-y87wi4"></div> <div class="skeleton__line svelte-y87wi4"></div> <div class="skeleton__image svelte-y87wi4"></div> <div class="skeleton__line skeleton__line--button svelte-y87wi4"></div></article>`);
	}
	$$renderer.push(`<!--]--></div>`);
	bind_props($$props, { compact });
}
//#endregion
//#region src/lib/components/Timeline/TimelineFeed.svelte
function TimelineFeed($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		const hourHeadingFormatter = new Intl.DateTimeFormat(void 0, {
			weekday: "short",
			month: "short",
			day: "numeric",
			hour: "numeric"
		});
		const shortDateFormatter = new Intl.DateTimeFormat(void 0, {
			month: "short",
			day: "numeric"
		});
		const hourClockFormatter = new Intl.DateTimeFormat(void 0, {
			hour: "numeric",
			minute: "2-digit"
		});
		let timeline = [];
		let loadingInitial = true;
		function isSameLocalDay(a, b) {
			return a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();
		}
		function hourStart(date) {
			const normalized = new Date(date);
			normalized.setMinutes(0, 0, 0);
			return normalized;
		}
		function hourRangeLabel(start) {
			const end = new Date(start);
			end.setMinutes(59, 59, 999);
			return `${hourClockFormatter.format(start)} — ${hourClockFormatter.format(end)}`;
		}
		function hourHeading(start) {
			const now = /* @__PURE__ */ new Date();
			if (isSameLocalDay(start, now)) return `Today · ${hourClockFormatter.format(start)}`;
			const yesterday = new Date(now);
			yesterday.setDate(yesterday.getDate() - 1);
			if (isSameLocalDay(start, yesterday)) return `Yesterday · ${hourClockFormatter.format(start)}`;
			return `${hourHeadingFormatter.format(start)} · ${shortDateFormatter.format(start)}`;
		}
		function groupByHour(items) {
			const grouped = /* @__PURE__ */ new Map();
			const sorted = [...items].sort((left, right) => {
				const leftTime = new Date(left.capture.timestamp).getTime();
				return new Date(right.capture.timestamp).getTime() - leftTime;
			});
			for (const item of sorted) {
				const capturedAt = new Date(item.capture.timestamp);
				if (!Number.isFinite(capturedAt.getTime())) continue;
				const start = hourStart(capturedAt);
				const key = start.toISOString();
				const bucket = grouped.get(key);
				if (bucket) {
					bucket.captures.push(item);
					continue;
				}
				grouped.set(key, {
					key,
					heading: hourHeading(start),
					rangeLabel: hourRangeLabel(start),
					captures: [item]
				});
			}
			return Array.from(grouped.values());
		}
		$: groupByHour(timeline);
		$$renderer.push(`<section class="panel svelte-1lp8sdf"${attr("aria-busy", loadingInitial)}><header class="panel__header svelte-1lp8sdf"><p class="panel__section svelte-1lp8sdf">Timeline</p> <h2 class="svelte-1lp8sdf">Chronology stream</h2> <p class="panel__summary svelte-1lp8sdf">Captures are grouped by hour with extraction overlays for activity, projects, and summaries.</p></header> `);
		$$renderer.push("<!--[0-->");
		TimelineSkeleton($$renderer, {});
		$$renderer.push(`<!--]--> `);
		$$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]--> <div class="panel__sentinel svelte-1lp8sdf" aria-hidden="true"></div></section>`);
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
	TimelineFeed($$renderer, {});
}
//#endregion
export { _page as default };
