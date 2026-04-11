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
      detail: 'Generates markdown output for today’s synthesis and prints to stdout.',
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

<section class="deck" aria-busy={loading || refreshing}>
  <header class="deck__header">
    <div>
      <p class="deck__eyebrow">Operations</p>
      <h2>Settings command deck</h2>
      <p class="deck__summary">
        Live daemon telemetry from <code>/api/health</code> and <code>/api/stats</code>, plus quick
        maintenance commands.
      </p>
    </div>

    <button
      class="deck__refresh"
      type="button"
      disabled={refreshing}
      on:click={() => void refreshState(false)}
    >
      {refreshing ? 'Refreshing…' : 'Refresh signal'}
    </button>
  </header>

  {#if !loading && !daemonOnline}
    <aside class="deck__offline" role="status" aria-live="polite">
      <strong>Daemon disconnected</strong>
      <p>
        <code>/api/health</code> is unreachable. Start the daemon with <code>screencap start</code> and
        refresh this view.
      </p>
    </aside>
  {/if}

  <div class="stats-grid">
    <article class="metric">
      <p>Status</p>
      <strong class:offline={!daemonOnline}>{daemonOnline ? 'Online' : 'Offline'}</strong>
      <small>Health: {health.status}</small>
    </article>

    <article class="metric">
      <p>Total storage</p>
      <strong>{storageUsed}</strong>
      <small>{stats.storage_bytes.toLocaleString()} bytes on disk</small>
    </article>

    <article class="metric">
      <p>Active time today</p>
      <strong>{activeToday}</strong>
      <small>Daemon online window since local midnight</small>
    </article>

    <article class="metric">
      <p>Daemon uptime</p>
      <strong>{daemonUptime}</strong>
      <small>Last updated {formatLastUpdated(lastUpdated)}</small>
    </article>
  </div>

  <div class="detail-grid">
    <article class="detail-card">
      <header>
        <h3>Edit config.toml</h3>
        <p>Applied at daemon start</p>
      </header>

      <ol>
        <li>Open <code>{CONFIG_PATH}</code> in your editor.</li>
        <li>Adjust capture cadence, exclusions, provider keys, or retention limits.</li>
        <li>Restart daemon so new values are loaded.</li>
      </ol>

      <div class="detail-card__commands">
        <code>open -e {CONFIG_PATH}</code>
        <code>screencap stop && screencap start</code>
      </div>
    </article>

    <article class="detail-card detail-card--commands">
      <header>
        <h3>CLI maintenance</h3>
        <p>Operational shortcuts</p>
      </header>

      <ul>
        {#each CLI_COMMANDS as item}
          <li>
            <div>
              <strong>{item.label}</strong>
              <p>{item.detail}</p>
              <code>{item.command}</code>
            </div>
            <button type="button" on:click={() => void copyCommand(item.command)}>Copy</button>
          </li>
        {/each}
      </ul>
    </article>
  </div>

  <footer class="deck__footer">
    <p>
      Captures today <strong>{stats.captures_today.toLocaleString()}</strong>
    </p>
    <p>
      Lifetime captures <strong>{stats.capture_count.toLocaleString()}</strong>
    </p>
  </footer>

  {#if copyNotice}
    <p class="copy-notice" role="status" aria-live="polite">{copyNotice}</p>
  {/if}
</section>

<style>
  .deck {
    height: 100%;
    padding: clamp(1.15rem, 2.5vw, 2rem);
    display: grid;
    align-content: start;
    gap: 0.92rem;
    overflow: auto;
    background:
      radial-gradient(circle at 8% 16%, rgb(255 179 71 / 14%), transparent 34%),
      radial-gradient(circle at 90% 10%, rgb(112 255 227 / 18%), transparent 36%),
      linear-gradient(152deg, rgb(25 30 45 / 96%), rgb(10 13 22 / 97%));
  }

  .deck__header {
    display: flex;
    justify-content: space-between;
    align-items: start;
    gap: 1rem;
  }

  .deck__eyebrow {
    font-size: 0.69rem;
    letter-spacing: 0.24em;
    text-transform: uppercase;
    color: var(--ember);
  }

  h2 {
    font-size: clamp(1.9rem, 4vw, 3rem);
    margin-top: 0.28rem;
  }

  .deck__summary {
    margin-top: 0.44rem;
    color: var(--paper-200);
    font-size: 0.86rem;
    max-width: 58ch;
  }

  .deck__summary code {
    font-size: 0.74rem;
  }

  .deck__refresh {
    border: 1px solid rgb(246 241 231 / 44%);
    border-radius: 999px;
    background: linear-gradient(130deg, rgb(112 255 227 / 26%), rgb(255 179 71 / 28%));
    color: #0e1421;
    font-family: var(--body-font);
    font-size: 0.74rem;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    font-weight: 700;
    padding: 0.56rem 1rem;
    cursor: pointer;
    transition: transform 180ms ease, box-shadow 180ms ease;
  }

  .deck__refresh:hover,
  .deck__refresh:focus-visible {
    transform: translate(0.14rem, -0.14rem);
    box-shadow: 0.3rem 0.3rem 0 rgb(17 21 33 / 72%);
    outline: none;
  }

  .deck__refresh:disabled {
    cursor: wait;
    opacity: 0.7;
    transform: none;
    box-shadow: none;
  }

  .deck__offline {
    border: 1px solid rgb(255 78 166 / 62%);
    border-radius: 0.95rem;
    background: linear-gradient(140deg, rgb(82 18 46 / 62%), rgb(46 13 27 / 55%));
    padding: 0.75rem 0.9rem;
    display: grid;
    gap: 0.25rem;
  }

  .deck__offline strong {
    display: block;
    font-size: 0.9rem;
    letter-spacing: 0.09em;
    text-transform: uppercase;
  }

  .deck__offline p {
    color: var(--paper-200);
    font-size: 0.84rem;
  }

  .stats-grid {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 0.72rem;
  }

  .metric {
    border: 1px solid rgb(246 241 231 / 34%);
    border-radius: 0.95rem;
    background: linear-gradient(154deg, rgb(10 14 24 / 84%), rgb(14 19 31 / 70%));
    padding: 0.84rem;
    display: grid;
    gap: 0.42rem;
    box-shadow: inset 0 0 0 1px rgb(255 255 255 / 4%);
  }

  .metric p,
  .metric small {
    margin: 0;
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--paper-200);
  }

  .metric strong {
    font-family: var(--display-font);
    font-size: clamp(1.28rem, 2.7vw, 2rem);
    letter-spacing: 0.03em;
  }

  .metric strong.offline {
    color: var(--surge);
  }

  .detail-grid {
    display: grid;
    grid-template-columns: minmax(0, 1.08fr) minmax(0, 1fr);
    gap: 0.72rem;
  }

  .detail-card {
    border: 1px solid rgb(246 241 231 / 32%);
    border-radius: 0.95rem;
    background: rgb(6 9 17 / 65%);
    padding: 0.95rem;
    display: grid;
    gap: 0.78rem;
  }

  .detail-card header {
    display: flex;
    justify-content: space-between;
    gap: 0.75rem;
    align-items: baseline;
  }

  h3 {
    font-size: clamp(1rem, 2.2vw, 1.45rem);
  }

  .detail-card header p {
    color: var(--paper-200);
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.11em;
  }

  .detail-card ol {
    margin: 0;
    padding-left: 1.1rem;
    display: grid;
    gap: 0.45rem;
    color: var(--paper-200);
    font-size: 0.86rem;
  }

  .detail-card__commands {
    display: grid;
    gap: 0.42rem;
  }

  code {
    border: 1px solid rgb(246 241 231 / 24%);
    border-radius: 0.52rem;
    background: rgb(9 13 22 / 82%);
    color: var(--pulse);
    padding: 0.36rem 0.48rem;
    font-size: 0.73rem;
    font-family: 'SFMono-Regular', Consolas, monospace;
    overflow-x: auto;
  }

  .detail-card--commands ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.62rem;
  }

  .detail-card--commands li {
    border: 1px solid rgb(246 241 231 / 20%);
    border-radius: 0.82rem;
    padding: 0.7rem;
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 0.72rem;
    align-items: start;
    background: rgb(11 16 27 / 58%);
  }

  .detail-card--commands strong {
    display: block;
    font-size: 0.77rem;
    letter-spacing: 0.11em;
    text-transform: uppercase;
  }

  .detail-card--commands p {
    margin: 0.35rem 0;
    color: var(--paper-200);
    font-size: 0.82rem;
  }

  .detail-card--commands button {
    align-self: center;
    border: 1px solid rgb(246 241 231 / 46%);
    border-radius: 999px;
    background: transparent;
    color: var(--paper-100);
    padding: 0.38rem 0.8rem;
    font-family: var(--body-font);
    font-size: 0.68rem;
    letter-spacing: 0.11em;
    text-transform: uppercase;
    cursor: pointer;
    transition: border-color 160ms ease, color 160ms ease, transform 160ms ease;
  }

  .detail-card--commands button:hover,
  .detail-card--commands button:focus-visible {
    border-color: var(--pulse);
    color: var(--pulse);
    transform: translateY(-1px);
    outline: none;
  }

  .deck__footer {
    border: 1px solid rgb(246 241 231 / 24%);
    border-radius: 0.85rem;
    background: rgb(8 11 19 / 62%);
    padding: 0.68rem 0.82rem;
    display: flex;
    justify-content: space-between;
    gap: 0.75rem;
    font-size: 0.78rem;
    text-transform: uppercase;
    letter-spacing: 0.09em;
    color: var(--paper-200);
  }

  .deck__footer strong {
    font-size: 0.92rem;
    color: var(--paper-100);
  }

  .copy-notice {
    position: sticky;
    bottom: 0.4rem;
    justify-self: end;
    border: 1px solid rgb(112 255 227 / 58%);
    border-radius: 999px;
    padding: 0.34rem 0.75rem;
    background: rgb(11 19 25 / 88%);
    color: var(--pulse);
    font-size: 0.72rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  @media (max-width: 1080px) {
    .stats-grid,
    .detail-grid {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }

  @media (max-width: 760px) {
    .deck__header {
      flex-direction: column;
    }

    .stats-grid,
    .detail-grid {
      grid-template-columns: 1fr;
    }

    .detail-card--commands li,
    .deck__footer {
      grid-template-columns: 1fr;
      display: grid;
    }
  }
</style>
