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
     * dot, the tooltip, and the menu shell. The state→color/copy mapping is the
     * pure `drive-index-status.ts` (unit-tested).
     */
    import type { VolumeIndexStatus } from '$lib/ipc/bindings'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { tString } from '$lib/intl/messages.svelte'
    import { formatDateForDisplay } from '$lib/settings/format-utils'
    import {
        driveIndexState,
        driveIndexMenuActions,
        driveIndexMenuLabelKey,
        driveIndexDuration,
        driveIndexScanProgress,
        hasLastScanFacts,
        type DriveIndexMenuAction,
        type DriveIndexState,
    } from './drive-index-status'
    import type { DriveScanProgress } from './drive-index-manager.svelte'

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
        /**
         * Live in-flight scan progress for THIS badge's volume (entries scanned
         * + start time), or `undefined` when it isn't scanning. When present and
         * the badge is `scanning`, the tooltip shows a live count + elapsed clock.
         */
        scanProgress?: DriveScanProgress | undefined
        /** The parent runs the actual IPC for a picked menu action. */
        onAction: (volumeId: string, action: DriveIndexMenuAction) => void
    }

    const { volumeId, status, breadcrumb = false, scanProgress, onAction }: Props = $props()

    const badgeState = $derived<DriveIndexState>(driveIndexState(status))

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

    // A 1 Hz clock that ticks ONLY while this badge is scanning, so the tooltip's
    // elapsed time advances live (the count itself comes from 500 ms backend
    // events). Idle badges never run a timer.
    let now = $state(Date.now())
    $effect(() => {
        if (badgeState !== 'scanning') return
        now = Date.now()
        const id = setInterval(() => {
            now = Date.now()
        }, 1000)
        return () => {
            clearInterval(id)
        }
    })

    // The live "Indexing… N files · 0:42" copy for a scanning badge. Falls back to
    // the static phrasing before the first progress event arrives (no count yet).
    const scanningTooltip = $derived.by(() => {
        if (!scanProgress) return tString('fileExplorer.navigation.driveIndex.tooltipScanning')
        const { key, params } = driveIndexScanProgress(scanProgress.entriesScanned, scanProgress.scanStartedAt, now)
        return tString(key, params)
    })

    const tooltipText = $derived.by(() => {
        switch (badgeState) {
            case 'disabled':
                return tString('fileExplorer.navigation.driveIndex.tooltipDisabled')
            case 'scanning':
                return scanningTooltip
            case 'stale':
                return tString('fileExplorer.navigation.driveIndex.tooltipStale')
            case 'fresh':
                return hasLastScanFacts(status)
                    ? tString('fileExplorer.navigation.driveIndex.tooltipFresh', {
                          date: lastIndexedDate,
                          duration: durationText,
                      })
                    : tString('fileExplorer.navigation.driveIndex.tooltipFreshNoScan')
        }
    })

    const menuActions = $derived(driveIndexMenuActions(badgeState))
    const showFooter = $derived(hasLastScanFacts(status))

    let menuOpen = $state(false)
    let badgeRef: HTMLButtonElement | undefined = $state()
    let menuRef: HTMLDivElement | undefined = $state()

    function toggleMenu(e: MouseEvent) {
        e.stopPropagation()
        menuOpen = !menuOpen
    }

    function pickAction(action: DriveIndexMenuAction, e: MouseEvent) {
        e.stopPropagation()
        menuOpen = false
        onAction(volumeId, action)
    }

    function handleClickOutside(event: MouseEvent) {
        const target = event.target as Node
        if (badgeRef?.contains(target) || menuRef?.contains(target)) return
        menuOpen = false
    }

    function handleKeyDown(event: KeyboardEvent) {
        if (event.key === 'Escape' && menuOpen) {
            menuOpen = false
            badgeRef?.focus()
        }
    }

    $effect(() => {
        if (!menuOpen) return
        document.addEventListener('click', handleClickOutside, true)
        document.addEventListener('keydown', handleKeyDown)
        return () => {
            document.removeEventListener('click', handleClickOutside, true)
            document.removeEventListener('keydown', handleKeyDown)
        }
    })
</script>

<button
    type="button"
    bind:this={badgeRef}
    class="drive-index-badge drive-index-badge-{badgeState}"
    class:breadcrumb-drive-index-badge={breadcrumb}
    aria-label={`${tString('fileExplorer.navigation.driveIndex.ariaLabel')}: ${tooltipText}`}
    aria-haspopup="menu"
    aria-expanded={menuOpen}
    use:tooltip={menuOpen ? '' : tooltipText}
    onclick={toggleMenu}
></button>

{#if menuOpen}
    <div class="drive-index-menu" bind:this={menuRef} role="menu" onclick={(e: MouseEvent) => { e.stopPropagation() }}>
        {#each menuActions as action (action)}
            <button type="button" class="drive-index-menu-item" role="menuitem" onclick={(e: MouseEvent) => { pickAction(action, e) }}>
                {tString(driveIndexMenuLabelKey(action))}
            </button>
        {/each}
        {#if showFooter}
            <div class="drive-index-menu-separator"></div>
            <div class="drive-index-menu-footer">
                {tString('fileExplorer.navigation.driveIndex.footer', {
                    date: lastIndexedDate,
                    duration: durationText,
                })}
            </div>
        {/if}
    </div>
{/if}

<style>
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

    /* In a dropdown row, push the badge to the far right (same as the SMB dot). */
    :global(.volume-item) .drive-index-badge {
        margin-left: auto;
    }

    /* If another right-aligned badge precedes us, just add a small gap. */
    :global(.volume-item) :global(.smb-indicator) + .drive-index-badge,
    :global(.volume-item) :global(.usb-speed-indicator) + .drive-index-badge,
    :global(.volume-item) :global(.read-only-indicator) + .drive-index-badge {
        margin-left: var(--spacing-sm);
    }

    /* Closed-breadcrumb placement: a small left margin so it sits next to the
       SMB / USB badges instead of jamming against them. */
    .breadcrumb-drive-index-badge {
        margin-left: var(--spacing-xs);
    }

    .drive-index-menu {
        position: absolute;
        top: 100%;
        left: 0;
        margin-top: var(--spacing-xs);
        min-width: 220px;
        /* Same frosted glass as the breadcrumb popup. See `.volume-dropdown`. */
        background: var(--color-bg-glass);
        -webkit-backdrop-filter: saturate(180%) blur(20px);
        backdrop-filter: saturate(180%) blur(20px);
        border: 0.5px solid var(--color-border-glass);
        border-radius: var(--radius-md);
        box-shadow: var(--shadow-md);
        z-index: var(--z-overlay);
        padding: var(--spacing-xs) 0;
    }

    .drive-index-menu-item {
        display: block;
        width: 100%;
        text-align: left;
        padding: var(--spacing-sm) var(--spacing-md);
        background: none;
        border: none;
        color: var(--color-text-primary);
        font: inherit;
        cursor: default;
        white-space: nowrap;
    }

    .drive-index-menu-item:hover,
    .drive-index-menu-item:focus-visible {
        background-color: var(--color-accent-subtle);
        outline: none;
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

    /* Reduced transparency: drop the blur (the glass token flips opaque in app.css). */
    :global(html.reduce-transparency) .drive-index-menu {
        -webkit-backdrop-filter: none;
        backdrop-filter: none;
    }
</style>
