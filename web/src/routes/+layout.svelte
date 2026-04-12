<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import '../app.css';

  import Layout from '$lib/components/Layout.svelte';
  import { navItems } from '$lib/utils/nav';
  import { initTheme } from '$lib/stores/theme';

  onMount(() => {
    initTheme();
  });
</script>

<Layout items={navItems} pathname={$page.url.pathname}>
  <slot />
</Layout>
