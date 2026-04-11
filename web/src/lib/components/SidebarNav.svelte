<script lang="ts">
  import { BarChart3, BrainCircuit, ScanSearch, TimerReset } from 'lucide-svelte';

  import type { NavItem } from '$lib/utils/nav';

  export let items: NavItem[] = [];
  export let pathname = '/';

  const iconByKey = {
    timeline: TimerReset,
    insights: BrainCircuit,
    search: ScanSearch,
    stats: BarChart3,
  } as const;

  function isActive(href: string): boolean {
    return href === '/' ? pathname === '/' : pathname.startsWith(href);
  }
</script>

<aside class="rail">
  <header class="rail__header">
    <p class="rail__eyebrow">Signal Cartographer</p>
    <h1>Screencap</h1>
    <p class="rail__lede">
      Local-first memory console tuned for timeline review, synthesis inspection, and high-recall
      search.
    </p>
  </header>

  <nav class="rail__nav" aria-label="Primary">
    {#each items as item}
      {@const Icon = iconByKey[item.icon]}
      <a
        href={item.href}
        class:active={isActive(item.href)}
        aria-current={isActive(item.href) ? 'page' : undefined}
        aria-label={`${item.label}: ${item.caption}`}
>
        <span class="rail__icon" aria-hidden="true"><Icon size={17} strokeWidth={2.35} /></span>
        <span>
          <strong>{item.label}</strong>
          <small>{item.caption}</small>
        </span>
      </a>
    {/each}
  </nav>

  <footer class="rail__footer">
    <p>US-015 foundation shell</p>
    <p>Routes and design system staged</p>
  </footer>
</aside>

<style>
  .rail {
    position: sticky;
    top: 1rem;
    align-self: start;
    border: 2px solid var(--paper-100);
    border-radius: var(--radius-card);
    box-shadow: var(--shadow-hard);
    background: linear-gradient(160deg, rgb(28 31 43 / 96%), rgb(17 19 29 / 98%));
    padding: 1.2rem;
    display: grid;
    gap: 1.1rem;
  }

  .rail__header {
    display: grid;
    gap: 0.7rem;
  }

  .rail__eyebrow {
    font-size: 0.68rem;
    text-transform: uppercase;
    letter-spacing: 0.22em;
    color: var(--ember);
  }

  h1 {
    font-size: clamp(1.95rem, 3.8vw, 2.75rem);
  }

  .rail__lede {
    color: var(--paper-200);
    font-size: 0.86rem;
  }

  .rail__nav {
    display: grid;
    gap: 0.55rem;
  }

  .rail__nav a {
    border: 2px solid rgb(246 241 231 / 44%);
    border-radius: 0.9rem;
    background: linear-gradient(170deg, rgb(67 75 101 / 42%), rgb(24 27 38 / 86%));
    padding: 0.72rem 0.78rem;
    text-decoration: none;
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 0.65rem;
    align-items: center;
    transition: transform 160ms ease, border-color 160ms ease, box-shadow 160ms ease;
  }

  .rail__nav a:hover,
  .rail__nav a:focus-visible {
    transform: translate(0.2rem, -0.2rem);
    border-color: var(--pulse);
    box-shadow: 0.4rem 0.4rem 0 rgb(112 255 227 / 24%);
    outline: none;
  }

  .rail__nav a.active {
    border-color: #111521;
    background: linear-gradient(120deg, var(--ember), #ffd27a);
    color: #111521;
    box-shadow: 0.45rem 0.45rem 0 #111521;
  }

  .rail__icon {
    width: 2rem;
    aspect-ratio: 1;
    border-radius: 0.62rem;
    border: 1px solid rgb(246 241 231 / 55%);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: rgb(15 18 28 / 50%);
  }

  .rail__nav a.active .rail__icon {
    border-color: #111521;
    background: rgb(255 255 255 / 35%);
  }

  strong,
  small {
    display: block;
  }

  strong {
    font-size: 0.86rem;
    letter-spacing: 0.09em;
    text-transform: uppercase;
  }

  small {
    font-size: 0.72rem;
    letter-spacing: 0.05em;
    color: rgb(246 241 231 / 80%);
  }

  .rail__nav a.active small {
    color: #0b0e17;
  }

  .rail__footer {
    border-top: 1px solid rgb(246 241 231 / 28%);
    padding-top: 0.85rem;
    display: grid;
    gap: 0.22rem;
    font-size: 0.71rem;
    text-transform: uppercase;
    letter-spacing: 0.09em;
    color: var(--paper-200);
  }

  @media (max-width: 980px) {
    .rail {
      position: static;
    }
  }
</style>
