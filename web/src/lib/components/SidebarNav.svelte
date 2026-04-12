<script lang="ts">
  import type { NavItem } from '$lib/utils/nav';

  export let items: NavItem[] = [];
  export let pathname = '/';

  function isActive(href: string): boolean {
    return href === '/' ? pathname === '/' : pathname.startsWith(href);
  }
</script>

<aside class="h-full w-64 fixed left-0 top-0 z-40 bg-slate-50/80 dark:bg-slate-900/60 glass-sidebar flex flex-col py-6 px-4 gap-2 tracking-tight border-r border-transparent dark:border-slate-800/50">
  <div class="flex items-center gap-3 px-2 mb-8">
    <div class="w-8 h-8 rounded-lg bg-primary-container flex items-center justify-center">
      <span class="material-symbols-outlined text-white text-sm" style="font-variation-settings: 'FILL' 1;">videocam</span>
    </div>
    <div class="flex flex-col">
      <span class="text-lg font-bold text-slate-900 dark:text-white leading-tight">Screencap</span>
      <span class="text-xs text-slate-500 dark:text-slate-400 font-medium">Screen Memory</span>
    </div>
  </div>

  <nav class="flex-1 flex flex-col gap-1">
    {#each items as item}
      <a
        href={item.href}
        class="flex items-center gap-3 px-3 py-2.5 rounded-lg transition-all duration-200 ease-in-out"
        class:active-nav={isActive(item.href)}
        class:inactive-nav={!isActive(item.href)}
        aria-current={isActive(item.href) ? 'page' : undefined}
      >
        <span
          class="material-symbols-outlined"
          style={isActive(item.href) ? "font-variation-settings: 'FILL' 1;" : ''}
        >{item.icon}</span>
        <span class="text-sm">{item.label}</span>
      </a>
    {/each}
  </nav>

  <div class="mt-auto px-2 pt-4 border-t border-slate-100 dark:border-slate-800/50">
    <a
      href="/settings"
      class="flex items-center gap-3 px-1 py-2 rounded-lg text-slate-500 dark:text-slate-400 hover:text-slate-700 dark:hover:text-slate-200 transition-colors text-sm"
      class:text-primary={pathname.startsWith('/settings')}
    >
      <span class="material-symbols-outlined text-[20px]">settings</span>
      <span>Settings</span>
    </a>
  </div>
</aside>

<style>
  .active-nav {
    background-color: rgb(219 234 254 / 0.6);
    color: #007AFF;
    font-weight: 600;
    border-left: 4px solid #007AFF;
  }

  :global(.dark) .active-nav {
    background-color: rgb(59 130 246 / 0.1);
    color: #60a5fa;
    border-left-color: #3b82f6;
  }

  .inactive-nav {
    color: rgb(100 116 139);
  }
  .inactive-nav:hover {
    background-color: rgb(226 232 240 / 0.5);
  }

  :global(.dark) .inactive-nav {
    color: rgb(148 163 184);
  }
  :global(.dark) .inactive-nav:hover {
    background-color: rgb(30 41 59 / 0.5);
  }
</style>
