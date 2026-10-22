<script lang="ts">
    /**
     * What a window shows when its page threw while rendering, in place of SvelteKit's bare
     * "500 / Internal Error". `hooks.client.ts` has already logged the throw (so an opted-in
     * auto-send ships it) and handed over its `Name: message` as `page.error.detail`. The
     * panel itself, shared with the root layout's `WindowBoundary`, is `PageCrashPanel`.
     */
    import { page } from '$app/state'
    import PageCrashPanel from '#lib/error-reporter/PageCrashPanel.svelte'
    import { reloadWindow } from '#lib/error-reporter/reload-window.js'
</script>

<PageCrashPanel detail={page.error?.detail ?? null} onReload={reloadWindow} />
