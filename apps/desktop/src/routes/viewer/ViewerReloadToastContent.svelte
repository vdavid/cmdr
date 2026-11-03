<script lang="ts">
    import { viewerReload } from '#lib/tauri-commands/index.js'
    import { dismissToast } from '#lib/ui/toast/toast-store.svelte.js'
    import { getAppLogger } from '#lib/logging/logger.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import type { MessageKey } from '#lib/intl/keys.gen.js'
    import { logSessionOpFailure, sessionOpFailureKey } from './viewer-session-op-failure'

    interface Props {
        toastId: string
        /** The viewer session whose file changed on disk. */
        sessionId: string
        /** `grew`: bytes were appended. `rotated`: the file was replaced. */
        kind: 'grew' | 'rotated'
        /** Why the last reload didn't go through, or `null` before one didn't. Swaps the text and the button. */
        failureKey: MessageKey | null
        /** Runs after a reload went through, so the page drops its stale rows and refetches. */
        onReloaded: () => void
        /**
         * Runs when a reload didn't go through. The owner re-raises this toast with `failureKey`, so it stays up,
         * turns into a warning, and the screen reader hears the new text (a re-raise is spoken; an in-place edit isn't).
         */
        onReloadRefused: (failureKey: MessageKey) => void
    }

    const { toastId, sessionId, kind, failureKey, onReloaded, onReloadRefused }: Props = $props()

    const log = getAppLogger('viewer-tail')

    let reloading = false

    const message = $derived(
        failureKey !== null
            ? tString(failureKey)
            : kind === 'rotated'
              ? tString('viewer.reloadToast.rotated')
              : tString('viewer.reloadToast.grew'),
    )

    async function reload(): Promise<void> {
        if (reloading) return
        reloading = true
        try {
            const res = await viewerReload(sessionId)
            if (res.status === 'ok') {
                onReloaded()
                dismissToast(toastId)
                return
            }
            logSessionOpFailure(log, 'reload', res.error)
            onReloadRefused(sessionOpFailureKey('reload', res.error))
        } catch (e) {
            log.error('viewer_reload threw: {error}', { error: e })
            onReloadRefused(sessionOpFailureKey('reload', null))
        } finally {
            reloading = false
        }
    }
</script>

<div>
    <span>{message}</span>
    <div class="viewer-reload-actions">
        <button
            type="button"
            class="viewer-reload-button"
            onclick={() => {
                void reload()
            }}
        >
            {failureKey !== null ? tString('viewer.reloadToast.retry') : tString('viewer.reloadToast.reload')}
        </button>
    </div>
</div>

<style>
    .viewer-reload-actions {
        display: flex;
        justify-content: flex-end;
        margin-top: var(--spacing-md);
    }

    .viewer-reload-button {
        background: var(--color-bg-tertiary);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-sm);
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        font-weight: 500;
        /* stylelint-disable-next-line declaration-property-value-disallowed-list -- mini button height target */
        padding: 2px 10px;
        line-height: var(--font-line-height-normal);
        transition: all var(--transition-base);
    }

    .viewer-reload-button:hover {
        background: var(--color-bg-secondary);
    }

    .viewer-reload-button:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 1px;
    }
</style>
