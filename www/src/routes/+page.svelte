<script lang="ts">
	import CheckIcon from '$lib/components/checkIcon.svelte';
	import CopyIcon from '$lib/components/copyIcon.svelte';
	import { onMount } from 'svelte';
	import { crossfade, fade } from 'svelte/transition';

	const command = 'ssh -o SendEnv=TERM_PROGRAM erica@devcomp.xyz';
	const cursor = '█'

	let showCheckmark = $state(false);
	let animationFinished = $state(false);
	let commandText = $state(cursor);

	function copy(event: MouseEvent) {
		event.preventDefault();

		navigator.clipboard.writeText(command);
		showCheckmark = true;

		setTimeout(() => (showCheckmark = false), 1000);
	}

	function blinkCursor() {
		return setInterval(() => {
			if (commandText.charAt(commandText.length - 1) === cursor) {
				commandText = command + ' ';
			} else {
				commandText = command + cursor;
			}
		}, 500)
	}

	onMount(async () => {
		await new Promise((res) => setTimeout(res, 500));

		const animation = setInterval(() => {
			if (commandText.length - 1 < command.length) {
				commandText = command.substring(0, commandText.length) + cursor;
			} else {
				animationFinished = true;
				clearInterval(animation);
				blinkCursor();
			}
		}, 200);
	});
</script>

<main class="flex h-screen w-screen items-center justify-center">
	<div class="border-accent/50 relative flex h-[300px] w-[700px] flex-col rounded-lg border-2 p-4">
		<div class="flex items-center space-x-2">
			<pre class="text-primary inline font-bold"><span class="text-primary/50 select-none">$&nbsp;</span>{commandText}</pre>
			{#if animationFinished}
				<button class="text-accent/50 hover:text-accent font-normal transition-all hover:cursor-pointer" onclick={copy} transition:fade={{delay: 500}}>
					{#if showCheckmark}
						<CheckIcon />
					{:else}
						<CopyIcon />
					{/if}
				</button>
			{/if}
		</div>

		{#if animationFinished}
			<div class="flex flex-col ml-4" transition:fade={{delay: 500}}>
				<code class="text-accent/50"><span class="select-none">#&nbsp;</span>&lt;q&gt; &lt;ctrl-d&gt; &lt;ctrl-c&gt; &lt;esc&gt; - quit</code>
				<code class="text-accent/50"><span class="select-none">#&nbsp;</span>&lt;ctrl-z&gt; - suspend</code>
				<code class="text-accent/50"><span class="select-none">#&nbsp;</span>&lt;right&gt; - next tab</code>
				<code class="text-accent/50"><span class="select-none">#&nbsp;</span>&lt;left&gt; - prev tab</code>
				<code class="text-accent/50"><span class="select-none">#&nbsp;</span>&lt;down&gt; - next option</code>
				<code class="text-accent/50"><span class="select-none">#&nbsp;</span>&lt;up&gt; - prev option</code>
			</div>
		{/if}

		<div class="group absolute -bottom-6 right-0">
			<span class="group-hover:animate-sleep-z absolute -left-4 top-8 opacity-0">z</span>
			<span class="group-hover:animate-sleep-z absolute -left-3 top-8 opacity-0" style="animation-delay:0.3s;">z</span>
			<span class="group-hover:animate-sleep-z absolute -left-2 top-8 opacity-0" style="animation-delay:0.6s;">z</span>

			<pre>
 |\__/,|   (`\
 |_ _  |.--.) )
 ( T   )     /
(((^_(((/(((_>
		</pre>
		</div>
	</div>
</main>
