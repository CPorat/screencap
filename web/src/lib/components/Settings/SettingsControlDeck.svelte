<script lang="ts">
  import { onMount } from 'svelte';

  import { getHealth, getStats, type HealthResponse, type SystemStats } from '$lib/api';

  const CONFIG_PATH = '~/.screencap/config.toml';
  const CLI_COMMANDS = [
    {
      label: 'Prune stale captures',
      command: 'screencap prune --older-than 30d',
      detail: 'Reclaims screenshot + extraction storage older than your retention window.',
    },
    {
      label: 'Export daily summary',
      command: 'screencap export --date $(date +%F)',
      detail: 'Generates markdown output for today\'s synthesis and prints to stdout.',
    },
  ] as const;

  const EMPTY_STATS: SystemStats = {
    capture_count: 0,
    captures_today: 0,
    storage_bytes: 0,
    uptime_secs: 0,
  };

  const EMPTY_HEALTH: HealthResponse = {
    status: 'offline',
    uptime_secs: 0,
  };

  let loading = true;
  let refreshing = false;
  let stats: SystemStats = EMPTY_STATS;
  let health: HealthResponse = EMPTY_HEALTH;
  let lastUpdated: Date | null = null;
  let copyNotice = '';

  let refreshTimer: ReturnType<typeof setInterval> | null = null;
  let copyNoticeTimer: ReturnType<typeof setTimeout> | null = null;

  $: daemonOnline = health.status.toLowerCase() === 'ok';
  $: storageUsed = formatBytes(stats.storage_bytes);
  $: daemonUptime = formatDuration(health.uptime_secs);
  $: activeToday = formatDuration(
    daemonOnline ? Math.min(health.uptime_secs, secondsSinceLocalMidnight()) : 0
  );

  onMount(() => {
    void refreshState(true);
    refreshTimer = setInterval(() => {
      void refreshState(false);
    }, 30_000);

    return () => {
      if (refreshTimer) {
        clearInterval(refreshTimer);
      }

      if (copyNoticeTimer) {
        clearTimeout(copyNoticeTimer);
      }
    };
  });

  async function refreshState(initialLoad: boolean): Promise<void> {
    if (initialLoad) {
      loading = true;
    } else {
      refreshing = true;
    }

    const [statsPayload, healthPayload] = await Promise.all([getStats(), getHealth()]);

    stats = statsPayload;
    health = healthPayload;
    lastUpdated = new Date();
    loading = false;
    refreshing = false;
  }

  async function copyCommand(command: string): Promise<void> {
    if (!navigator.clipboard) {
      flashNotice('Clipboard unavailable. Copy in terminal manually.');
      return;
    }

    try {
      await navigator.clipboard.writeText(command);
      flashNotice(`Copied: ${command}`);
    } catch (error) {
      console.error('Failed to copy command', error);
      flashNotice('Copy failed. Run the command manually.');
    }
  }

  function flashNotice(message: string): void {
    copyNotice = message;

    if (copyNoticeTimer) {
      clearTimeout(copyNoticeTimer);
    }

    copyNoticeTimer = setTimeout(() => {
      copyNotice = '';
    }, 2600);
  }

  function secondsSinceLocalMidnight(): number {
    const now = new Date();
    const midnight = new Date(now);
    midnight.setHours(0, 0, 0, 0);

    return Math.max(0, Math.floor((now.getTime() - midnight.getTime()) / 1000));
  }

  function formatDuration(totalSeconds: number): string {
    const safeSeconds = Number.isFinite(totalSeconds) ? Math.max(0, Math.floor(totalSeconds)) : 0;
    const hours = Math.floor(safeSeconds / 3_600);
    const minutes = Math.floor((safeSeconds % 3_600) / 60);

    if (hours > 0) {
      return `${hours}h ${minutes}m`;
    }

    return `${Math.max(minutes, 0)}m`;
  }

  function formatBytes(bytes: number): string {
    if (!Number.isFinite(bytes) || bytes <= 0) {
      return '0 MB';
    }

    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    const exponent = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
    const value = bytes / 1024 ** exponent;

    return `${value.toFixed(exponent >= 2 ? 1 : 0)} ${units[exponent]}`;
  }

  function formatLastUpdated(value: Date | null): string {
    if (!value) {
      return '—';
    }

    return new Intl.DateTimeFormat(undefined, {
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
    }).format(value);
  }
</script>

<div class="space-y-8" aria-busy={loading || refreshing}>
  <!-- Page Header -->
  <div class="flex items-end justify-between">
    <div>
      <h1 class="text-[2.25rem] font-semibold tracking-tight text-on-surface">Settings</h1>
      <p class="text-secondary text-sm">Daemon telemetry and maintenance</p>
    </div>
    <button
      type="button"
      class="px-6 py-2.5 bg-primary text-white rounded-xl font-semibold text-sm hover:opacity-90 transition-opacity disabled:opacity-50 disabled:cursor-wait flex items-center gap-2"
      disabled={refreshing}
      on:click={() => void refreshState(false)}
    >
      <span class="material-symbols-outlined text-sm">refresh</span>
      {refreshing ? 'Refreshing...' : 'Refresh'}
    </button>
  </div>

  <!-- Daemon Offline Banner -->
  {#if !loading && !daemonOnline}
    <div class="bg-red-50 dark:bg-red-950/50 rounded-[24px] p-6 flex items-start gap-4" role="status" aria-live="polite">
      <div class="bg-red-100 dark:bg-red-900/50 p-2 rounded-xl">
        <span class="material-symbols-outlined text-red-600 dark:text-red-400">warning</span>
      </div>
      <div>
        <h3 class="text-sm font-bold text-red-900 dark:text-red-200">Daemon disconnected</h3>
        <p class="text-sm text-red-700 dark:text-red-300 mt-1">
          <code class="bg-red-100 dark:bg-red-900/50 px-1.5 py-0.5 rounded text-xs font-mono">/api/health</code> is unreachable.
          Start the daemon with <code class="bg-red-100 dark:bg-red-900/50 px-1.5 py-0.5 rounded text-xs font-mono">screencap start</code> and refresh this view.
        </p>
      </div>
    </div>
  {/if}

  <!-- Status Cards -->
  <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6">
    <div class="bg-surface-container-lowest rounded-[24px] p-6">
      <div class="flex items-center gap-3 mb-4">
        <div class="p-2 rounded-xl {daemonOnline ? 'bg-emerald-50 dark:bg-emerald-950' : 'bg-red-50 dark:bg-red-950'}">
          <span class="material-symbols-outlined text-lg {daemonOnline ? 'text-emerald-600 dark:text-emerald-400' : 'text-red-600 dark:text-red-400'}" style="font-variation-settings: 'FILL' 1;">
            {daemonOnline ? 'check_circle' : 'cancel'}
          </span>
        </div>
        <span class="text-[10px] font-bold text-secondary uppercase tracking-widest">Status</span>
      </div>
      <div class="text-2xl font-bold {daemonOnline ? 'text-emerald-600 dark:text-emerald-400' : 'text-red-600 dark:text-red-400'}">{daemonOnline ? 'Online' : 'Offline'}</div>
      <p class="text-[11px] text-on-surface-variant mt-1">Health: {health.status}</p>
    </div>

    <div class="bg-surface-container-lowest rounded-[24px] p-6">
      <div class="flex items-center gap-3 mb-4">
        <div class="p-2 rounded-xl bg-blue-50 dark:bg-blue-950">
          <span class="material-symbols-outlined text-lg text-blue-600 dark:text-blue-400">hard_drive</span>
        </div>
        <span class="text-[10px] font-bold text-secondary uppercase tracking-widest">Storage</span>
      </div>
      <div class="text-2xl font-bold text-on-surface">{storageUsed}</div>
      <p class="text-[11px] text-on-surface-variant mt-1">{stats.storage_bytes.toLocaleString()} bytes on disk</p>
    </div>

    <div class="bg-surface-container-lowest rounded-[24px] p-6">
      <div class="flex items-center gap-3 mb-4">
        <div class="p-2 rounded-xl bg-amber-50 dark:bg-amber-950">
          <span class="material-symbols-outlined text-lg text-amber-600 dark:text-amber-400">schedule</span>
        </div>
        <span class="text-[10px] font-bold text-secondary uppercase tracking-widest">Active Today</span>
      </div>
      <div class="text-2xl font-bold text-on-surface">{activeToday}</div>
      <p class="text-[11px] text-on-surface-variant mt-1">Since local midnight</p>
    </div>

    <div class="bg-surface-container-lowest rounded-[24px] p-6">
      <div class="flex items-center gap-3 mb-4">
        <div class="p-2 rounded-xl bg-purple-50 dark:bg-purple-950">
          <span class="material-symbols-outlined text-lg text-purple-600 dark:text-purple-400">timer</span>
        </div>
        <span class="text-[10px] font-bold text-secondary uppercase tracking-widest">Uptime</span>
      </div>
      <div class="text-2xl font-bold text-on-surface">{daemonUptime}</div>
      <p class="text-[11px] text-on-surface-variant mt-1">Last updated {formatLastUpdated(lastUpdated)}</p>
    </div>
  </div>

  <!-- Config & CLI -->
  <div class="grid grid-cols-1 lg:grid-cols-2 gap-6">
    <!-- Config -->
    <div class="bg-surface-container-lowest rounded-[24px] p-6">
      <div class="flex items-center justify-between mb-6">
        <div class="flex items-center gap-3">
          <span class="material-symbols-outlined text-primary">settings</span>
          <h2 class="text-sm font-bold uppercase tracking-widest text-on-surface">Configuration</h2>
        </div>
        <span class="text-[10px] text-secondary font-bold">Applied at daemon start</span>
      </div>

      <ol class="space-y-4 text-sm text-on-surface-variant">
        <li class="flex items-start gap-3">
          <span class="bg-primary text-white text-xs font-bold w-5 h-5 flex items-center justify-center rounded-full shrink-0 mt-0.5">1</span>
          <span>Open <code class="bg-surface-container-low px-1.5 py-0.5 rounded text-xs font-mono text-primary">{CONFIG_PATH}</code> in your editor.</span>
        </li>
        <li class="flex items-start gap-3">
          <span class="bg-primary text-white text-xs font-bold w-5 h-5 flex items-center justify-center rounded-full shrink-0 mt-0.5">2</span>
          <span>Adjust capture cadence, exclusions, provider keys, or retention limits.</span>
        </li>
        <li class="flex items-start gap-3">
          <span class="bg-primary text-white text-xs font-bold w-5 h-5 flex items-center justify-center rounded-full shrink-0 mt-0.5">3</span>
          <span>Restart daemon so new values are loaded.</span>
        </li>
      </ol>

      <div class="mt-6 space-y-2">
        <code class="block bg-surface-container-low px-4 py-3 rounded-xl text-xs font-mono text-on-surface-variant">open -e {CONFIG_PATH}</code>
        <code class="block bg-surface-container-low px-4 py-3 rounded-xl text-xs font-mono text-on-surface-variant">screencap stop && screencap start</code>
      </div>
    </div>

    <!-- CLI Maintenance -->
    <div class="bg-surface-container-lowest rounded-[24px] p-6">
      <div class="flex items-center gap-3 mb-6">
        <span class="material-symbols-outlined text-primary">terminal</span>
        <h2 class="text-sm font-bold uppercase tracking-widest text-on-surface">CLI Maintenance</h2>
      </div>

      <div class="space-y-4">
        {#each CLI_COMMANDS as item}
          <div class="bg-surface-container-low rounded-2xl p-4">
            <div class="flex items-start justify-between mb-2">
              <div>
                <h3 class="text-sm font-bold text-on-surface">{item.label}</h3>
                <p class="text-xs text-on-surface-variant mt-1">{item.detail}</p>
              </div>
              <button
                type="button"
                class="px-3 py-1.5 bg-surface-container-lowest text-primary rounded-lg text-xs font-bold hover:bg-primary hover:text-white transition-colors shrink-0 ml-4"
                on:click={() => void copyCommand(item.command)}
              >
                Copy
              </button>
            </div>
            <code class="block bg-surface-container-lowest px-3 py-2 rounded-lg text-xs font-mono text-on-surface-variant mt-3">{item.command}</code>
          </div>
        {/each}
      </div>
    </div>
  </div>

  <!-- Footer Stats -->
  <div class="bg-surface-container-low rounded-[24px] px-6 py-4 flex items-center justify-between">
    <div class="flex items-center gap-2">
      <span class="material-symbols-outlined text-primary text-[18px]">photo_camera</span>
      <span class="text-xs font-bold text-secondary uppercase tracking-wider">Captures today</span>
      <span class="text-sm font-bold text-on-surface ml-1">{stats.captures_today.toLocaleString()}</span>
    </div>
    <div class="flex items-center gap-2">
      <span class="material-symbols-outlined text-primary text-[18px]">database</span>
      <span class="text-xs font-bold text-secondary uppercase tracking-wider">Lifetime captures</span>
      <span class="text-sm font-bold text-on-surface ml-1">{stats.capture_count.toLocaleString()}</span>
    </div>
  </div>

  <!-- Copy Notice -->
  {#if copyNotice}
    <div class="fixed bottom-6 right-6 bg-primary text-white px-4 py-2 rounded-xl text-xs font-bold shadow-xl z-50" role="status" aria-live="polite">
      {copyNotice}
    </div>
  {/if}
</div>
