<script lang="ts">
  import Router from 'svelte-spa-router';
  import active from 'svelte-spa-router/active';
  import Insights from './routes/Insights.svelte';
  import Search from './routes/Search.svelte';
  import Stats from './routes/Stats.svelte';
  import Timeline from './pages/Timeline.svelte';

  const routes = {
    '/': Timeline,
    '/timeline': Timeline,
    '/insights': Insights,
    '/search': Search,
    '/stats': Stats,
  };

  const navItems = [
    { label: 'Timeline', path: '/timeline' },
    { label: 'Insights', path: '/insights' },
    { label: 'Search', path: '/search' },
    { label: 'Stats', path: '/stats' },
  ] as const;
</script>

<div class="noise" aria-hidden="true"></div>

<main class="shell">
  <header class="masthead">
    <p class="eyebrow">Screencap command center</p>
    <h1>Signal over noise.</h1>
    <p class="intro">
      A hard-edged interface for reading activity, finding patterns, and making decisions without
      distraction.
    </p>

    <nav aria-label="Primary navigation">
      {#each navItems as item}
        <a href="#{item.path}" use:active class="nav-link">{item.label}</a>
      {/each}
    </nav>
  </header>

  <section class="stage">
    <Router {routes} />
  </section>
</main>

<style>
  .noise {
    position: fixed;
    inset: 0;
    pointer-events: none;
    opacity: 0.18;
    background-image: radial-gradient(circle at 1px 1px, rgba(255, 255, 255, 0.28) 1px, transparent 0);
    background-size: 5px 5px;
    mix-blend-mode: soft-light;
    z-index: -1;
  }

  .shell {
    min-height: 100vh;
    max-width: 1100px;
    margin: 0 auto;
    padding: clamp(1.2rem, 2vw, 2.4rem);
    display: grid;
    grid-template-columns: minmax(260px, 360px) minmax(0, 1fr);
    gap: clamp(1rem, 2vw, 2rem);
  }

  .masthead {
    border: 3px solid var(--surface-border);
    background: var(--surface);
    padding: clamp(1.2rem, 2vw, 2rem);
    box-shadow: 12px 12px 0 var(--surface-shadow);
    align-self: start;
    position: sticky;
    top: 1.2rem;
  }

  .eyebrow {
    margin: 0;
    text-transform: uppercase;
    letter-spacing: 0.22em;
    font-size: 0.74rem;
    color: var(--accent);
  }

  h1 {
    font-family: var(--display-font);
    font-size: clamp(2.2rem, 7vw, 4.2rem);
    line-height: 0.92;
    margin: 0.7rem 0 1rem;
    text-transform: uppercase;
    letter-spacing: 0.02em;
  }

  .intro {
    margin: 0;
    color: var(--muted);
    max-width: 28ch;
  }

  nav {
    margin-top: 1.4rem;
    display: grid;
    gap: 0.65rem;
  }

  :global(.nav-link) {
    display: block;
    border: 2px solid var(--surface-border);
    text-decoration: none;
    color: var(--text);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 0.76rem;
    padding: 0.72rem 0.9rem;
    background: transparent;
    transition: transform 140ms ease, background-color 140ms ease;
  }

  :global(.nav-link:hover) {
    transform: translate(4px, -4px);
    background: var(--surface-lift);
  }

  :global(.nav-link.active) {
    background: var(--accent);
    color: #121212;
    border-color: #121212;
    box-shadow: 4px 4px 0 #121212;
  }

  .stage {
    border: 3px solid var(--surface-border);
    background: var(--surface);
    box-shadow: 12px 12px 0 var(--surface-shadow);
    min-height: 70vh;
    overflow: hidden;
  }

  @media (max-width: 900px) {
    .shell {
      grid-template-columns: 1fr;
    }

    .masthead {
      position: static;
    }
  }
</style>
