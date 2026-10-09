<script lang="ts">
    /**
     * Per-drive index freshness badge: a small colored dot (gray/blue/green/
     * yellow) mirroring the existing SMB connection light and USB-speed ring in
     * `VolumeBreadcrumb.svelte`. Clicking it opens a small themed menu (turn
     * on/off, rescan, stop) with a "last indexed" footer.
     *
     * Two placements share this one component: the always-visible active-drive
     * badge next to the dropdown trigger, and a per-row badge inside the
     * dropdown. The parent owns the IPC actions (so it can route an SMB
     * `credentials_needed` refusal into its login flow); this component owns the
     * dot, the tooltip, and what the menu says. The state→color/copy mapping is
     * the pure `drive-index-status.ts` (unit-tested).
     *
     * The menu is the house `Menu` (`$lib/ui/DETAILS.md` § Menu), which owns keys,
     * cursor, placement, focus, and closing. In the dropdown placement it's a menu
     * opened from inside another one, a case the primitive handles for both.
     */
    import { onDestroy } from 'svelte'
    import type { VolumeIndexStatus } from '$lib/ipc/bindings'
    import Menu from '$lib/ui/Menu.svelte'
    import { createMenu } from '$lib/ui/menu-controller.svelte'
    import type { MenuSection } from '$lib/ui/menu-types'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { tString } from '$lib/intl/messages.svelte'
    import { formatInteger } from '$lib/intl/number-format'
    import { formatDateForDisplay } from '$lib/settings/format-utils'
    import {
        driveIndexState,
        driveIndexMenuActions,
        driveIndexMenuLabelKey,
        driveIndexDuration,
        driveIndexCoalescedNote,
        driveIndexUnreadableNote,
        hasLastScanFacts,
        type DriveIndexMenuAction,
        type DriveIndexState,
    } from './drive-index-status'
    import { getVolumeActivity, getVolumeAggregation, getVolumePhase, placeholderActivity } from '$lib/indexing'
    import IndexingDriveRow from '$lib/indexing/IndexingDriveRow.svelte'
    import { getDriveIndexingEnabled } from '$lib/settings/reactive-settings.svelte'

    interface Props {
        /** The drive this badge describes. */
        volumeId: string
        /** Backend index status (freshness + last-scan facts). */
        status: VolumeIndexStatus
        /**
         * Larger dot + left margin for the always-visible breadcrumb placement
         * (matches the `.breadcrumb-smb-indicator` spacing). Off for dropdown rows.
         */
        breadcrumb?: boolean
        /** This drive's display name, for the scanning tooltip's shared body. */
        driveName: string
        /**
         * Whether the drive answers right now (`answersNow`). A phone nobody has
         * connected doesn't, so the menu drops the actions that start a walk.
         */
        answers: boolean
        /** The parent runs the actual IPC for a picked menu action. */
        onAction: (volumeId: string, action: DriveIndexMenuAction) => void
    }

    const { volumeId, status, breadcrumb = false, driveName, answers, onAction }: Props = $props()

    const badgeState = $derived<DriveIndexState>(driveIndexState(status))

    // The MASTER drive-indexing switch. While it's off the backend refuses every
    // start, so this drive's own choice is overridden: the menu drops its actions
    // and says so instead of offering buttons that would be refused. The drive's
    // choice is remembered and comes back with the master switch.
    const masterEnabled = $derived(getDriveIndexingEnabled())

    // ISO date (date portion only) for the "last indexed" copy. We always format
    // ISO regardless of the user's date-format preference, per the plan (ISO
    // dates everywhere). `formatDateForDisplay('iso')` yields "YYYY-MM-DD HH:mm";
    // take the date half.
    const lastIndexedDate = $derived(
        status.scanCompletedAt != null
            ? formatDateForDisplay(status.scanCompletedAt, 'iso', '').text.split(' ')[0]
            : '',
    )
    const duration = $derived(driveIndexDuration(status.scanDurationMs))
    const durationText = $derived(duration ? tString(duration.key, duration.params) : '')

    // This volume's live scan/replay activity + aggregation, the SINGLE source of
    // live indexing progress (`index-state`). The scanning tooltip renders the
    // shared status body from it, so the badge and the corner indicator show the
    // identical representation. `undefined` for a non-root (SMB/MTP) volume in
    // the window between the freshness flip to `scanning` and the first ~500 ms
    // progress tick (index-state's root-only backfill doesn't hydrate it) — then
    // the static text fallback shows instead of an empty tooltip.
    const activity = $derived(getVolumeActivity(volumeId))
    const aggregation = $derived(getVolumeAggregation(volumeId))
    // This volume's top-level phase, so the checklist stays visible through the
    // reconcile step (scan + aggregation both done, only the phase event marks it):
    // there's no live `activity` then, so fall back to a placeholder the checklist
    // reads its state from via the phase. `undefined` until the first signal lands.
    const phase = $derived(getVolumePhase(volumeId))
    const bodyActivity = $derived(activity ?? (phase != null ? placeholderActivity(volumeId) : undefined))

    // The shared body lives in a hidden host; we hand its inner element to the
    // tooltip as `contentEl`. Rich only once there's something to show (live
    // activity, or a phase mid-pipeline), else the static fallback text. Mirrors
    // the indicator's `contentEl` pattern. The host mounts only while the tooltip
    // is open (`onOpenChange`): mounted, the body re-renders on every progress
    // event and runs a 1 Hz clock, for a tooltip nobody is reading.
    let scanBodyEl = $state<HTMLDivElement>()
    let scanTooltipOpen = $state(false)
    const hasRichScanningBody = $derived(badgeState === 'scanning' && bodyActivity != null)

    // The "macOS lost track of changes N times" paragraph, appended to the tooltip
    // whenever signals were coalesced since the last full check. Recomputed when
    // `status` changes (the manager refreshes it on index events), so the hour
    // counts can lag a live clock by a few minutes — fine for whole hours.
    const coalescedNote = $derived(driveIndexCoalescedNote(status, Date.now() / 1000))
    const coalescedText = $derived.by(() => {
        if (!coalescedNote) return ''
        // Counts ride as preformatted `*Text` params (formatting is single-sourced
        // in `$lib/intl`); the raw integers only drive the plural branches.
        const params: Record<string, string | number> = {
            count: coalescedNote.count,
            countText: formatInteger(coalescedNote.count),
        }
        if (coalescedNote.hours != null) {
            params.hours = coalescedNote.hours
            params.hoursText = formatInteger(coalescedNote.hours)
        }
        if (coalescedNote.remaining != null) {
            params.remaining = coalescedNote.remaining
            params.remainingText = formatInteger(coalescedNote.remaining)
        }
        return tString(coalescedNote.key, params)
    })

    // The "done, with holes" footnote: a finished index that holds no rows for
    // some folders says so here, because search's coverage note is otherwise the
    // only place it's said and someone who never searches never sees it.
    const unreadableNote = $derived(driveIndexUnreadableNote(status))
    const unreadableText = $derived(
        unreadableNote
            ? tString(unreadableNote.key, {
                  count: unreadableNote.count,
                  countText: formatInteger(unreadableNote.count),
              })
            : '',
    )

    // The text tooltip per state. The `scanning` text is the static fallback for
    // the no-activity-yet window (the rich body replaces it once activity lands);
    // unified onto the `indexing.scan.*` family so both surfaces say the same.
    const stateTooltipText = $derived.by(() => {
        // The master switch is the honest headline whenever it's off: the dot is
        // gray because indexing is off everywhere, not because of this drive.
        if (!masterEnabled) return tString('fileExplorer.navigation.driveIndex.tooltipIndexingOff')
        switch (badgeState) {
            case 'disabled':
                return tString('fileExplorer.navigation.driveIndex.tooltipDisabled')
            case 'scanning':
                return tString('indexing.scan.label')
            case 'stale':
                // Nothing watches a phone over ADB, so its index is stale while it's
                // plugged in, and a disconnect is the wrong thing to blame.
                return status.liveWatch
                    ? tString('fileExplorer.navigation.driveIndex.tooltipStale')
                    : tString('fileExplorer.navigation.driveIndex.tooltipStalePhone')
            case 'fresh':
                return hasLastScanFacts(status)
                    ? tString('fileExplorer.navigation.driveIndex.tooltipFresh', {
                          date: lastIndexedDate,
                          duration: durationText,
                      })
                    : tString('fileExplorer.navigation.driveIndex.tooltipFreshNoScan')
            case 'failed':
                return tString('fileExplorer.navigation.driveIndex.tooltipFailed')
        }
    })

    // Each note is its own paragraph under the state line (`.cmdr-tooltip` is
    // `white-space: pre-line`, so a newline renders). Both can be true at once.
    const tooltipText = $derived(
        [stateTooltipText, coalescedText, unreadableText].filter((line) => line !== '').join('\n'),
    )

    // The tooltip param: the rich DOM body while scanning with live activity,
    // else the text tooltip for this state. The text also stands in for the body
    // in the moment before it mounts. The template blanks it while the menu is open.
    const tooltipParam = $derived(
        hasRichScanningBody
            ? {
                  text: tooltipText,
                  contentEl: scanBodyEl ?? undefined,
                  onOpenChange: (open: boolean) => (scanTooltipOpen = open),
              }
            : tooltipText,
    )

    const menuActions = $derived(driveIndexMenuActions(badgeState, masterEnabled, answers))
    const showFooter = $derived(hasLastScanFacts(status))

    // One section of plain action rows, or none at all while the master switch is
    // off — the footer snippet then carries the explanation instead.
    const sections = $derived<MenuSection<DriveIndexMenuAction>[]>(
        menuActions.length === 0
            ? []
            : [
                  {
                      id: 'actions',
                      items: menuActions.map((action) => ({
                          value: action,
                          label: tString(driveIndexMenuLabelKey(action)),
                          data: action,
                      })),
                  },
              ],
    )

    let badgeRef: HTMLButtonElement | undefined = $state()
    /** Whatever held focus when the menu opened, so closing it doesn't move focus anywhere new. */
    let focusBeforeOpen: HTMLElement | null = null

    const menu = createMenu<DriveIndexMenuAction>({
        getSections: () => sections,
        onSelect: (item) => {
            if (item.data) onAction(volumeId, item.data)
        },
        restoreFocus: () => {
            // ❗ Back to whatever held focus, ❌ not to the badge: in a switcher row that's the
            // switcher's own surface, which owns the keyboard the moment this menu lets go of it.
            focusBeforeOpen?.focus()
            focusBeforeOpen = null
        },
    })

    function toggleMenu(): void {
        if (!badgeRef) return
        if (!menu.isOpen)
            focusBeforeOpen = document.activeElement instanceof HTMLElement ? document.activeElement : null
        menu.toggleUnder(badgeRef)
    }

    onDestroy(() => {
        menu.destroy()
    })
</script>

<button
    type="button"
    bind:this={badgeRef}
    class="drive-index-badge drive-index-badge-{badgeState}"
    class:breadcrumb-drive-index-badge={breadcrumb}
    aria-label={`${tString('fileExplorer.navigation.driveIndex.ariaLabel')}: ${tooltipText}`}
    aria-haspopup="menu"
    aria-expanded={menu.isOpen}
    use:tooltip={menu.isOpen ? '' : tooltipParam}
    onclick={toggleMenu}
></button>

{#if hasRichScanningBody && bodyActivity && scanTooltipOpen}
    <!-- The scanning tooltip's rich body. Lives in a hidden host; the tooltip
         action adopts the INNER element (`scanBodyEl`) as `contentEl`, not the
         hidden host (an adopted element keeps its own `hidden`, so a hidden host
         would render an empty tooltip). Mirrors `IndexingStatusIndicator`. -->
    <div hidden>
        <div bind:this={scanBodyEl} class="scan-tooltip-body">
            <IndexingDriveRow activity={bodyActivity} {aggregation} {driveName} showHeading={false} />
        </div>
    </div>
{/if}

<Menu {menu} ariaLabel={tString('fileExplorer.navigation.driveIndex.ariaLabel')}>
    <!-- Both things under the last row. Neither is a ROW (one wraps to several lines, the
         other is a quiet caption), which is what keeps this menu off the OS's
         (`docs/guides/building-ui.md` § Building a menu). There's always something above the
         separator: the actions, or the note that replaces them. -->
    {#snippet footer()}
        {#if !masterEnabled}
            <!-- Master switch off: no actions, one line saying why and where to
                 change it, plus the reassurance that this drive's own choice is kept. -->
            <p class="drive-index-menu-note">{tString('fileExplorer.navigation.driveIndex.menuIndexingOffNote')}</p>
        {:else if menuActions.length === 0}
            <!-- The drive doesn't answer yet (a phone nobody has connected), so
                 nothing here can start. Say what will. -->
            <p class="drive-index-menu-note">
                {tString('fileExplorer.navigation.driveIndex.refusedDisconnected', { name: driveName })}
            </p>
        {/if}
        {#if showFooter}
            <div class="drive-index-menu-separator"></div>
            <div class="drive-index-menu-footer">
                {tString('fileExplorer.navigation.driveIndex.footer', {
                    date: lastIndexedDate,
                    duration: durationText,
                })}
            </div>
        {/if}
    {/snippet}
</Menu>

<style>
    /* Stable width for the scanning tooltip's shared body, so it doesn't jitter
       as the live counters tick (the tooltip action measures once on show and
       can't see later content growth). The counters wrap within the tooltip's
       own `max-width` (on `.cmdr-tooltip`). Mirrors the corner indicator. */
    .scan-tooltip-body {
        min-width: 200px;
    }

    /* The dot mirrors `.smb-indicator` / `.usb-speed-indicator`: same 10px round
       shape, same flex sizing, but as a focusable <button> (it opens a menu). */
    .drive-index-badge {
        width: 10px;
        height: 10px;
        border-radius: 50%;
        flex-shrink: 0;
        opacity: 0.8;
        padding: 0;
        border: none;
        cursor: default;
        background-color: var(--color-text-tertiary);
    }

    .drive-index-badge:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 2px;
    }

    /*noinspection CssUnusedSymbol*/
    .drive-index-badge-disabled {
        background-color: var(--color-text-tertiary);
    }

    /*noinspection CssUnusedSymbol*/
    .drive-index-badge-scanning {
        background-color: var(--color-apple-blue);
    }

    /*noinspection CssUnusedSymbol*/
    .drive-index-badge-fresh {
        background-color: var(--color-allow);
    }

    /*noinspection CssUnusedSymbol*/
    .drive-index-badge-stale {
        background-color: var(--color-warning);
    }

    /* Red: the index DB died with a storage problem and indexing stopped. Distinct
       from gray (deliberately off) and yellow (stale but browsable). */
    /*noinspection CssUnusedSymbol*/
    .drive-index-badge-failed {
        background-color: var(--color-error);
    }

    /* The scanning dot pulses to signal live work, like the corner hourglass.
       Gated behind reduced-motion (honor the user's preference). */
    @media (prefers-reduced-motion: no-preference) {
        /*noinspection CssUnusedSymbol*/
        .drive-index-badge-scanning {
            animation: drive-index-pulse 2s ease-in-out infinite;
        }
    }

    @keyframes drive-index-pulse {
        0%,
        100% {
            opacity: 0.5;
        }
        50% {
            opacity: 1;
        }
    }

    /* In a switcher row the badge sits in the trailing cluster, which spaces it
       (`VolumeChooserMenu.svelte` § `.row-trailing`); nothing to do here. */

    /* Closed-breadcrumb placement: a small left margin so it sits next to the
       SMB / USB badges instead of jamming against them. */
    .breadcrumb-drive-index-badge {
        margin-left: var(--spacing-xs);
    }

    /* The surface, the rows, and their hover belong to the house `Menu`; what's left here is
       the two things under the last row that aren't rows.

       The master-switch-off explanation. Wraps (unlike the nowrap action rows), so
       the sentence stays readable inside the menu's width. */
    .drive-index-menu-note {
        margin: 0;
        padding: var(--spacing-xs) var(--spacing-md);
        max-width: 260px;
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        line-height: var(--font-line-height-normal);
    }

    .drive-index-menu-separator {
        height: 1px;
        background-color: var(--color-border-strong);
        margin: var(--spacing-xs) var(--spacing-sm);
    }

    .drive-index-menu-footer {
        padding: var(--spacing-xs) var(--spacing-md);
        color: var(--color-text-tertiary);
        font-size: var(--font-size-xs);
        white-space: nowrap;
    }
</style>
