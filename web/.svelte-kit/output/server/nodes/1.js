

export const index = 1;
let component_cache;
export const component = async () => component_cache ??= (await import('../entries/fallbacks/error.svelte.js')).default;
export const imports = ["_app/immutable/nodes/1.D8XVC1kw.js","_app/immutable/chunks/BRzsxr0b.js","_app/immutable/chunks/DEmO-16d.js","_app/immutable/chunks/Cfug8aQt.js"];
export const stylesheets = [];
export const fonts = [];
