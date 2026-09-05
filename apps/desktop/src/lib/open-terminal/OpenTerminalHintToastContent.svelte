<script lang="ts">
    /**
     * The one-time INFO toast raised the first time "Open terminal here" runs on
     * a Mac carrying more than one terminal app. It says what just happened and
     * offers the row that picks a different app.
     *
     * Persistent on purpose: it's the only place this setting is ever advertised,
     * and it shows exactly once (the caller spends
     * `behavior.openTerminalHereToastSeen` when it raises this). A four-second
     * transient toast would be a coin flip on whether anyone read it.
     */
    import { dismissToast } from '$lib/ui/toast'
    import Button from '$lib/ui/Button.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { openSettingsToTerminalApp } from './terminal-app-setting'

    interface Props {
        toastId: string
    }

    const { toastId }: Props = $props()

    function handleDismiss(): void {
        dismissToast(toastId)
    }

    async function handleOpenSettings(): Promise<void> {
        dismissToast(toastId)
        await openSettingsToTerminalApp()
    }
</script>

<div class="content">
    <span class="message">{tString('commands.handler.openTerminalHere.hint')}</span>
    <div class="actions">
        <Button size="mini" variant="secondary" onclick={handleDismiss}
            >{tString('commands.handler.openTerminalHere.dismiss')}</Button
        >
        <Button size="mini" variant="primary" onclick={() => void handleOpenSettings()}
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
