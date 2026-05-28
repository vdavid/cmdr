<script lang="ts">
    /**
     * First-trigger warn toast for the global reveal hotkey (default ⌃⌥⌘J).
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
    import { commands } from '$lib/ipc/bindings'
    import { setGlobalRevealEnabled } from './global-shortcut-setting'
    import { getAppLogger } from '$lib/logging/logger'

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
        setGlobalRevealEnabled(false)
        try {
            // Tell the backend to unregister live so the next ⌃⌥⌘J in Chrome
            // doesn't still fire. The Settings UI store change also drives
            // this, but the IPC ack here is what the test asserts on.
            const result = await commands.setGlobalRevealShortcut(false, binding)
            if (result.status === 'error') {
                log.warn('setGlobalRevealShortcut(false, ...) returned an error: {error}', {
                    error: JSON.stringify(result.error),
                })
            }
        } catch (err) {
            log.warn('Failed to call setGlobalRevealShortcut: {err}', { err: String(err) })
        }
        dismissToast(toastId)
    }
</script>

<div class="content">
    <span class="message">The {binding} shortcut jumps to your latest download from anywhere. Keep it on?</span>
    <div class="actions">
        <button class="primary" type="button" onclick={handleKeepOn}>Keep it on</button>
        <button class="link" type="button" onclick={handleTurnOff}>Turn it off</button>
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
        gap: var(--spacing-md);
        align-items: center;
    }

    .primary {
        background: var(--color-accent);
        color: var(--color-accent-fg);
        border: 1px solid transparent;
        border-radius: var(--radius-sm);
        padding: var(--spacing-xxs) var(--spacing-sm);
        font-size: var(--font-size-xs);
    }

    .primary:hover {
        background: var(--color-accent-hover);
    }

    .link {
        background: none;
        border: none;
        padding: 0;
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .link:hover {
        color: var(--color-text-secondary);
    }
</style>
