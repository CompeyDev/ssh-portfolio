<script lang="ts">
	import '../app.css';

	let { children } = $props();
</script>

<svelte:head>
	<title>devcomp.xyz</title>
	<link rel="icon" href="/favicon.png" />
	<meta content="compey's ssh home" property="og:title" />
	<meta content="#B59BFD" data-react-helmet="true" name="theme-color" />
</svelte:head>

{@render children?.()}
