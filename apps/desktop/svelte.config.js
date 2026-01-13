// Tauri doesn't have a Node.js server to do proper SSR
// so we use adapter-static with a fallback to index.html to put the site in SPA mode
// See: https://svelte.dev/docs/kit/single-page-apps
// See: https://v2.tauri.app/start/frontend/sveltekit/ for more info
import adapter from '@sveltejs/adapter-static'
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte'

// A11y warnings to suppress (same as in package.json check script)
const suppressedWarnings = [
    'a11y_no_noninteractive_element_interactions',
    'a11y_click_events_have_key_events',
    'a11y_no_noninteractive_tabindex',
    'a11y_interactive_supports_focus',
    'state_referenced_locally',
    'non_reactive_update',
]

/** @type {import('@sveltejs/kit').Config} */
const config = {
    preprocess: vitePreprocess(),
    compilerOptions: {
        warningFilter: (warning) => !suppressedWarnings.includes(warning.code),
    },
    kit: {
        adapter: adapter({
            fallback: 'index.html',
        }),
    },
}

export default config
