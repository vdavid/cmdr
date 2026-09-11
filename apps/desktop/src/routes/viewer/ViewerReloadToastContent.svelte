<script lang="ts">
    import { viewerReload } from '$lib/tauri-commands'
    import { dismissToast } from '$lib/ui/toast/toast-store.svelte'
    import { getAppLogger } from '$lib/logging/logger'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        toastId: string
        /** The viewer session whose file changed on disk. */
        sessionId: string
        /** `grew`: bytes were appended. `rotated`: the file was replaced. */
        kind: 'grew' | 'rotated'
    }

    const { toastId, sessionId, kind }: Props = $props()

    const log = getAppLogger('viewer-tail')

    const message = $derived(
        kind === 'rotated' ? tString('viewer.reloadToast.rotated') : tString('viewer.reloadToast.grew'),
    )

    async function reload(): Promise<void> {
        try {
            const res = await viewerReload(sessionId)
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
            {tString('viewer.reloadToast.reload')}
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
