import adapter from '@sveltejs/adapter-static';
import type { Config } from '@sveltejs/kit';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

export default <Config>{
	// Consult https://svelte.dev/docs/kit/integrations
	// for more information about preprocessors
	preprocess: vitePreprocess(),

	kit: {
		// adapter-auto only supports some environments, see https://svelte.dev/docs/kit/adapter-auto for a list.
		// If your environment is not supported, or you settled on a specific environment, switch out the adapter.
		// See https://svelte.dev/docs/kit/adapters for more information about adapters.
		adapter: adapter()
	}
};

/* Self-compilation setup: Do not edit! */
if (process.env.COMPILE_JS === 'true') {
	const esbuild = await import('esbuild');
	esbuild
		.build({
			entryPoints: ['svelte.config.ts'],
			outfile: 'svelte.config.js',
			minify: true,
			platform: 'node',
			banner: { js: '// This is an @generated config. Your changes will be overwritten. Please edit `svelte.config.ts` instead.' }
		})
		.catch(() => process.exit(1));
}
