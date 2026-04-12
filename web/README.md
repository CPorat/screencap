# Screencap Web UI

Embedded Svelte frontend for the Screencap daemon. Built to static files and served by the Rust binary on `localhost:7878`.

## Stack

- **SvelteKit** with `adapter-static` (fallback `index.html`)
- **Tailwind CSS v4** — CSS-first configuration via `@theme` in `app.css`
- **Chart.js** for stats visualizations
- **Google Material Symbols Outlined** for icons
- **Inter** font family

## Design system

The UI follows a "Digital Architect" design language: macOS-inspired glassmorphism, MD3 color tokens, tonal layering, and ambient shadows. Light theme by default with full dark mode support toggled via the header.

Color tokens and theme overrides live in `src/app.css`. Components use Tailwind utilities referencing those tokens (e.g. `text-on-surface`, `bg-surface-container`), plus `dark:` variants where needed.

## Development

```bash
# From the repo root:
make web-dev       # Svelte dev server on :5173, proxies /api to :7878
make web-build     # Production build to web/dist/
make web-check     # TypeScript/Svelte type-check
```

The dev server proxies all `/api` requests to the running Rust daemon, so start the daemon first:

```bash
make dev           # in a separate terminal
```

## Project structure

```
src/
  app.css             # Tailwind @theme, global styles, dark mode overrides
  app.html            # HTML shell (fonts, icons)
  routes/
    +layout.svelte    # Root layout (theme init)
    +page.svelte      # Timeline route
    Timeline.svelte
    insights/
    search/
    stats/
    settings/
  lib/
    components/       # Shared components (Layout, SidebarNav, CaptureCard, etc.)
    stores/           # Svelte stores (theme)
    utils/            # Navigation metadata
```
