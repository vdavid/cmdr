<script lang="ts">
    /**
     * The toast for a chosen terminal app that has been uninstalled: Cmdr opened
     * Terminal.app instead, and the caller has already reset the setting to it so
     * this can't repeat. The button leads to the row, in case the user has since
     * installed something else.
     *
     * `appName` is `null` when Cmdr carries no name for the stored choice (a
     * bundle id this version doesn't know). The unnamed wording covers that; ❌
     * never show the bundle id, which reads like a name and isn't one.
     */
    import { dismissToast } from '$lib/ui/toast'
    import Button from '$lib/ui/Button.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { openSettingsToTerminalApp } from './terminal-app-setting'

    interface Props {
        toastId: string
        appName: string | null
    }

    const { toastId, appName }: Props = $props()

    const message = $derived(
        appName === null
            ? tString('commands.handler.openTerminalHere.appMissingUnnamed')
            : tString('commands.handler.openTerminalHere.appMissing', { app: appName }),
    )

    async function handleOpenSettings(): Promise<void> {
        dismissToast(toastId)
        await openSettingsToTerminalApp()
    }
</script>

<div class="content">
    <span class="message">{message}</span>
    <div class="actions">
        <Button size="mini" variant="secondary" onclick={() => void handleOpenSettings()}
            >{tString('commands.handler.openTerminalHere.openSettings')}</Button
        >
    </div>
</div>

<style>
    .content {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        font-size: var(--font-size-sm);
    }

    .message {
        color: var(--color-text-primary);
    }

    .actions {
        display: flex;
        justify-content: flex-end;
        gap: var(--spacing-sm);
        margin-top: var(--spacing-md);
    }
</style>
