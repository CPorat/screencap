/** @type {import('tailwindcss').Config} */
const config = {
  content: ['./src/**/*.{html,js,svelte,ts}'],
  theme: {
    extend: {
      colors: {
        ink: {
          950: 'var(--ink-950)',
          900: 'var(--ink-900)',
          800: 'var(--ink-800)',
        },
        pulse: 'var(--pulse)',
        surge: 'var(--surge)',
        ember: 'var(--ember)',
      },
      boxShadow: {
        hard: 'var(--shadow-hard)',
      },
    },
  },
  plugins: [],
};

export default config;
