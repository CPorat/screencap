<script lang="ts">
  import SidebarNav from '$lib/components/SidebarNav.svelte';
  import type { NavItem } from '$lib/utils/nav';
  import { themePreference, setTheme } from '$lib/stores/theme';

  export let items: NavItem[] = [];
  export let pathname = '/';

  const THEME_META = {
    light: { next: 'dark' as const, icon: 'light_mode', label: 'Light' },
    dark:  { next: 'system' as const, icon: 'dark_mode', label: 'Dark' },
    system: { next: 'light' as const, icon: 'routine', label: 'System' },
  };

  function cycleTheme(): void {
    setTheme(THEME_META[$themePreference].next);
  }

  $: themeMeta = THEME_META[$themePreference];
</script>

<SidebarNav {items} {pathname} />

<header class="fixed top-0 right-0 w-[calc(100%-16rem)] z-30 bg-surface-container-lowest/70 glass-header flex items-center justify-between px-8 h-16 shadow-sm shadow-outline/10">
  <div class="flex items-center gap-4 flex-1">
    <div class="relative w-full max-w-md">
      <span class="material-symbols-outlined absolute left-3 top-1/2 -translate-y-1/2 text-on-surface-variant text-[20px]">search</span>
      <input
        type="text"
        class="w-full bg-surface-container-low border-none rounded-xl py-2 pl-10 pr-4 text-sm focus:ring-2 focus:ring-primary/20 transition-all placeholder:text-on-surface-variant"
        placeholder="Search sessions, projects, or insights..."
      />
    </div>
  </div>
  <div class="flex items-center gap-3 text-on-surface-variant">
    <button
      class="flex items-center gap-1.5 px-3 py-1.5 rounded-lg hover:bg-surface-container-high transition-colors text-xs font-medium"
      on:click={cycleTheme}
      title="Theme: {themeMeta.label}"
      aria-label="Toggle theme (currently {themeMeta.label})"
    >
      <span class="material-symbols-outlined text-[18px]">{themeMeta.icon}</span>
      <span class="hidden sm:inline">{themeMeta.label}</span>
    </button>
    <button class="hover:text-on-surface transition-colors p-1.5 rounded-lg hover:bg-surface-container-high" aria-label="Capture status">
      <span class="material-symbols-outlined">sensors</span>
    </button>
    <a href="/settings" class="hover:text-on-surface transition-colors p-1.5 rounded-lg hover:bg-surface-container-high" aria-label="Settings">
      <span class="material-symbols-outlined">settings</span>
    </a>
  </div>
</header>

<main class="ml-64 pt-20 pb-12 px-8 min-h-screen bg-background">
  <slot />
</main>
