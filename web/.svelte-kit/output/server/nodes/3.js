

export const index = 3;
let component_cache;
export const component = async () => component_cache ??= (await import('../entries/pages/insights/_page.svelte.js')).default;
export const imports = ["_app/immutable/nodes/3.BLOuxpOw.js","_app/immutable/chunks/xpSSn9CY.js","_app/immutable/chunks/Cfug8aQt.js","_app/immutable/chunks/BuqrJCOf.js"];
export const stylesheets = ["_app/immutable/assets/3.CXeqjQhF.css"];
export const fonts = [];
