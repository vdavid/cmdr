<script lang="ts">
    /**
     * Self-dismissing INFO toast shown after "Hide function key bar" from the
     * bar's own right-click context menu. The inline link is the only place
     * this toast points back to the row that turns the bar on again.
     */
    import type { Snippet } from 'svelte'
    import LinkButton from '$lib/ui/LinkButton.svelte'
    import Trans from '$lib/intl/Trans.svelte'
    import { openSettingsWindow, settingAnchorId } from '$lib/settings/settings-window'
    import { getAppLogger } from '$lib/logging/logger'

    const log = getAppLogger('fileExplorer')

    async function handleOpenSettings(): Promise<void> {
        try {
            await openSettingsWindow(
                'function-key-bar-toast',
                ['Appearance', 'Listing'],
                settingAnchorId('appearance.showFunctionKeyBar'),
            )
        } catch (err) {
            log.warn('Failed to open Settings from the function-key-bar-hidden toast: {err}', { err: String(err) })
        }
    }
</script>

{#snippet settingsLink(children: Snippet)}<LinkButton onclick={handleOpenSettings}
        >{@render children()}</LinkButton
    >{/snippet}

<span class="message">
    <Trans key="fileExplorer.functionKeyBar.hiddenToast" snippets={{ settingsLink }} />
</span>

<style>
    .message {
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
    }
</style>
