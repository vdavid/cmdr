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
     * Most dialogs here are PROP-DRIVEN: they take everything they render as props,
     * so the harness passes a fixture and the shipping component does the rest.
     * Nothing below branches inside a dialog.
     *
     * The store-seeded ones are the exception, and they render NOTHING here: they
     * take no content props, so the harness patches the real module store and the
     * APP's own mount site renders them. The harness still owns the undo — the
     * seed's restore closure is an `$effect` cleanup below — and watches for the
     * dialog closing itself, since it closes through its own store rather than
     * through `closeGalleryDialog`.
     *
     * Five of them (delete, transfer, mkdir, mkfile, go-to-path) do real work on
     * mount, so their props are BUILT from the real fixture directory the request
     * carries (`disk-fixture.ts`) rather than written by hand: the scan tallies,
     * conflict warnings, and space figures are then the real ones.
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
    import DeleteDialog from '$lib/file-operations/delete/DeleteDialog.svelte'
    import TransferDialog from '$lib/file-operations/transfer/TransferDialog.svelte'
    import NewFolderDialog from '$lib/file-operations/mkdir/NewFolderDialog.svelte'
    import NewFileDialog from '$lib/file-operations/mkfile/NewFileDialog.svelte'
    import GoToPathDialog from '$lib/go-to-path/GoToPathDialog.svelte'
    import { MtpPermissionDialog, PtpcameradDialog } from '$lib/mtp'
    import DeleteAiModelDialog from '$lib/settings/sections/DeleteAiModelDialog.svelte'
    // The viewer copy dialogs live in the viewer window's route. Same relative-import
    // shape `lib/search/ImageSearchResults.svelte` uses for `routes/viewer/media-view`.
    import ViewerCopyDialogs from '../../routes/viewer/ViewerCopyDialogs.svelte'
    import { getAppLogger } from '$lib/logging/logger'
    import { untrack } from 'svelte'
    import { closeGalleryDialog, getOpenGalleryDialog, type GalleryDiskFixture } from './gallery-state.svelte'
    import { fixtureRecords } from './fixtures'
    import { buildStoreSeed, type StoreSeededDialogId } from './fixtures/store-seeded'
    import type { StoreSeed } from './store-seeding'
    import type { AlertFixture } from './fixtures/alert'
    import type { DeleteAiModelFixture } from './fixtures/ai-model'
    import type { DeleteFixture, GoToPathFixture, NewEntryFixture, TransferFixture } from './fixtures/disk'
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
        | { kind: 'delete'; props: DeleteFixture }
        | { kind: 'transfer'; props: TransferFixture }
        | { kind: 'mkdir'; props: NewEntryFixture }
        | { kind: 'mkfile'; props: NewEntryFixture }
        | { kind: 'go-to-path'; props: GoToPathFixture }
        | { kind: 'delete-ai-model'; props: DeleteAiModelFixture }
        /** Renders nothing: the app's own mount site shows the seeded dialog. */
        | { kind: 'store-seeded'; seed: StoreSeed }
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
     * Same, for the disk-backed dialogs: their fixture is a builder that needs the
     * resolved fixture directory. No directory (the pane couldn't get there) means
     * no dialog, for the same reason a missing fixture does.
     */
    function withDiskFixture<T>(
        build: ((disk: GalleryDiskFixture) => T) | undefined,
        disk: GalleryDiskFixture | undefined,
        toPlan: (props: T) => RenderPlan,
    ): RenderPlan {
        return build === undefined || disk === undefined ? null : toPlan(build(disk))
    }

    /**
     * Same, for the store-seeded dialogs: the fixture is a patch for a real app
     * store, so the "plan" is the seed rather than a prop bag. A state id with no
     * patch seeds nothing, for the same reason a missing fixture renders nothing.
     */
    function withStoreSeed(dialogId: StoreSeededDialogId, stateId: string): RenderPlan {
        const seed = buildStoreSeed(dialogId, stateId)
        return seed === null ? null : { kind: 'store-seeded', seed }
    }

    /** The "Go to path" preview has no pane jump behind it, so closing IS the outcome. */
    function closeGoToPathPreview(): Promise<undefined> {
        closeGalleryDialog()
        return Promise.resolve(undefined)
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
    const planResolvers: Partial<
        Record<SoftDialogId, (stateId: string, disk: GalleryDiskFixture | undefined) => RenderPlan>
    > = {
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
        'delete-ai-model': (id) =>
            withFixture(fixtureRecords['delete-ai-model'][id], (f) => ({ kind: 'delete-ai-model', props: f })),

        'bulk-rename-review': (id) => withStoreSeed('bulk-rename-review', id),
        'error-report': (id) => withStoreSeed('error-report', id),
        feedback: (id) => withStoreSeed('feedback', id),
        'operation-log': (id) => withStoreSeed('operation-log', id),
        'whats-new': (id) => withStoreSeed('whats-new', id),

        'delete-confirmation': (id, disk) =>
            withDiskFixture(fixtureRecords['delete-confirmation'][id], disk, (props) => ({ kind: 'delete', props })),
        'transfer-confirmation': (id, disk) =>
            withDiskFixture(fixtureRecords['transfer-confirmation'][id], disk, (props) => ({
                kind: 'transfer',
                props,
            })),
        'mkdir-confirmation': (id, disk) =>
            withDiskFixture(fixtureRecords['mkdir-confirmation'][id], disk, (props) => ({ kind: 'mkdir', props })),
        'new-file-confirmation': (id, disk) =>
            withDiskFixture(fixtureRecords['new-file-confirmation'][id], disk, (props) => ({ kind: 'mkfile', props })),
        'go-to-path': (id, disk) =>
            withDiskFixture(fixtureRecords['go-to-path'][id], disk, (props) => ({ kind: 'go-to-path', props })),

        about: () => ({ kind: 'about' }),
        'commercial-reminder': () => ({ kind: 'commercial-reminder' }),
        license: () => ({ kind: 'license' }),
        'connect-to-server': () => ({ kind: 'connect-to-server' }),
        'mtp-permission': () => ({ kind: 'mtp-permission' }),
    }

    const plan = $derived.by((): RenderPlan => {
        const open = getOpenGalleryDialog()
        if (!open) return null

        const resolved = planResolvers[open.dialogId]?.(open.stateId, open.disk) ?? null
        if (resolved) return resolved

        log.warn('Dialog gallery has no fixture for {dialogId} / {stateId}', {
            dialogId: open.dialogId,
            stateId: open.stateId,
        })
        return null
    })

    /**
     * Restoring a seeded store is STRUCTURAL: the undo is this effect's cleanup,
     * so closing the dialog, swapping to another preview, and unmounting the
     * harness all put the store back. No fixture has to remember, and no preview
     * can leave the app half-seeded.
     *
     * `untrack` matters: `apply()` reads the fields it's about to overwrite, and
     * a tracked read would make the effect depend on its own writes (restore,
     * re-seed, forever).
     */
    $effect(() => {
        if (plan?.kind !== 'store-seeded') return
        const seed = plan.seed
        return untrack(() => seed.apply())
    })

    /**
     * A store-seeded dialog closes through ITS OWN store (Escape, its Cancel
     * button), never through `closeGalleryDialog`, so the gallery would otherwise
     * still believe a preview is up and `+page.svelte` would keep suppressing
     * every global shortcut. `seeded` guards the first pass, where the store is
     * still closed until the effect above runs.
     */
    let seeded = false
    $effect(() => {
        if (plan?.kind !== 'store-seeded') {
            seeded = false
            return
        }
        // Read first, unconditionally: this is the subscription that brings us
        // back when the dialog closes itself.
        const isOpen = plan.seed.isOpen()
        if (isOpen) seeded = true
        else if (seeded) closeGalleryDialog()
    })
</script>

<!--
  Keyed on the plan, so every trigger REMOUNTS rather than re-rendering with new
  props. The disk-backed dialogs start their work in `onMount` (the delete and
  transfer scans), so a reused component would keep the previous state's tally on
  screen next to the new state's file list — a number that's real, but not of what
  you're looking at. Production keys these two dialogs for the same reason
  (`DialogManager.svelte`).
-->
{#key plan}
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
    {:else if plan?.kind === 'delete'}
        <DeleteDialog {...plan.props} onConfirm={closeGalleryDialog} onCancel={closeGalleryDialog} />
    {:else if plan?.kind === 'transfer'}
        <TransferDialog {...plan.props} onConfirm={closeGalleryDialog} onCancel={closeGalleryDialog} />
    {:else if plan?.kind === 'mkdir'}
        <NewFolderDialog {...plan.props} onCreated={closeGalleryDialog} onCancel={closeGalleryDialog} />
    {:else if plan?.kind === 'mkfile'}
        <NewFileDialog {...plan.props} onCreated={closeGalleryDialog} onCancel={closeGalleryDialog} />
    {:else if plan?.kind === 'go-to-path'}
        <GoToPathDialog {...plan.props} onGo={closeGoToPathPreview} onCancel={closeGalleryDialog} />
    {:else if plan?.kind === 'delete-ai-model'}
        <DeleteAiModelDialog {...plan.props} onConfirm={closeGalleryDialog} onCancel={closeGalleryDialog} />
    {/if}
{/key}
