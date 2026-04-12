import { writable } from 'svelte/store';
import { browser } from '$app/environment';

type Theme = 'light' | 'dark' | 'system';

const STORAGE_KEY = 'screencap-theme';

function getInitialTheme(): Theme {
  if (!browser) return 'system';
  const stored = localStorage.getItem(STORAGE_KEY);
  if (stored === 'light' || stored === 'dark' || stored === 'system') return stored;
  return 'system';
}

function resolveTheme(preference: Theme): 'light' | 'dark' {
  if (preference !== 'system') return preference;
  if (!browser) return 'light';
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

function applyTheme(resolved: 'light' | 'dark'): void {
  if (!browser) return;
  document.documentElement.classList.toggle('dark', resolved === 'dark');
}

export const themePreference = writable<Theme>(getInitialTheme());

export function setTheme(preference: Theme): void {
  themePreference.set(preference);
  if (browser) {
    localStorage.setItem(STORAGE_KEY, preference);
  }
  applyTheme(resolveTheme(preference));
}

let initialized = false;

export function initTheme(): void {
  applyTheme(resolveTheme(getInitialTheme()));

  if (browser && !initialized) {
    initialized = true;
    window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
      if (getInitialTheme() === 'system') {
        applyTheme(resolveTheme('system'));
      }
    });
  }
}
