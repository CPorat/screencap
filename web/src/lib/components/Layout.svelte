<script lang="ts">
  import { onMount } from 'svelte';

  import SidebarNav from '$lib/components/SidebarNav.svelte';
  import { formatConsoleTime } from '$lib/utils/time';
  import type { NavItem } from '$lib/utils/nav';

  export let items: NavItem[] = [];
  export let pathname = '/';

  let consoleTime = formatConsoleTime(new Date());

  onMount(() => {
    const timer = setInterval(() => {
      consoleTime = formatConsoleTime(new Date());
    }, 30_000);

    return () => clearInterval(timer);
  });
</script>

<div class="atmosphere" aria-hidden="true"></div>

<main class="shell">
  <SidebarNav {items} {pathname} />

  <section class="stage" aria-live="polite">
    <header class="stage__header">
      <p class="stage__eyebrow">Screencap UI Shell</p>
      <p class="stage__clock" aria-label="Current local time">{consoleTime}</p>
    </header>

    <div class="stage__body">
      <slot />
    </div>
  </section>
</main>

<style>
  .atmosphere {
    position: fixed;
    inset: 0;
    pointer-events: none;
    background:
      conic-gradient(from 245deg at 94% 7%, rgb(255 78 166 / 8%), transparent 28%),
      radial-gradient(circle at 5% 88%, rgb(255 179 71 / 10%), transparent 32%);
  }

  .shell {
    min-height: 100vh;
    width: min(1240px, 100% - 2rem);
    margin: 0 auto;
    padding: clamp(1rem, 1.8vw, 1.9rem) 0;
    display: grid;
    grid-template-columns: minmax(270px, 320px) minmax(0, 1fr);
    gap: clamp(0.9rem, 1.8vw, 1.4rem);
  }

  .stage {
    border: 2px solid var(--paper-100);
    border-radius: var(--radius-card);
    background: linear-gradient(160deg, rgb(25 28 42 / 96%), rgb(13 15 23 / 98%));
    box-shadow: var(--shadow-hard);
    min-height: min(84vh, 920px);
    display: grid;
    grid-template-rows: auto 1fr;
    overflow: hidden;
  }

  .stage__header {
    padding: 0.9rem 1.2rem;
    border-bottom: 1px solid rgb(246 241 231 / 26%);
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 0.75rem;
    background: linear-gradient(90deg, rgb(112 255 227 / 10%), transparent 55%);
  }

  .stage__eyebrow,
  .stage__clock {
    font-size: 0.73rem;
    text-transform: uppercase;
    letter-spacing: 0.16em;
    color: var(--paper-200);
  }

  .stage__clock {
    color: var(--pulse);
  }

  .stage__body {
    min-height: 0;
  }

  @media (max-width: 980px) {
    .shell {
      grid-template-columns: 1fr;
    }

    .stage {
      min-height: min(76vh, 760px);
    }
  }
</style>
