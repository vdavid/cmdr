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
     * Every dialog here is PROP-DRIVEN: it takes everything it renders as props, so
     * the harness passes a fixture and the shipping component does the rest. Nothing
     * below branches inside a dialog. Store-seeded and event-seeded dialogs work
     * differently and don't belong in this table.
     *
     * Every fixture callback closes the preview. The gallery deliberately performs
     * no action: `onResolve`, `onCommit`, `onSaveAs` and friends have nothing real
     * behind them here, and pretending otherwise would be the lie this instrument
     * exists to avoid.
     *
     * Mounted from `routes/(main)/+layout.svelte` inside `{#if import.meta.env.DEV}`.
     * Details: [DETAILS.md](DETAILS.md).
     */
    import type { SoftDialogId } from '$lib/ui/dialog-registry'
    import type { CrashReport } from '$lib/tauri-commands'
    import AlertDialog from '$lib/ui/AlertDialog.svelte'
    import AboutWindow from '$lib/licensing/AboutWindow.svelte'
    import CommercialReminderModal from '$lib/licensing/CommercialReminderModal.svelte'
    import ExpirationModal from '$lib/licensing/ExpirationModal.svelte'
    import LicenseKeyDialog from '$lib/licensing/LicenseKeyDialog.svelte'
    import ExtensionChangeDialog from '$lib/file-explorer/rename/ExtensionChangeDialog.svelte'
    import RenameConflictDialog from '$lib/file-explorer/rename/RenameConflictDialog.svelte'
    import ArchivePasswordDialog from '$lib/file-operations/transfer/ArchivePasswordDialog.svelte'
    import TransferErrorDialog from '$lib/file-operations/transfer/TransferErrorDialog.svelte'
    import ConnectToServerDialog from '$lib/file-explorer/network/ConnectToServerDialog.svelte'
    import CrashReportDialog from '$lib/crash-reporter/CrashReportDialog.svelte'
    import SelectionDialog from '$lib/selection-dialog/SelectionDialog.svelte'
    import { MtpPermissionDialog, PtpcameradDialog } from '$lib/mtp'
    // The viewer copy dialogs live in the viewer window's route. Same relative-import
    // shape `lib/search/ImageSearchResults.svelte` uses for `routes/viewer/media-view`.
    import ViewerCopyDialogs from '../../routes/viewer/ViewerCopyDialogs.svelte'
    import { getAppLogger } from '$lib/logging/logger'
    import { closeGalleryDialog, getOpenGalleryDialog } from './gallery-state.svelte'
    import { fixtureRecords } from './fixtures'
    import type { AlertFixture } from './fixtures/alert'
    import type { ExpirationFixture } from './fixtures/licensing'
    import type { ExtensionChangeFixture, RenameConflictFixture } from './fixtures/rename'
    import type { ArchivePasswordFixture } from './fixtures/archive-password'
    import type { TransferErrorFixture } from './fixtures/transfer-error'
    import type { PtpcameradFixture } from './fixtures/devices'
    import type { SelectionFixture } from './fixtures/selection'

    const log = getAppLogger('dialogGallery')

    /** What to render for the current request. `null` means nothing is previewed. */
    type RenderPlan =
        | { kind: 'alert'; props: AlertFixture }
        | { kind: 'about' }
        | { kind: 'commercial-reminder' }
        | { kind: 'expiration'; props: ExpirationFixture }
        | { kind: 'license' }
        | { kind: 'extension-change'; props: ExtensionChangeFixture }
        | { kind: 'rename-conflict'; props: RenameConflictFixture }
        | { kind: 'archive-password'; props: ArchivePasswordFixture }
        | { kind: 'transfer-error'; props: TransferErrorFixture }
        | { kind: 'connect-to-server' }
        | { kind: 'mtp-permission' }
        | { kind: 'ptpcamerad'; props: PtpcameradFixture }
        | { kind: 'crash-report'; props: { report: CrashReport } }
        | { kind: 'viewer-copy'; props: { confirmBytes: number | null; refuseBytes: number | null } }
        | { kind: 'selection'; props: SelectionFixture & { mode: 'add' | 'remove' } }
        | null

    /**
     * Wraps a looked-up fixture in a plan, or reports the miss as `null`. A state id
     * that doesn't resolve renders NOTHING: a design review has to be able to trust
     * that what's on screen is what the fixture says, so a half-filled dialog is the
     * one outcome the harness must never produce.
     */
    function withFixture<T>(fixture: T | undefined, build: (fixture: T) => RenderPlan): RenderPlan {
        return fixture === undefined ? null : build(fixture)
    }

    /**
     * How each dialog turns a request into a plan, one entry per dialog id. A table
     * rather than a `switch` because the switch grew past the complexity ceiling at
     * this many dialogs; each entry still owns its own typed fixture lookup, so
     * nothing here is cast.
     *
     * The last five take callbacks only: there's no fixture to miss, and the single
     * state each exposes is what its gallery row discloses.
     */
    const planResolvers: Partial<Record<SoftDialogId, (stateId: string) => RenderPlan>> = {
        alert: (id) => withFixture(fixtureRecords.alert[id], (f) => ({ kind: 'alert', props: f })),
        expiration: (id) => withFixture(fixtureRecords.expiration[id], (f) => ({ kind: 'expiration', props: f })),
        'extension-change': (id) =>
            withFixture(fixtureRecords['extension-change'][id], (f) => ({ kind: 'extension-change', props: f })),
        'rename-conflict': (id) =>
            withFixture(fixtureRecords['rename-conflict'][id], (f) => ({ kind: 'rename-conflict', props: f })),
        'archive-password': (id) =>
            withFixture(fixtureRecords['archive-password'][id], (f) => ({ kind: 'archive-password', props: f })),
        'transfer-error': (id) =>
            withFixture(fixtureRecords['transfer-error'][id], (f) => ({ kind: 'transfer-error', props: f })),
        ptpcamerad: (id) => withFixture(fixtureRecords.ptpcamerad[id], (f) => ({ kind: 'ptpcamerad', props: f })),
        'crash-report': (id) =>
            withFixture(fixtureRecords['crash-report'][id], (report) => ({ kind: 'crash-report', props: { report } })),
        'viewer-copy-confirm': (id) =>
            withFixture(fixtureRecords['viewer-copy-confirm'][id], (f) => ({
                kind: 'viewer-copy',
                props: { confirmBytes: f.bytes, refuseBytes: null },
            })),
        'viewer-copy-refuse': (id) =>
            withFixture(fixtureRecords['viewer-copy-refuse'][id], (f) => ({
                kind: 'viewer-copy',
                props: { confirmBytes: null, refuseBytes: f.bytes },
            })),
        'selection-add': (id) =>
            withFixture(fixtureRecords['selection-add'][id], (f) => ({
                kind: 'selection',
                props: { ...f, mode: 'add' },
            })),
        'selection-remove': (id) =>
            withFixture(fixtureRecords['selection-remove'][id], (f) => ({
                kind: 'selection',
                props: { ...f, mode: 'remove' },
            })),

        about: () => ({ kind: 'about' }),
        'commercial-reminder': () => ({ kind: 'commercial-reminder' }),
        license: () => ({ kind: 'license' }),
        'connect-to-server': () => ({ kind: 'connect-to-server' }),
        'mtp-permission': () => ({ kind: 'mtp-permission' }),
    }

    const plan = $derived.by((): RenderPlan => {
        const open = getOpenGalleryDialog()
        if (!open) return null

        const resolved = planResolvers[open.dialogId]?.(open.stateId) ?? null
        if (resolved) return resolved

        log.warn('Dialog gallery has no fixture for {dialogId} / {stateId}', {
            dialogId: open.dialogId,
            stateId: open.stateId,
        })
        return null
    })
</script>

{#if plan?.kind === 'alert'}
    <AlertDialog {...plan.props} onClose={closeGalleryDialog} />
{:else if plan?.kind === 'about'}
    <AboutWindow onClose={closeGalleryDialog} />
{:else if plan?.kind === 'commercial-reminder'}
    <CommercialReminderModal onClose={closeGalleryDialog} />
{:else if plan?.kind === 'expiration'}
    <ExpirationModal {...plan.props} onClose={closeGalleryDialog} />
{:else if plan?.kind === 'license'}
    <LicenseKeyDialog onClose={closeGalleryDialog} onSuccess={closeGalleryDialog} />
{:else if plan?.kind === 'extension-change'}
    <ExtensionChangeDialog {...plan.props} onKeepOld={closeGalleryDialog} onUseNew={closeGalleryDialog} />
{:else if plan?.kind === 'rename-conflict'}
    <RenameConflictDialog {...plan.props} onResolve={closeGalleryDialog} />
{:else if plan?.kind === 'archive-password'}
    <ArchivePasswordDialog {...plan.props} onSubmit={closeGalleryDialog} onCancel={closeGalleryDialog} />
{:else if plan?.kind === 'transfer-error'}
    <TransferErrorDialog {...plan.props} onClose={closeGalleryDialog} onRetry={closeGalleryDialog} />
{:else if plan?.kind === 'connect-to-server'}
    <ConnectToServerDialog onConnect={closeGalleryDialog} onClose={closeGalleryDialog} />
{:else if plan?.kind === 'mtp-permission'}
    <MtpPermissionDialog onClose={closeGalleryDialog} onRetry={closeGalleryDialog} />
{:else if plan?.kind === 'ptpcamerad'}
    <PtpcameradDialog {...plan.props} onClose={closeGalleryDialog} onRetry={closeGalleryDialog} />
{:else if plan?.kind === 'crash-report'}
    <CrashReportDialog {...plan.props} onClose={closeGalleryDialog} />
{:else if plan?.kind === 'viewer-copy'}
    <ViewerCopyDialogs
        {...plan.props}
        onCancelConfirm={closeGalleryDialog}
        onProceedConfirm={closeGalleryDialog}
        onDismissRefuse={closeGalleryDialog}
        onSaveAs={closeGalleryDialog}
    />
{:else if plan?.kind === 'selection'}
    <SelectionDialog {...plan.props} onCommit={closeGalleryDialog} onClose={closeGalleryDialog} />
{/if}
