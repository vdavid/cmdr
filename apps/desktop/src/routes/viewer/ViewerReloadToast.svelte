<script lang="ts" module>
    /**
     * Module-state bridge for the persistent reload toast. The toast system
     * mounts components without props, so the viewer page calls
     * `setReloadToastContext({ sessionId, kind, toastId })` immediately
     * before `addToast(ViewerReloadToast, ...)` and the toast renders
     * against this state. There's at most one reload toast per session per
     * kind active at a time; rapid changes coalesce by toast id, so the
     * "last write wins" semantic is fine.
     */
    interface ReloadToastContext {
        sessionId: string
        toastId: string
        kind: 'grew' | 'rotated'
    }

    let ctx = $state<ReloadToastContext>({ sessionId: '', toastId: '', kind: 'grew' })

    export function setReloadToastContext(next: ReloadToastContext): void {
        ctx = next
    }
</script>

<script lang="ts">
    import { commands } from '$lib/ipc/bindings'
    import { dismissToast } from '$lib/ui/toast/toast-store.svelte'
    import { getAppLogger } from '$lib/logging/logger'
    import { tString } from '$lib/intl/messages.svelte'

    const log = getAppLogger('viewer-tail')

    const message = $derived(
        ctx.kind === 'rotated'
            ? tString('viewer.reloadToast.rotated')
            : tString('viewer.reloadToast.grew'),
    )

    async function reload(): Promise<void> {
        const session = ctx.sessionId
        const toastId = ctx.toastId
        try {
            const res = await commands.viewerReload(session)
            if (res.status === 'error') {
                log.warn('viewer_reload failed: {error}', { error: res.error })
            }
        } catch (e) {
            log.warn('viewer_reload threw: {error}', { error: String(e) })
        } finally {
            dismissToast(toastId)
        }
    }
</script>

<div class="viewer-reload-toast">
    <span class="viewer-reload-message">{message}</span>
    <button
        type="button"
        class="viewer-reload-button"
        onclick={() => {
            void reload()
        }}
    >
        {tString('viewer.reloadToast.reload')}
    </button>
</div>

<style>
    .viewer-reload-toast {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
    }

    .viewer-reload-message {
        flex: 1;
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
        line-height: 1.4;
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
