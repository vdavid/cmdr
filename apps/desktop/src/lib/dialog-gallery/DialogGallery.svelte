<script lang="ts">
    /**
     * Dev-only harness that renders whichever gallery dialog the Debug window asked
     * for, in the MAIN window.
     *
     * Main window, not Debug: these dialogs are designed against the two-pane
     * backdrop, `ModalDialog` reports every mount to the Rust `SoftDialogTracker`
     * (so a Debug-window copy would lie to MCP), and the Debug window's minimal
     * capability set would make window- and URL-opening buttons fail silently.
     *
     * Mounted from `routes/(main)/+layout.svelte` inside `{#if import.meta.env.DEV}`.
     * Details: [DETAILS.md](DETAILS.md).
     */
    import AlertDialog from '$lib/ui/AlertDialog.svelte'
    import { getAppLogger } from '$lib/logging/logger'
    import { closeGalleryDialog, getOpenGalleryDialog } from './gallery-state.svelte'
    import { alertFixtures, type AlertFixture } from './fixtures/alert'

    const log = getAppLogger('dialogGallery')

    /** What to render for the current request. `null` means nothing is previewed. */
    type RenderPlan = { kind: 'alert'; props: AlertFixture } | null

    const plan = $derived.by((): RenderPlan => {
        const open = getOpenGalleryDialog()
        if (!open) return null

        switch (open.dialogId) {
            case 'alert': {
                const fixture = alertFixtures[open.stateId]
                if (fixture) return { kind: 'alert', props: fixture }
                break
            }
            default:
                break
        }

        // Rendering nothing beats rendering a half-filled dialog: a design review
        // has to be able to trust that what's on screen is what the fixture says.
        log.warn('Dialog gallery has no fixture for {dialogId} / {stateId}', {
            dialogId: open.dialogId,
            stateId: open.stateId,
        })
        return null
    })
</script>

{#if plan?.kind === 'alert'}
    <AlertDialog {...plan.props} onClose={closeGalleryDialog} />
{/if}
