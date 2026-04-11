<script lang="ts">
  import { onMount } from 'svelte';

  import {
    getApps,
    getDailyInsight,
    getStats,
    type AppUsage,
    type DailyInsight,
    type SystemStats,
  } from '$lib/api';

  const EMPTY_STATS: SystemStats = {
    capture_count: 0,
    captures_today: 0,
    storage_bytes: 0,
    uptime_secs: 0,
  };

  let loading = true;
  let stats: SystemStats = EMPTY_STATS;
  let apps: AppUsage[] = [];
  let dailyInsight: DailyInsight | null = null;
  let today = formatLocalDate(new Date());

  $: topApps = apps.slice(0, 8);
  $: maxAppCount = topApps.reduce((max, app) => Math.max(max, app.capture_count), 0) || 1;
  $: dailySummary = extractDailySummary(dailyInsight);

  onMount(async () => {
    today = formatLocalDate(new Date());

    try {
      const [statsPayload, appsPayload, dailyPayload] = await Promise.all([
        getStats(),
        getApps(),
        getDailyInsight(today),
      ]);

      stats = statsPayload;
      apps = appsPayload;
      dailyInsight = dailyPayload;
    } catch (error) {
      console.error('Failed to load stats view', error);
    } finally {
      loading = false;
    }
  });

  function formatLocalDate(value: Date): string {
    const year = value.getFullYear();
    const month = String(value.getMonth() + 1).padStart(2, '0');
    const day = String(value.getDate()).padStart(2, '0');
    return `${year}-${month}-${day}`;
  }

  function formatBytes(bytes: number): string {
    if (!Number.isFinite(bytes) || bytes <= 0) {
      return '0 MB';
    }

    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    const exponent = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
    const value = bytes / 1024 ** exponent;
    const precision = exponent >= 2 ? 1 : 0;
    return `${value.toFixed(precision)} ${units[exponent]}`;
  }

  function formatUptime(totalSeconds: number): string {
    const safeSeconds = Number.isFinite(totalSeconds) ? Math.max(0, Math.floor(totalSeconds)) : 0;
    const days = Math.floor(safeSeconds / 86_400);
    const hours = Math.floor((safeSeconds % 86_400) / 3_600);
    const minutes = Math.floor((safeSeconds % 3_600) / 60);

    if (days > 0) {
      return `${days}d ${hours}h ${minutes}m`;
    }

    if (hours > 0) {
      return `${hours}h ${minutes}m`;
    }

    return `${Math.max(minutes, 1)}m`;
  }

  function extractDailySummary(insight: DailyInsight | null): string | null {
    if (!insight) {
      return null;
    }

    const data = insight.data;
    if (typeof data.narrative === 'string' && data.narrative.trim()) {
      return data.narrative;
    }

    if (typeof data.summary === 'string' && data.summary.trim()) {
      return data.summary;
    }

    if (typeof insight.narrative === 'string' && insight.narrative.trim()) {
      return insight.narrative;
    }

    return null;
  }
</script>

<svelte:head>
  <title>Screencap · Stats</title>
</svelte:head>

<section class="panel" aria-busy={loading}>
  <header class="panel__header">
    <p class="panel__section">Insights</p>
    <h2>Daily signal board</h2>
    <p class="panel__summary">System health, app activity concentration, and the generated day summary.</p>
  </header>

  <div class="stats-grid">
    <article class="stat-card">
      <p>Total captures</p>
      <strong>{stats.capture_count.toLocaleString()}</strong>
      <small>{stats.captures_today.toLocaleString()} captured today</small>
    </article>
    <article class="stat-card">
      <p>Storage used</p>
      <strong>{formatBytes(stats.storage_bytes)}</strong>
      <small>Screenshot and metadata footprint</small>
    </article>
    <article class="stat-card">
      <p>Daemon uptime</p>
      <strong>{formatUptime(stats.uptime_secs)}</strong>
      <small>{stats.uptime_secs.toLocaleString()} seconds online</small>
    </article>
  </div>

  <div class="panel-grid">
    <article class="panel-card">
      <header>
        <h3>Top apps</h3>
        <p>Most frequent app captures</p>
      </header>

      {#if loading}
        <p class="panel__state">Loading app activity…</p>
      {:else if topApps.length === 0}
        <p class="panel__state">No app activity captured yet.</p>
      {:else}
        <ul class="app-list">
          {#each topApps as app}
            {@const appWidth = Math.max(8, Math.round((app.capture_count / maxAppCount) * 100))}
            <li>
              <div class="app-list__label">
                <span>{app.app_name}</span>
                <strong>{app.capture_count.toLocaleString()}</strong>
              </div>
              <div class="app-list__track" role="presentation">
                <div class="app-list__bar" style={`width: ${appWidth}%`}></div>
              </div>
            </li>
          {/each}
        </ul>
      {/if}
    </article>

    <article class="panel-card">
      <header>
        <h3>Daily summary</h3>
        <p>{today}</p>
      </header>

      {#if loading}
        <p class="panel__state">Checking summary status…</p>
      {:else if dailySummary}
        <p class="daily-summary">{dailySummary}</p>
      {:else}
        <p class="panel__state">No summary generated yet for today.</p>
      {/if}
    </article>
  </div>
</section>

<style>
  .panel {
    height: 100%;
    padding: clamp(1.2rem, 2.6vw, 2rem);
    display: grid;
    align-content: start;
    gap: 1.05rem;
    overflow: auto;
    background:
      linear-gradient(154deg, rgb(29 35 51 / 94%), rgb(13 17 26 / 98%)),
      radial-gradient(circle at 82% 8%, rgb(112 255 227 / 16%), transparent 34%);
  }

  .panel__header {
    display: grid;
    gap: 0.52rem;
  }

  .panel__section {
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.22em;
    color: var(--pulse);
  }

  h2 {
    font-size: clamp(1.95rem, 4.2vw, 3.25rem);
  }

  .panel__summary,
  .panel__state {
    color: var(--paper-200);
    font-size: 0.88rem;
  }

  .stats-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 0.75rem;
  }

  .stat-card {
    border: 1px solid rgb(246 241 231 / 36%);
    border-radius: 0.9rem;
    background: linear-gradient(155deg, rgb(12 17 27 / 85%), rgb(13 15 23 / 62%));
    padding: 0.95rem;
    display: grid;
    gap: 0.45rem;
  }

  .stat-card p,
  .stat-card small {
    color: var(--paper-200);
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.11em;
  }

  .stat-card strong {
    font-family: var(--display-font);
    font-size: clamp(1.45rem, 2.75vw, 2.35rem);
    letter-spacing: 0.03em;
  }

  .panel-grid {
    display: grid;
    grid-template-columns: minmax(0, 1.15fr) minmax(0, 1fr);
    gap: 0.8rem;
  }

  .panel-card {
    border: 1px solid rgb(246 241 231 / 32%);
    border-radius: 0.95rem;
    background: rgb(8 12 20 / 58%);
    padding: 0.95rem;
    display: grid;
    gap: 0.7rem;
    align-content: start;
  }

  .panel-card header {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 0.75rem;
  }

  h3 {
    font-size: clamp(1.1rem, 2.4vw, 1.5rem);
  }

  .panel-card header p {
    font-size: 0.74rem;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--paper-200);
  }

  .app-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.56rem;
  }

  .app-list li {
    display: grid;
    gap: 0.34rem;
  }

  .app-list__label {
    display: flex;
    justify-content: space-between;
    gap: 0.5rem;
    align-items: baseline;
    font-size: 0.84rem;
  }

  .app-list__label span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .app-list__label strong {
    font-family: var(--display-font);
    font-size: 0.95rem;
    letter-spacing: 0.04em;
  }

  .app-list__track {
    width: 100%;
    height: 0.52rem;
    border-radius: 999px;
    border: 1px solid rgb(246 241 231 / 25%);
    background: rgb(12 15 23 / 84%);
    overflow: hidden;
  }

  .app-list__bar {
    height: 100%;
    border-radius: 999px;
    background: linear-gradient(90deg, var(--pulse), var(--surge));
  }

  .daily-summary {
    font-size: 0.92rem;
    color: var(--paper-100);
    line-height: 1.56;
  }

  @media (width <= 920px) {
    .stats-grid,
    .panel-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
