

export const index = 3;
let component_cache;
export const component = async () => component_cache ??= (await import('../entries/pages/insights/_page.svelte.js')).default;
export const imports = ["_app/immutable/nodes/3.92ZfEI1w.js","_app/immutable/chunks/cYO6TuvX.js","_app/immutable/chunks/Cfug8aQt.js","_app/immutable/chunks/k0Ot2IhZ.js"];
export const stylesheets = ["_app/immutable/assets/3.CXeqjQhF.css"];
export const fonts = [];
