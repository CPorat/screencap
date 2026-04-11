

export const index = 6;
let component_cache;
export const component = async () => component_cache ??= (await import('../entries/pages/stats/_page.svelte.js')).default;
export const imports = ["_app/immutable/nodes/6.DP6EdvC7.js","_app/immutable/chunks/BRzsxr0b.js","_app/immutable/chunks/Cfug8aQt.js","_app/immutable/chunks/piEpTZFl.js","_app/immutable/chunks/DXaP8Rne.js"];
export const stylesheets = ["_app/immutable/assets/6.BOrRq9Xa.css"];
export const fonts = [];
