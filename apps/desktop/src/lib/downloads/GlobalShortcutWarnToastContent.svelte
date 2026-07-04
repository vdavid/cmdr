<script lang="ts">
    /**
     * First-trigger warn toast for the global go-to-latest hotkey (default ⌃⌥⌘J).
     *
     * Fires once per fresh binding (the `acknowledged` flag, reset whenever
     * the user rebinds, controls suppression). The toast itself is purely
     * informational — the bridge already flipped `acknowledged = true` BEFORE
     * opening this toast, so the buttons don't re-write that bit. "Turn it
     * off" additionally flips `enabled = false` and asks the backend to
     * unregister live.
     *
     * Props-only. We snapshot the binding string at toast-creation time so a
     * mid-flight remap doesn't mutate the visible copy — the toast describes
     * THIS one combo the user just pressed.
     */
    import { dismissToast } from '$lib/ui/toast'
    import Button from '$lib/ui/Button.svelte'
    import { setGlobalGoToLatestShortcut } from '$lib/tauri-commands'
    import { setGlobalGoToLatestEnabled } from './global-shortcut-setting'
    import { getAppLogger } from '$lib/logging/logger'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        toastId: string
        binding: string
    }

    const { toastId, binding }: Props = $props()
    const log = getAppLogger('downloads')

    function handleKeepOn(): void {
        // `acknowledged` was already flipped to true by the bridge before
        // this toast appeared. Just dismiss.
        dismissToast(toastId)
    }

    async function handleTurnOff(): Promise<void> {
        setGlobalGoToLatestEnabled(false)
        try {
            // Tell the backend to unregister live so the next ⌃⌥⌘J in Chrome
            // doesn't still fire. The Settings UI store change also drives
            // this, but the IPC ack here is what the test asserts on.
            const result = await setGlobalGoToLatestShortcut(false, binding)
            if (result.status === 'error') {
                log.warn('setGlobalGoToLatestShortcut(false, ...) returned an error: {error}', {
                    error: JSON.stringify(result.error),
                })
            }
        } catch (err) {
            log.warn('Failed to call setGlobalGoToLatestShortcut: {err}', { err: String(err) })
        }
        dismissToast(toastId)
    }
</script>

<div class="content">
    <span class="message">{tString('downloads.warnToast.message', { binding })}</span>
    <div class="actions">
        <Button size="mini" variant="secondary" onclick={handleTurnOff}>{tString('downloads.warnToast.turnOff')}</Button>
        <Button size="mini" variant="primary" onclick={handleKeepOn}>{tString('downloads.warnToast.keepOn')}</Button>
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
        line-height: 1.4;
    }

    .actions {
        display: flex;
        justify-content: flex-end;
        gap: var(--spacing-sm);
        margin-top: var(--spacing-md);
    }
</style>
