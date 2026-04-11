<script lang="ts">
  import { page } from '$app/stores';

  import Layout from '$lib/components/Layout.svelte';
  import { navItems } from '$lib/utils/nav';
</script>

<Layout items={navItems} pathname={$page.url.pathname}>
  <slot />
</Layout>
