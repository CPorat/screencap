import { Q as escape_html, X as attr, at as fallback, f as ensure_array_like, l as bind_props, r as onDestroy, t as createEventDispatcher } from "./index-server.js";
//#region src/lib/components/CaptureDetailsModal.svelte
function CaptureDetailsModal($$renderer, $$props) {
	$$renderer.component(($$renderer) => {
		let screenshotSrc, appLabel, capturedLabel, activityLabel, descriptionLabel, keyContentLabel, topics, extractionJson;
		let open = fallback($$props["open"], false);
		let capture = fallback($$props["capture"], null);
		let extraction = fallback($$props["extraction"], null);
		createEventDispatcher();
		const timestampFormatter = new Intl.DateTimeFormat(void 0, {
			dateStyle: "medium",
			timeStyle: "short"
		});
		let imageFailed = false;
		let previousCaptureId = null;
		function buildScreenshotSrc(target) {
			if (target.screenshot_url?.trim()) return target.screenshot_url;
			if (!target.screenshot_path?.trim()) return null;
			const normalizedPath = target.screenshot_path.replace(/^\/+/, "");
			return `/api/screenshots/${encodeURIComponent(normalizedPath)}`;
		}
		function lockScroll() {}
		function unlockScroll() {}
		onDestroy(() => {
			unlockScroll();
		});
		$: if (capture?.id !== previousCaptureId) {
			imageFailed = false;
			previousCaptureId = capture?.id ?? null;
		}
		$: if (open) lockScroll();
		else unlockScroll();
		$: screenshotSrc = capture && !imageFailed ? buildScreenshotSrc(capture) : null;
		$: appLabel = capture?.app_name?.trim() || "Unknown app";
		$: capturedLabel = capture && Number.isFinite(new Date(capture.timestamp).getTime()) ? timestampFormatter.format(new Date(capture.timestamp)) : "Timestamp unavailable";
		$: activityLabel = extraction?.activity_type?.trim() || "unclassified";
		$: descriptionLabel = extraction?.description?.trim() || "No extraction description available.";
		$: keyContentLabel = extraction?.key_content?.trim() || "No key content extracted for this capture.";
		$: topics = extraction?.topics ?? [];
		$: extractionJson = extraction ? JSON.stringify(extraction, null, 2) : "null";
		if (open && capture) {
			$$renderer.push("<!--[0-->");
			$$renderer.push(`<div class="modal-backdrop svelte-14nrj99" role="presentation"><div class="modal svelte-14nrj99" role="dialog" aria-modal="true" aria-labelledby="capture-modal-title"><header class="modal__header svelte-14nrj99"><div><p class="modal__eyebrow svelte-14nrj99">Capture Details</p> <h2 id="capture-modal-title" class="svelte-14nrj99">${escape_html(appLabel)}</h2> <p class="modal__timestamp svelte-14nrj99">${escape_html(capturedLabel)}</p></div> <button class="modal__close svelte-14nrj99" type="button" aria-label="Close capture details">Close</button></header> `);
			if (screenshotSrc) {
				$$renderer.push("<!--[0-->");
				$$renderer.push(`<img class="modal__screenshot svelte-14nrj99"${attr("src", screenshotSrc)}${attr("alt", `Screenshot for ${appLabel} at ${capturedLabel}`)} loading="lazy"/>`);
			} else {
				$$renderer.push("<!--[-1-->");
				$$renderer.push(`<div class="modal__screenshot modal__screenshot--fallback svelte-14nrj99" role="img" aria-label="Screenshot unavailable">Screenshot unavailable</div>`);
			}
			$$renderer.push(`<!--]--> <div class="modal__content svelte-14nrj99"><section class="modal__section svelte-14nrj99"><h3 class="svelte-14nrj99">Summary</h3> <dl class="modal__facts svelte-14nrj99"><div class="svelte-14nrj99"><dt class="svelte-14nrj99">Activity type</dt> <dd class="svelte-14nrj99">${escape_html(activityLabel)}</dd></div> <div class="svelte-14nrj99"><dt class="svelte-14nrj99">Description</dt> <dd class="svelte-14nrj99">${escape_html(descriptionLabel)}</dd></div> <div class="svelte-14nrj99"><dt class="svelte-14nrj99">Key content</dt> <dd class="svelte-14nrj99">${escape_html(keyContentLabel)}</dd></div></dl> <h4 class="svelte-14nrj99">Topics</h4> `);
			if (topics.length > 0) {
				$$renderer.push("<!--[0-->");
				$$renderer.push(`<div class="modal__topics svelte-14nrj99" aria-label="Capture topics"><!--[-->`);
				const each_array = ensure_array_like(topics);
				for (let $$index = 0, $$length = each_array.length; $$index < $$length; $$index++) {
					let topic = each_array[$$index];
					$$renderer.push(`<span class="svelte-14nrj99">${escape_html(topic)}</span>`);
				}
				$$renderer.push(`<!--]--></div>`);
			} else {
				$$renderer.push("<!--[-1-->");
				$$renderer.push(`<p class="modal__muted svelte-14nrj99">No topics extracted.</p>`);
			}
			$$renderer.push(`<!--]--></section> <section class="modal__section svelte-14nrj99"><h3 class="svelte-14nrj99">Full extraction payload</h3> <pre class="svelte-14nrj99">${escape_html(extractionJson)}</pre></section></div></div></div>`);
		} else $$renderer.push("<!--[-1-->");
		$$renderer.push(`<!--]-->`);
		bind_props($$props, {
			open,
			capture,
			extraction
		});
	});
}
//#endregion
export { CaptureDetailsModal as t };
