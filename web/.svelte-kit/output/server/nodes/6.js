

export const index = 6;
let component_cache;
export const component = async () => component_cache ??= (await import('../entries/pages/stats/_page.svelte.js')).default;
export const imports = ["_app/immutable/nodes/6.24jRhnx3.js","_app/immutable/chunks/xpSSn9CY.js","_app/immutable/chunks/Cfug8aQt.js","_app/immutable/chunks/BuqrJCOf.js","_app/immutable/chunks/DAdVtK1J.js"];
export const stylesheets = ["_app/immutable/assets/6.DX5oTruk.css"];
export const fonts = [];
