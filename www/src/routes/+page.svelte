<script lang="ts">
	import CheckIcon from '$lib/components/checkIcon.svelte';
	import CopyIcon from '$lib/components/copyIcon.svelte';
	import LaunchIcon from '$lib/components/launchIcon.svelte';
	import { onMount } from 'svelte';
	import { crossfade, fade } from 'svelte/transition';

	const repoUrl = 'https://github.com/CompeyDev/ssh-portfolio';
	const sshDestination = 'erica@devcomp.xyz';
	const command = `ssh -o SendEnv=TERM_PROGRAM ${sshDestination}`;
	const cursor = '█';

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
		}, 500);
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
	<div class="relative flex h-[300px] w-[750px] flex-col rounded-lg border-2 border-accent/50 p-4">
		<div class="flex items-center space-x-1.5">
			<pre class="inline font-bold text-primary"><span class="text-primary/50 select-none">$&nbsp;</span>{commandText}<span class="cursor hidden">{cursor}</span></pre>

			{#if animationFinished}
				<div class="flex flex-row space-x-2 font-normal text-accent/50 transition-all">
					<button class="hover:cursor-pointer hover:text-accent" onclick={launch} transition:fade={{ delay: 500 }}>
						{#if hasLaunched}
							<CheckIcon />
						{:else}
							<LaunchIcon />
						{/if}
					</button>

					<button class="hover:cursor-pointer hover:text-accent" onclick={copy} transition:fade={{ delay: 500 }}>
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
			<div transition:fade={{ delay: 500 }} class="flex flex-col">
				<div class="ml-4 flex flex-col">
					<code class="text-accent/50"><span class="select-none">#&nbsp;</span>&lt;q&gt; &lt;ctrl-d&gt; &lt;ctrl-c&gt; &lt;esc&gt; - quit</code>
					<code class="text-accent/50"><span class="select-none">#&nbsp;</span>&lt;ctrl-z&gt; - suspend</code>
					<code class="text-accent/50"><span class="select-none">#&nbsp;</span>&lt;right&gt; - next tab</code>
					<code class="text-accent/50"><span class="select-none">#&nbsp;</span>&lt;left&gt; - prev tab</code>
					<code class="text-accent/50"><span class="select-none">#&nbsp;</span>&lt;down&gt; - next option</code>
					<code class="text-accent/50"><span class="select-none">#&nbsp;</span>&lt;up&gt; - prev option</code>
				</div>
				<div class="mt-2 flex flex-col">
					<code class="font-bold text-accent/50"><span class="text-primary/50 select-none">$&nbsp;</span><span class="select-none">#&nbsp;</span>...or view the src code:</code>
					<code class="font-bold text-primary"><span class="text-primary/50 select-none">$&nbsp;</span>git clone <a target="_blank" href={repoUrl} class="text-blue-300 underline">{repoUrl}</a></code>
				</div>
			</div>
		{/if}

		<div class="group absolute right-0 -bottom-6">
			<span class="absolute top-8 -left-4 opacity-0 group-hover:animate-sleep-z">z</span>
			<span class="absolute top-8 -left-3 opacity-0 group-hover:animate-sleep-z" style="animation-delay:0.3s;">z</span>
			<span class="absolute top-8 -left-2 opacity-0 group-hover:animate-sleep-z" style="animation-delay:0.6s;">z</span>

			<pre>
 |\__/,|   (`\
 |_ _  |.--.) )
 ( T   )     /
(((^_(((/(((_>
		</pre>
		</div>
	</div>
</main>
