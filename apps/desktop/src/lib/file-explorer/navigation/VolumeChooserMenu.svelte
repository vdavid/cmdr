<script lang="ts">
    /**
     * The volume switcher's list: every place the pane can go, grouped, on the house `Menu`
     * primitive (`$lib/ui/Menu.svelte`). The chip that opens it is `VolumeBreadcrumb.svelte`.
     *
     * ❗ The primitive owns every interaction: open and close, anchoring, the cursor, the
     * keyboard (including ⌥↑/⌥↓ reorder and the submenu), keyboard-vs-pointer mode, drag,
     * focus, and placement. This component owns only the DATA (sections built from
     * `volume-grouping.ts`) and what a row shows: the filesystem tag, the badges, the
     * eject or disconnect control, the disk-space line, and the inline rename field.
     */
    import { onDestroy, untrack } from 'svelte'
    import { getVolumes, getVolumesTimedOut, isVolumesRefreshing, isVolumeRetryFailed, requestVolumeRefresh } from '$lib/stores/volume-store.svelte'
    import { isVolumeBusy, isVolumeEjecting } from '$lib/stores/volume-busy-store.svelte'
    import { isRestricted } from '$lib/stores/restricted-paths-store.svelte'
    import { getCachedIcon, iconCacheVersion } from '$lib/icon-cache'
    import { dependOn } from '$lib/utils/reactivity'
    import { eventMatchesCommand } from '$lib/shortcuts'
    import { tString } from '$lib/intl/messages.svelte'
    import { restrictedFolderTooltip } from '$lib/system-strings.svelte'
    import { getFileSizeFormat } from '$lib/settings/reactive-settings.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import Icon from '$lib/ui/Icon.svelte'
    import Menu from '$lib/ui/Menu.svelte'
    import ShortcutChip from '$lib/ui/ShortcutChip.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import StatusGlyph from '$lib/ui/StatusGlyph.svelte'
    import { createMenu } from '$lib/ui/menu-controller.svelte'
    import type { MenuIcon, MenuItem, MenuRowContext, MenuSection } from '$lib/ui/menu-types'
    import { deviceVolumeLabel } from '$lib/adb/adb-volume-label'
    import { deviceRowState } from '$lib/adb/device-readiness'
    import {
        isReadyForFirstConnectPrompt,
        maybePromptFirstConnect,
        withdrawGonePrompts,
    } from '$lib/indexing/first-connect-trigger'
    import { silenceDrive } from '$lib/indexing/drive-index-prefs'
    import { setSetting } from '$lib/settings'
    import { getUsageBar, formatDiskSpaceShort } from '../disk-space-utils'
    import type { VolumeInfo } from '../types'
    import type { VolumeChangePayload } from '../pane/types'
    import ConnectionDot from './ConnectionDot.svelte'
    import DetachButton from './DetachButton.svelte'
    import DriveIndexBadge from './DriveIndexBadge.svelte'
    import ImageIndexDriveBadge from './ImageIndexDriveBadge.svelte'
    import UsbSpeedDot from './UsbSpeedDot.svelte'
    import { createDirectConnectionSwitches } from './direct-connection-switch.svelte'
    import { detachControlFor } from './detach-control'
    import { runDetach } from './detach-volume'
    import { isDriveRow } from './drive-index-manager.svelte'
    import { filesystemLabel } from './filesystem-label'
    import { pathForPickedVolume } from './picked-volume-path'
    import {
        rowMenuItems,
        runRowFix,
        runVolumeRowAction,
        volumeRowMenu,
        type RowMenuPick,
        type RowToggleKind,
    } from './row-menu'
    import { listSavedPlaces, setServerAutoReconnect, type SavedPlaceFacts } from './server-row-actions'
    import { shouldShowCheckmark } from './volume-checkmark'
    import { groupByCategory } from './volume-grouping'
    import { createVolumeSpaceManager } from './volume-space-manager.svelte'
    import type { DriveBadges } from './drive-badges.svelte'
    import type { FavoritesMenuOpenTrigger } from './favorites-analytics'

    interface Props {
        /** The volume the pane's path really sits on: the row wearing the checkmark. */
        containingVolumeId: string | null
        /** The index dots, shared with the chip so both placements fetch and subscribe once. */
        badges: DriveBadges
        /** The chip the list hangs under, read at open time. */
        getAnchor: () => HTMLElement | undefined
        /** The chip's whole control cluster: pressing a control in it doesn't close the list. */
        getChipCluster: () => HTMLElement | undefined
        onVolumeChange?: (change: VolumeChangePayload) => void
        /** Tab hands this switcher to the other pane; the dual-pane owner performs the handoff. */
        onSwitchPane: () => void
        /**
         * Hand the header over to the favorites menu: the "See N favorites" row, or ⌃D
         * typed right here (the row has just taught that key, so it has to work).
         */
        onShowFavorites: (trigger: FavoritesMenuOpenTrigger) => void
        /** So the chip can keep one header menu open at a time. */
        onOpenChange: (open: boolean) => void
    }

    const {
        containingVolumeId,
        badges,
        getAnchor,
        getChipCluster,
        onVolumeChange,
        onSwitchPane,
        onShowFavorites,
        onOpenChange,
    }: Props = $props()

    const volumes = $derived(getVolumes())
    const volumesTimedOut = $derived(getVolumesTimedOut())
    const volumesRefreshing = $derived(isVolumesRefreshing())
    const volumeRetryFailed = $derived(isVolumeRetryFailed())

    const RESTRICTED_FOLDER_TOOLTIP = $derived(restrictedFolderTooltip())
    /* Short, because a screen reader reads it on every restricted volume; the instruction
       above is the tooltip the whole volume row carries. */
    const RESTRICTED_FOLDER_LABEL = $derived(tString('fileExplorer.restrictedFolder.label'))
    const READ_ONLY_TOOLTIP = $derived(tString('fileExplorer.navigation.readOnlyTooltip'))

    const spaceManager = createVolumeSpaceManager()
    const {
        volumeSpaceMap,
        spaceTimedOutSet,
        spaceRetryingSet,
        spaceRetryFailedSet,
        spaceRetryAttemptedSet,
        spaceAutoRetryingSet,
    } = spaceManager

    // Generic macOS folder icon used as fallback when a volume has no icon (for example,
    // FDA-gated favorites whose icons aren't fetched yet to avoid TCC popups). Reading
    // `$iconCacheVersion` re-evaluates this once the icon lands.
    const dirIconFallback = $derived.by(() => {
        dependOn($iconCacheVersion)
        return getCachedIcon('dir')
    })

    const groupedVolumes = $derived(groupByCategory(volumes))
    const allVolumes = $derived(groupedVolumes.flatMap((g) => g.items))
    const favoritesCount = $derived(volumes.filter((v) => v.category === 'favorite').length)

    /**
     * What a row carries back on a pick: a volume row, or one of its row actions. A union
     * rather than a value prefix, so a pick can't be read as the wrong kind of row.
     */
    type SwitcherRow = { kind: 'volume'; volume: VolumeInfo } | RowMenuPick

    const directSwitches = createDirectConnectionSwitches()

    /**
     * The places a saved server entry backs, re-read on every open: Edit and Forget server need
     * one, and "Reconnect automatically" shows its switch.
     */
    let savedPlaces = $state(new Map<string, SavedPlaceFacts>())

    /** The top of the pane's footer (usage bar, status line), where the switcher stops. */
    function paneFooterTop(): number | undefined {
        return getAnchor()?.closest('.file-pane')?.querySelector('[data-pane-footer]')?.getBoundingClientRect().top
    }

    /** The row that hands the header over to the favorites menu, and teaches ⌃D doing it. */
    const SEE_FAVORITES_VALUE = 'favorites:see'

    function rowIcon(volume: VolumeInfo, restricted: boolean): MenuIcon | undefined {
        if (volume.category === 'cloud_drive') return { src: '/icons/sync-online-only.svg' }
        if (volume.category === 'mobile_device') return { src: '/icons/mobile-device.svg' }
        if (volume.category === 'network') return { lucide: 'globe' }
        // TCC-denied paths: `NSWorkspace.iconForFile` returns a confusing "no access"
        // placeholder, so the generic Aqua folder icon stands in.
        if (restricted && dirIconFallback) return { src: dirIconFallback }
        if (volume.icon) return { src: volume.icon }
        if (dirIconFallback) return { src: dirIconFallback }
        return { lucide: 'folder' }
    }

    /**
     * A row's → submenu: its actions, fixes, and switches (`row-menu.ts`, the one list
     * right-click opens too). An SMB share's "Use Cmdr's fast direct connection" is a switch,
     * on every share Rust knows a switch for, direct ones too, so a direct share can go back
     * to the macOS mount from here; checking it on a share the OS mounted runs "Connect
     * directly", and while it's on but the share is still OS-mounted, "Connect directly now"
     * offers the same flow as a fix. Both read the switch value re-fetched on every open and
     * the live `connectionState`, so neither is stale. ❗ A pick closes the whole menu before
     * `onSelect`, so nobody sees the check flip: the next open shows it, re-read from Rust.
     */
    function rowSubmenu(volume: VolumeInfo): MenuItem<SwitcherRow>[] | undefined {
        const menu = volumeRowMenu(volume, {
            busy: isVolumeBusy(volume.id),
            ejecting: isVolumeEjecting(volume.id),
            isSaved: savedPlaces.has(volume.id),
            directConnection: directSwitches.valueFor(volume.id),
            autoReconnect: savedPlaces.get(volume.id)?.autoReconnect,
        })
        return rowMenuItems(volume.id, menu, (entry) => ({ kind: 'row-entry', volume, entry }))
    }

    function toMenuItem(volume: VolumeInfo): MenuItem<SwitcherRow> {
        const restricted = isRestricted(volume.path)
        // What the DEVICE's presence makes of the row: openable or greyed, and the sentence
        // that says why. `null` readiness (every disk, every server) answers "openable,
        // nothing to say", so this costs non-device rows nothing. ❗ A device the daemon
        // lists but can't use is `disabled`: the primitive greys it and never opens it,
        // rather than sending the pane somewhere that answers nothing. ❌ A
        // `waiting_for_authorization` row is NOT one of these: opening it ends the silence.
        const rowState = deviceRowState(volume.deviceReadiness)
        return {
            value: volume.id,
            label: deviceVolumeLabel(volume, volumes),
            icon: rowIcon(volume, restricted),
            // "The pane is here", ❌ not a radio: a radio row would hide its eject button from
            // VoiceOver (`MenuItemCheck`).
            check: shouldShowCheckmark(volume, containingVolumeId) ? { kind: 'current' } : undefined,
            disabled: !rowState.openable,
            tooltip: rowState.tooltip ?? (restricted ? RESTRICTED_FOLDER_TOOLTIP : ''),
            submenu: rowSubmenu(volume),
            data: { kind: 'volume', volume },
        }
    }

    /** The volume a top-level row stands for; the "See N favorites" row has none. */
    function rowVolume(item: MenuItem<SwitcherRow>): VolumeInfo | undefined {
        return item.data?.kind === 'volume' ? item.data.volume : undefined
    }

    const sections: MenuSection<SwitcherRow>[] = $derived.by(() => [
        // One row on top for the favorites, which live in their own menu (⌃D). It keeps
        // them a click away and is where the key gets taught; ❌ the switcher lists no
        // favorites itself, so there's no second place to manage them from.
        {
            id: 'favorites',
            items: [
                {
                    value: SEE_FAVORITES_VALUE,
                    label: tString('fileExplorer.navigation.seeFavorites', { count: favoritesCount }),
                    icon: { lucide: 'star' },
                },
            ],
        },
        ...groupedVolumes.map((group) => ({
            id: group.category,
            heading: group.label || undefined,
            items: group.items.map(toMenuItem),
        })),
    ])

    /** Where focus was when the menu opened, so closing it doesn't move the focused pane. */
    let focusBeforeOpen: HTMLElement | null = null

    /**
     * ⌃D right here swaps to the favorites menu. Central dispatch is suppressed while a
     * header menu is open, so nothing else would answer it, and the row above has just
     * taught the key — it has to work where it's advertised. `eventMatchesCommand` means a
     * rebind follows.
     */
    function handleKey(event: KeyboardEvent): boolean {
        if (eventMatchesCommand(event, 'pane.switch')) {
            // Unlike every other consumer key, Tab must suppress the browser's focus walk:
            // the dual-pane owner closes this menu and opens the other pane's in one handoff.
            event.preventDefault()
            onSwitchPane()
            return true
        }
        if (!eventMatchesCommand(event, 'favorites.open')) return false
        onShowFavorites('command')
        return true
    }

    const menu = createMenu<SwitcherRow>({
        getSections: () => sections,
        onSelect: (item) => {
            void handleSelect(item)
        },
        onKey: handleKey,
        onOpenChange: (open) => {
            onOpenChange(open)
            if (!open) return
            void spaceManager.fetchVolumeSpaces(volumes)
            badges.fetchForRows(volumes)
            void directSwitches.fetchForRows(volumes)
            void listSavedPlaces().then((places) => {
                savedPlaces = places
            })
        },
        restoreFocus: () => {
            // ❗ Back to whatever held focus, ❌ not to this pane: ⌥F2 opens the OTHER pane's
            // switcher, and closing it must not move the focus across.
            focusBeforeOpen?.focus()
            focusBeforeOpen = null
        },
        // ❗ The chip's own controls sit BESIDE the anchor, and ejecting from one deliberately
        // leaves the list open so several drives can go in a row.
        keepOpenWithin: getChipCluster,
    })

    export function open(): void {
        if (menu.isOpen) return
        const anchor = getAnchor()
        if (!anchor) return
        focusBeforeOpen = document.activeElement instanceof HTMLElement ? document.activeElement : null
        menu.openUnder(anchor)
        // Land the cursor on the row wearing the checkmark, so Enter re-opens where you
        // already are; with nothing checked the primitive's first row stands.
        const checked = allVolumes.find((volume) => shouldShowCheckmark(volume, containingVolumeId))
        if (checked) menu.highlight(checked.id)
    }

    export function close(): void {
        menu.close()
    }

    export function toggle(): void {
        if (menu.isOpen) menu.close()
        else open()
    }

    export function getIsOpen(): boolean {
        return menu.isOpen
    }

    async function handleSelect(item: MenuItem<SwitcherRow>): Promise<void> {
        if (item.value === SEE_FAVORITES_VALUE) {
            onShowFavorites('switcher_row')
            return
        }
        const row = item.data
        if (row?.kind === 'row-entry') {
            await runRowEntry(row)
            return
        }
        if (row) openVolume(row.volume)
    }

    /**
     * A row action or switch. Open is the row's own pick, so it moves THIS pane the way a
     * click on the row does; every other action goes where the palette's does.
     */
    async function runRowEntry({ volume, entry }: RowMenuPick): Promise<void> {
        if (entry.type === 'toggle') {
            await flipToggle[entry.toggle](volume)
            return
        }
        if (entry.type === 'fix') {
            await runRowFix({ volume, fix: entry.fix })
            return
        }
        if (entry.action === 'open') {
            openVolume(volume)
            return
        }
        await runVolumeRowAction({ volume, action: entry.action })
    }

    /** What flipping each row switch does. A `Record`, so a new `RowToggleKind` won't compile until it's handled. */
    const flipToggle: Record<RowToggleKind, (volume: VolumeInfo) => Promise<void>> = {
        'direct-connection': (volume) => directSwitches.pick(volume, volumes),
        // A row shows the switch only once the saved list answered, so the flip starts from it.
        'auto-reconnect': (volume) => setServerAutoReconnect(volume.id, !savedPlaces.get(volume.id)?.autoReconnect),
    }

    function openVolume(volume: VolumeInfo): void {
        // A saved server place opens on its start folder; anything else at its root.
        onVolumeChange?.({ volumeId: volume.id, volumePath: volume.path, targetPath: pathForPickedVolume(volume) })
        // The first-connect indexing prompt (D6) waits for the drive below.
        if (isDriveRow(volume)) awaitingFirstConnect = volume.id
    }

    // An offer to index a drive that went away (ejected, session gone) is withdrawn.
    $effect(() => {
        withdrawGonePrompts(volumes)
    })

    /** A drive picked here whose indexing prompt waits for it to be live and the pane on it. */
    let awaitingFirstConnect = $state<string | null>(null)
    $effect(() => {
        const volume = volumes.find((v) => v.id === awaitingFirstConnect)
        if (!volume || !isReadyForFirstConnectPrompt(volume, containingVolumeId)) return
        awaitingFirstConnect = null
        // Self-gates on settings, per-drive silence, and whether the drive is already indexed.
        void maybePromptFirstConnect(volume.id, volume.name, {
            onEnable: (vid) => { badges.runAction(vid, 'enable') },
            onSilenceDrive: (vid) => { silenceDrive(vid) },
            onSilenceAll: () => { setSetting('indexing.askForEachDrive', false) },
        })
    })

    // Clear cached space info when the volume list changes (mount/unmount/MTP connect) and
    // re-fetch if the menu is open.
    let prevVolumeIds = ''
    $effect(() => {
        const ids = volumes.map((v) => v.id).join(',')
        if (prevVolumeIds && ids !== prevVolumeIds) {
            spaceManager.clearAll()
            if (untrack(() => menu.isOpen)) void spaceManager.fetchVolumeSpaces(volumes)
        }
        prevVolumeIds = ids
    })

    onDestroy(() => {
        spaceManager.destroy()
        menu.destroy()
    })
</script>

<!-- The name the switcher already carries in Settings > Keyboard shortcuts, so screen readers
     and the shortcut scope say the same thing (and M2 adds no new copy to translate). -->
<!-- ❗ Stops above the pane's footer and scrolls: over it, the last row sat on the status
     bar with the status text showing through the glass. -->
<Menu {menu} ariaLabel={tString('shortcuts.scope.volumeChooser')} getBottomLimit={paneFooterTop}>
    {#snippet label(ctx: MenuRowContext<SwitcherRow>)}
        {@const volume = rowVolume(ctx.item)}
        <!-- TCC-restricted entries read quiet + italic (the shared `--color-text-quiet`
             token, as the file list's hidden entries do); a pinned place nobody has
             dialed is quiet too, so the connected rows above it read as the live ones.
             ❌ Not `aria-disabled`: opening one is what dials it. The favorites row
             carries no volume and takes neither treatment. -->
        <span
            class="volume-label"
            class:is-restricted={volume ? isRestricted(volume.path) : false}
            class:is-saved-place={volume?.connectionState === 'saved'}>{ctx.item.label}</span
        >
    {/snippet}

    {#snippet trailing(ctx: MenuRowContext<SwitcherRow>)}
        {@const volume = rowVolume(ctx.item)}
        {#if ctx.item.value === SEE_FAVORITES_VALUE}
            <!-- The key that opens the same menu from anywhere, live: a rebind shows here.
                 Not clickable — inside a row, a second target would double-activate. -->
            <ShortcutChip commandId="favorites.open" clickable={false} />
        {:else if volume}
            {@const fsLabel = filesystemLabel(volume)}
            <!-- Declared up here because `{@const}` has to be an immediate child of a block;
                 it's spent at the end of the row, in `.row-trailing`. -->
            {@const detach = detachControlFor(volume, {
                busy: isVolumeBusy(volume.id),
                ejecting: isVolumeEjecting(volume.id),
            })}
            {#if fsLabel}
                <!-- Filesystem name tag, sitting just right of the volume name. Quiet
                     secondary text so it reads as metadata, not a second title. -->
                <span class="volume-fs" class:is-saved-place={volume.connectionState === 'saved'}>{fsLabel}</span>
            {/if}
            {#if isRestricted(volume.path)}
                <!-- No tooltip of its own: the whole row already carries this same string, and
                     the row is the honest target (the italic dimmed label needs the
                     explanation as much as the glyph). -->
                <StatusGlyph name="info" label={RESTRICTED_FOLDER_LABEL} tooltip={null} />
            {/if}
            <span class="row-trailing">
                {#if volume.mountIsReadOnly}
                    <span class="read-only-indicator" use:tooltip={READ_ONLY_TOOLTIP}
                        ><Icon name="lock" size={14} aria-hidden="true" /></span
                    >
                {/if}
                {#if volume.connectionState}
                    <ConnectionDot
                        state={volume.connectionState}
                        hasOsMountFallback={volume.capabilities?.hasOsMountFallback ?? false}
                    />
                {/if}
                {#if volume.usbSpeed}
                    <UsbSpeedDot speed={volume.usbSpeed} />
                {/if}
                {#if isDriveRow(volume)}
                    {@const indexStatus = badges.statusFor(volume.id)}
                    {#if indexStatus}
                        <DriveIndexBadge
                            volumeId={volume.id}
                            status={indexStatus}
                            driveName={volume.name}
                            onAction={badges.runAction}
                        />
                    {/if}
                    {@const imageState = badges.imageStateFor(volume.id)}
                    {#if imageState}
                        <ImageIndexDriveBadge volumeId={volume.id} volumeState={imageState} />
                    {/if}
                {/if}
                <!-- ❗ The words AND the action come from one answer: a server says
                     Disconnect and drops its session (the place stays saved), a phone says
                     Disconnect and ejects, a `saved` row gets nothing. `detach-control.ts`. -->
                {#if detach}
                    <DetachButton {...detach.button} onclick={() => { void runDetach(volume, detach.action) }} />
                {/if}
            </span>
        {/if}
    {/snippet}

    {#snippet below(ctx: MenuRowContext<SwitcherRow>)}
        {@const volume = rowVolume(ctx.item)}
        {#if volume}
            {@const space = volumeSpaceMap.get(volume.id)}
            {#if space}
                {@const bar = getUsageBar(space)}
                <div class="volume-space-info">
                    <!-- No bar where there is no total to fill it against: the line carries
                         the used figure on its own. -->
                    {#if bar}
                        <div class="volume-space-bar">
                            <div
                                class="volume-space-fill"
                                style:width="{bar.usedPercent}%"
                                style:background-color="var({bar.cssVar})"
                            ></div>
                        </div>
                    {/if}
                    <span class="volume-space-text">{formatDiskSpaceShort(space, getFileSizeFormat())}</span>
                </div>
            {:else if spaceRetryingSet.has(volume.id)}
                <div
                    class="volume-space-info volume-space-timeout"
                    use:tooltip={spaceAutoRetryingSet.has(volume.id)
                        ? tString('fileExplorer.navigation.spaceRetryingAuto')
                        : tString('fileExplorer.navigation.spaceRetrying')}
                >
                    <div class="volume-space-bar volume-space-bar-timeout"><Spinner size="sm" /></div>
                    <span class="volume-space-text volume-space-text-timeout"
                        >{tString('fileExplorer.navigation.spaceRetryingText')}</span
                    >
                </div>
            {:else if spaceTimedOutSet.has(volume.id)}
                <!-- svelte-ignore a11y_click_events_have_key_events -->
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <div
                    class="volume-space-info volume-space-timeout"
                    class:space-shake={spaceRetryFailedSet.has(volume.id)}
                    use:tooltip={spaceRetryAttemptedSet.has(volume.id)
                        ? tString('fileExplorer.navigation.spaceStillUnavailable')
                        : tString('fileExplorer.navigation.spaceFetchFailed')}
                    onclick={(e: MouseEvent) => {
                        e.stopPropagation()
                        spaceManager.retryVolumeSpace(volume)
                    }}
                >
                    <div class="volume-space-bar volume-space-bar-timeout">
                        <span class="volume-space-timeout-icon">?</span>
                    </div>
                    <span class="volume-space-text volume-space-text-timeout"
                        >{tString('fileExplorer.navigation.spaceUnavailableText')}</span
                    >
                </div>
            {/if}
        {/if}
    {/snippet}

    {#snippet footer()}
        {#if volumesTimedOut}
            <div class="footer-separator"></div>
            <div class="timeout-warning-row" class:retry-failed={volumeRetryFailed}>
                <span class="timeout-warning-text"
                    >{volumeRetryFailed
                        ? tString('fileExplorer.navigation.volumesStillUnreachable')
                        : tString('fileExplorer.navigation.volumesMayBeMissing')}</span
                >
                <button
                    class="timeout-retry-button"
                    disabled={volumesRefreshing}
                    use:tooltip={tString('fileExplorer.navigation.refreshVolumeList')}
                    onclick={() => { requestVolumeRefresh() }}
                >
                    <span class="timeout-retry-icon" class:is-retrying={volumesRefreshing}
                        ><Icon name="rotate-cw" size={14} aria-hidden="true" /></span
                    >
                </button>
            </div>
        {/if}
    {/snippet}
</Menu>

<style>
    .volume-label {
        flex: 1;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    /*noinspection CssUnusedSymbol*/
    .volume-label.is-restricted {
        font-style: italic;
        color: var(--color-text-quiet);
    }

    /*noinspection CssUnusedSymbol*/
    .is-saved-place {
        color: var(--color-text-quiet);
    }

    .volume-fs {
        flex-shrink: 0;
        margin-left: var(--spacing-sm);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        white-space: nowrap;
    }

    .read-only-indicator {
        display: inline-flex;
        align-items: center;
        opacity: 0.7;
    }

    /* The right end of a row. `display: contents` so each badge stays a direct flex child of
       the row (no box of its own, and an empty cluster costs the row nothing), while the rule
       under it spaces the badges: the row's own `gap` plus this, so two dots sit 16px apart
       the way they did before the `Menu` primitive. The children are other components, whose
       roots this component's scoped CSS can't reach without `:global`. */
    .row-trailing {
        display: contents;
    }

    .row-trailing > :global(* + *) {
        margin-left: var(--spacing-sm);
    }

    .volume-space-info {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        /* stylelint-disable-next-line declaration-property-value-disallowed-list -- left pad aligns to a computed icon+gap offset; 14px/16px are measured widths */
        padding: 0 var(--spacing-md) var(--spacing-xs) calc(14px + var(--spacing-sm) + 16px + var(--spacing-sm));
    }

    .volume-space-bar {
        flex: 1;
        height: 2px;
        background-color: var(--color-disk-track);
        border-radius: var(--radius-sm);
    }

    .volume-space-fill {
        height: 100%;
        border-radius: var(--radius-sm);
    }

    .volume-space-text {
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        white-space: nowrap;
        flex-shrink: 0;
    }

    /* Volume space timeout placeholder */
    .volume-space-timeout {
        cursor: default;
    }

    .volume-space-bar-timeout {
        border: 1px dashed var(--color-border);
        background-color: transparent;
        display: flex;
        align-items: center;
        justify-content: center;
        height: 8px;
    }

    .volume-space-timeout-icon {
        font-size: var(--font-size-xs);
        color: var(--color-warning);
        line-height: var(--font-line-height-flat);
        transition: opacity var(--transition-base);
    }

    /* Shake on retry failure */
    /*noinspection CssUnusedSymbol*/
    .space-shake {
        animation: shake 300ms ease;
    }

    @keyframes shake {
        0%,
        100% {
            transform: translateX(0);
        }
        25% {
            transform: translateX(-3px);
        }
        75% {
            transform: translateX(3px);
        }
    }

    .volume-space-text-timeout {
        color: var(--color-warning);
    }

    /* The footer's own rule, matching the primitive's between-section separators. */
    .footer-separator {
        height: 1px;
        background-color: var(--color-border-strong);
        margin: var(--spacing-xs) var(--spacing-sm);
    }

    .timeout-warning-row {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        padding: var(--spacing-xs) var(--spacing-md);
    }

    .timeout-warning-text {
        font-size: var(--font-size-xs);
        color: var(--color-warning);
        flex: 1;
    }

    .timeout-retry-button {
        background: none;
        border: none;
        padding: 0 var(--spacing-xs);
        cursor: default;
        color: var(--color-warning-text);
        font-size: var(--font-size-md);
        line-height: var(--font-line-height-flat);
        border-radius: var(--radius-sm);
        transition: background-color var(--transition-base);
    }

    .timeout-retry-button:hover {
        background-color: var(--color-bg-tertiary);
    }

    .timeout-retry-button:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 1px;
    }

    .timeout-retry-button:disabled {
        opacity: 0.4;
        cursor: not-allowed;
    }

    .timeout-retry-icon {
        display: inline-flex;
        align-items: center;
        justify-content: center;
    }

    /*noinspection CssUnusedSymbol*/
    .timeout-retry-icon.is-retrying {
        animation: spin 0.8s linear infinite;
    }

    /*noinspection CssUnusedSymbol*/
    .timeout-warning-row.retry-failed {
        animation: flash-warning 0.3s ease;
    }

    @keyframes flash-warning {
        0%,
        100% {
            background-color: transparent;
        }
        50% {
            background-color: var(--color-warning-bg);
        }
    }

    @media (prefers-reduced-motion: reduce) {
        /*noinspection CssUnusedSymbol*/
        .timeout-retry-icon.is-retrying {
            animation: none;
        }

        /*noinspection CssUnusedSymbol*/
        .timeout-warning-row.retry-failed {
            animation: none;
        }

        /* Reduced motion: opacity flash instead of shake */
        /*noinspection CssUnusedSymbol*/
        .space-shake {
            animation: flash-warning 300ms ease;
        }
    }
</style>
