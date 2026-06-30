<script lang="ts">
    /**
     * One-time "your drive's index may be stale" dialog (D2). Mounted once
     * app-wide. Subscribes to the `index-freshness-changed` event (emitted only
     * on a real value change), and the FIRST time any external drive flips to
     * Stale — gated on the `indexing.staleNotify` setting and a persisted
     * one-shot flag — shows this explainer so the user learns the concept. The
     * yellow badge keeps showing regardless of this dialog.
     */
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import { onDestroy, onMount } from 'svelte'
    import type { UnlistenFn } from '@tauri-apps/api/event'
    import { onIndexFreshnessChanged } from '$lib/tauri-commands/indexing'
    import { getSetting, setSetting } from '$lib/settings'
    import { getVolumes } from '$lib/stores/volume-store.svelte'
    import { t, tString } from '$lib/intl/messages.svelte'
    import { hasShownFirstStaleDialog, markFirstStaleDialogShown } from './drive-index-prefs'

    let open = $state(false)
    let staleVolumeName = $state('')
    let unlisten: UnlistenFn | undefined

    function volumeName(volumeId: string): string {
        // `root` is the local disk, which is journaled and never goes stale — but
        // fall back to the id for any volume not currently in the store.
        return getVolumes().find((v) => v.id === volumeId)?.name ?? volumeId
    }

    onMount(() => {
        void onIndexFreshnessChanged((payload) => {
            // Only the exact Fresh→Stale edge for an EXTERNAL drive matters (the
            // event fires only on a change). Local `root` is journaled and never
            // stale, so it can't trip this even if a future path emitted it.
            if (payload.freshness !== 'stale' || payload.volumeId === 'root') return
            if (!getSetting('indexing.staleNotify')) return
            if (hasShownFirstStaleDialog()) return

            markFirstStaleDialogShown()
            staleVolumeName = volumeName(payload.volumeId)
            open = true
        }).then((u) => {
            unlisten = u
        })
    })

    onDestroy(() => {
        unlisten?.()
    })

    function close() {
        open = false
    }

    function neverShowAgain() {
        setSetting('indexing.staleNotify', false)
        open = false
    }
</script>

{#if open}
    <ModalDialog
        titleId="drive-index-stale-dialog-title"
        dialogId="drive-index-stale"
        role="dialog"
        onclose={close}
        ariaDescribedby="drive-index-stale-body"
        containerStyle="width: 440px"
    >
        {#snippet title()}{tString('indexing.staleDialog.title')}{/snippet}

        <div class="body">
            <p id="drive-index-stale-body" class="description">
                {t('indexing.staleDialog.body', { name: staleVolumeName })}
            </p>
        </div>

        {#snippet footer()}
            <Button variant="secondary" onclick={neverShowAgain}>
                {tString('indexing.staleDialog.neverShowAgain')}
            </Button>
            <Button variant="primary" autoFocus onclick={close}>
                {tString('indexing.staleDialog.close')}
            </Button>
        {/snippet}
    </ModalDialog>
{/if}

<style>
    .body {
        padding: 0 var(--spacing-xl);
    }

    .description {
        margin: 0 0 var(--spacing-md);
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        line-height: 1.5;
    }
</style>
