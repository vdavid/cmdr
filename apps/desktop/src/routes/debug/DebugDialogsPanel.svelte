<script lang="ts">
    /**
     * Debug > Soft dialogs: the inventory of every registered soft dialog, with a
     * button per reviewable state.
     *
     * The dialogs themselves open in the MAIN window (see
     * `$lib/dialog-gallery/DETAILS.md`); this panel only emits the trigger. Same
     * `emitTo('main', …)` transport `DebugErrorPreviewPanel` uses.
     */
    import {
        DIALOG_GALLERY_ENTRIES,
        UNREGISTERED_OVERLAY_ENTRIES,
        type GalleryHostWindow,
    } from '$lib/dialog-gallery/gallery-registry'
    import type { SoftDialogId } from '$lib/ui/dialog-registry'

    const hostWindowLabels: Record<GalleryHostWindow, string> = {
        main: 'Main window',
        settings: 'Settings window',
        viewer: 'Viewer window',
    }

    const readyCount = $derived(DIALOG_GALLERY_ENTRIES.filter((e) => e.status === 'ready').length)

    async function openDialog(dialogId: SoftDialogId, stateId: string) {
        try {
            const { emitTo } = await import('@tauri-apps/api/event')
            await emitTo('main', 'debug-open-gallery-dialog', { dialogId, stateId })
        } catch (error) {
            // eslint-disable-next-line no-console -- Debug window is dev-only
            console.error('Opening the gallery dialog failed:', error)
        }
    }
</script>

<section class="debug-section">
    <h2>Soft dialogs</h2>
    <p class="dialogs-intro">
        Every dialog in <code>SOFT_DIALOG_REGISTRY</code>, opened on demand with fixture data.
        {readyCount} of {DIALOG_GALLERY_ENTRIES.length} are wired up so far. They all render over the
        <strong>main window</strong>, which is where they're designed to sit; rows tagged
        <em>Settings window</em> or <em>Viewer window</em> live somewhere else in the shipping app, so
        judge those on the dialog, not the backdrop.
    </p>

    <div class="dialogs-panel">
        {#each DIALOG_GALLERY_ENTRIES as entry (entry.dialogId)}
            <div class="dialog-row" class:blocked={entry.status !== 'ready'}>
                <div class="dialog-heading">
                    <span class="dialog-label">{entry.label}</span>
                    <code class="dialog-id">{entry.dialogId}</code>
                    <span class="dialog-host" class:foreign={entry.hostWindow !== 'main'}>
                        {hostWindowLabels[entry.hostWindow]}
                    </span>
                </div>
                {#if entry.note}
                    <p class="dialog-note">{entry.note}</p>
                {/if}
                {#if entry.status === 'ready'}
                    <div class="dialog-states">
                        {#each entry.states as state (state.id)}
                            <div class="dialog-state">
                                <button
                                    class="index-button"
                                    onclick={() => void openDialog(entry.dialogId, state.id)}
                                >
                                    {state.label}
                                </button>
                                {#if state.note}
                                    <span class="dialog-state-note">{state.note}</span>
                                {/if}
                            </div>
                        {/each}
                    </div>
                {:else}
                    <p class="dialog-reason">{entry.reason}</p>
                {/if}
            </div>
        {/each}
    </div>
</section>

<section class="debug-section">
    <h2>Not in the soft dialog registry</h2>
    <p class="dialogs-intro">
        Overlays that look modal but aren't registered soft dialogs, listed so the inventory above
        can't imply nothing else exists. Evoke these by hand.
    </p>

    <div class="dialogs-panel">
        {#each UNREGISTERED_OVERLAY_ENTRIES as overlay (overlay.overlayId)}
            <div class="dialog-row blocked">
                <div class="dialog-heading">
                    <span class="dialog-label">{overlay.label}</span>
                    <code class="dialog-id">{overlay.overlayId}</code>
                    <span class="dialog-host" class:foreign={overlay.hostWindow !== 'main'}>
                        {hostWindowLabels[overlay.hostWindow]}
                    </span>
                </div>
                <p class="dialog-reason">{overlay.reason}</p>
            </div>
        {/each}
    </div>
</section>

<style>
    /* stylelint-disable declaration-property-value-disallowed-list -- Dev utility window */

    .dialogs-intro {
        margin: 0 0 var(--spacing-md);
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        line-height: 1.5;
    }

    .dialogs-panel {
        background: var(--color-bg-secondary);
        border-radius: var(--radius-md);
        padding: var(--spacing-md);
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
    }

    .dialog-row {
        display: flex;
        flex-direction: column;
        gap: 4px;
        padding: var(--spacing-sm) 0;
        border-bottom: 1px solid var(--color-border);
    }

    .dialog-row:last-child {
        border-bottom: none;
    }

    .dialog-heading {
        display: flex;
        align-items: baseline;
        flex-wrap: wrap;
        gap: var(--spacing-sm);
    }

    .dialog-label {
        font-size: var(--font-size-md);
        color: var(--color-text-primary);
    }

    .blocked .dialog-label {
        color: var(--color-text-secondary);
    }

    .dialog-id {
        font-family: var(--font-mono);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .dialog-host {
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    /* The dialog doesn't live in the main window in the shipping app; the gallery
       shows it there anyway, so the mismatch has to be visible, not implied. */
    .dialog-host.foreign {
        padding: 1px var(--spacing-sm);
        border-radius: var(--radius-sm);
        background: var(--color-bg-tertiary);
        color: var(--color-text-secondary);
        font-weight: 600;
    }

    .dialog-states {
        display: flex;
        flex-wrap: wrap;
        gap: var(--spacing-sm);
        margin-top: 2px;
    }

    .dialog-state {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
    }

    .dialog-state-note,
    .dialog-note,
    .dialog-reason {
        margin: 0;
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        line-height: 1.5;
    }
</style>
