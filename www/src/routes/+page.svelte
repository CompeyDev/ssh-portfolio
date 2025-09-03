<script lang="ts">
	import CheckIcon from '$lib/components/checkIcon.svelte';
	import CopyIcon from '$lib/components/copyIcon.svelte';
	import LaunchIcon from '$lib/components/launchIcon.svelte';
	import { onMount } from 'svelte';
	import { crossfade, fade } from 'svelte/transition';

	const sshDestination = 'erica@devcomp.xyz';
	const command = `ssh -o SendEnv=TERM_PROGRAM ${sshDestination}`;
	const cursor = '█'

	let hasCopied = $state(false);
	let hasLaunched = $state(false);
	let animationFinished = $state(false);
	let commandText = $state(cursor);

	function copy(event: MouseEvent) {
		event.preventDefault();

		navigator.clipboard.writeText(command);
		hasCopied = true;

		setTimeout(() => (hasCopied = false), 1000);
	}

	function launch(event: MouseEvent) {
		event.preventDefault();

		window.location.href = `ssh://${sshDestination}`;
		hasLaunched = true;

		setTimeout(() => (hasLaunched = false), 1000);
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
				
				commandText = command.substring(0, commandText.length);
				const cursor = document.getElementsByClassName('cursor')[0];
				cursor.classList.remove('hidden');
			}
		}, 100);
	});
</script>

<main class="flex h-screen w-screen items-center justify-center">
	<div class="border-accent/50 relative flex h-[300px] w-[700px] flex-col rounded-lg border-2 p-4">
		<div class="flex items-center space-x-1.5">
			<pre class="text-primary inline font-bold"><span class="text-primary/50 select-none">$&nbsp;</span>{commandText}<span class="hidden cursor">{cursor}</span></pre>

			{#if animationFinished}
				<div class="flex flex-row space-x-2 text-accent/50 font-normal transition-all">
					<button class="hover:text-accent hover:cursor-pointer" onclick={launch} transition:fade={{delay: 500}}>
						{#if hasLaunched}
							<CheckIcon />
						{:else}
							<LaunchIcon />
						{/if}
					</button>

					<button class="hover:text-accent hover:cursor-pointer" onclick={copy} transition:fade={{delay: 500}}>
						{#if hasCopied}
							<CheckIcon />
						{:else}
							<CopyIcon />
						{/if}
					</button>
				</div>
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
