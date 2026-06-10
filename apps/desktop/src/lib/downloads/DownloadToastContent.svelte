<script lang="ts">
    /**
     * Toast that shows up when the watcher detects a new file in ~/Downloads.
     *
     * Pure-prop-driven: every input is captured at toast-creation time and
     * never re-read. The shortcut hint in particular is snapshotted (not
     * subscribed live) so a remap that happens between the toast appearing
     * and the user clicking doesn't mutate the displayed hint mid-flight.
     *
     * Whole body is mouse-clickable for "Jump to file"; the two explicit
     * buttons own keyboard activation. The clickable body has no `tabindex`
     * so it isn't focusable — it would otherwise be a confusing keyboard
     * second copy of the Jump action.
     */
    import Size from '$lib/ui/Size.svelte'
    import ShortcutChip from '$lib/ui/ShortcutChip.svelte'
    import Button from '$lib/ui/Button.svelte'
    import { dismissToast } from '$lib/ui/toast'
    import { goToDownload } from './go-to-latest'
    import {
        setDownloadsNotificationsMode,
        openSettingsToDownloadsNotifications,
    } from './notifications-mode'
    import { DEFAULT_GLOBAL_GO_TO_LATEST_BINDING } from './global-shortcut-binding'
    import GlobalShortcutAnimation from './GlobalShortcutAnimation.svelte'
    import type { ExplorerAPI } from '../../routes/(main)/explorer-api'

    /**
     * Subset of the `download-detected` Tauri payload this toast actually
     * reads. The event-bridge passes a wider object; eslint's
     * `svelte/no-unused-props` flags any field not consumed in the template,
     * so the prop type lists only what's rendered.
     */
    interface DownloadDetectedPayload {
        parentDir: string
        fileName: string
        inSubdir: boolean
        sizeBytes: number | null
    }

    interface Props {
        /** Dedup id of this toast; lets the component self-dismiss on action. */
        toastId: string
        /**
         * Snapshot of the focused-explorer handle at toast-creation time.
         * Pass `undefined` outside the main window context; `goToDownload` is
         * a no-op without it.
         */
        explorer: ExplorerAPI | undefined
        /** The `download-detected` Tauri payload that produced this toast. */
        event: DownloadDetectedPayload
        /**
         * Display string for the in-app go-to-latest shortcut (default `⌘J`),
         * snapshotted at toast-add time. NOT reactive: a remap mid-toast does
         * not change what's shown. Pass `''` to omit the in-app hint line (the
         * command is unbound).
         */
        shortcutHint: string
        /**
         * Display string for the GLOBAL go-to-latest hotkey (default `⌃⌥⌘J`),
         * the one that jumps from any app, snapshotted at toast-add time. Pass
         * `''` to omit the whole global hint line — the bridge does that when
         * the hotkey is turned off or unbound, since there's nothing to teach.
         * When the value still equals the default binding we also play the
         * keyboard animation; a remapped combo keeps the chip but drops the
         * animation (its keys would no longer match).
         */
        globalBinding: string
    }

    const { toastId, explorer, event, shortcutHint, globalBinding }: Props = $props()

    /**
     * Only show the keyboard animation for the default combo. The SVG lights up
     * the literal ⌃⌥⌘J keys, so a remapped binding would teach the wrong keys —
     * we keep the text chip (it tracks the snapshot) but drop the animation.
     */
    const showShortcutAnimation = $derived(globalBinding === DEFAULT_GLOBAL_GO_TO_LATEST_BINDING)

    /**
     * Relative-subdir label rendered when the file is below the Downloads
     * root. Strip everything up to the trailing `Downloads/` segment so a
     * macOS firmlinked path like `/Users/me/Downloads/Chrome/...` reads as
     * "Downloads/Chrome/" not the absolute path. Falls back to the raw
     * parent dir if the segment isn't found — better some context than none.
     */
    const subdirLabel = $derived.by(() => {
        if (!event.inSubdir) return ''
        const marker = '/Downloads/'
        const i = event.parentDir.lastIndexOf(marker)
        if (i === -1) return event.parentDir
        return 'Downloads/' + event.parentDir.slice(i + marker.length) + '/'
    })

    async function handleJump() {
        await goToDownload(explorer, event.parentDir, event.fileName)
        dismissToast(toastId)
    }

    async function handleStopShowing(e: MouseEvent) {
        // Buttons run their own actions; the body-click jump must NOT also
        // fire (otherwise hitting "Stop showing these" would also navigate
        // to the file before the Settings window comes up).
        e.stopPropagation()
        setDownloadsNotificationsMode('neither')
        await openSettingsToDownloadsNotifications()
        dismissToast(toastId)
    }

    async function handleJumpButton(e: MouseEvent) {
        // Stop propagation so the body-click handler doesn't run Jump twice.
        // The button's own action is the only one we want firing.
        e.stopPropagation()
        await handleJump()
    }

    function handleBodyClick() {
        void handleJump()
    }
</script>

<!--
    The outer container is the click surface for "Jump to file". It is
    intentionally NOT focusable: keyboard users reach the two buttons
    independently, and a third focusable surface (the div) would be a
    confusing keyboard duplicate of the primary action.
-->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="toast-body" onclick={handleBodyClick}>
    <span class="title">
        Downloaded <code class="file">{event.fileName}</code>
        {#if event.sizeBytes != null}
            <span class="size"><Size bytes={event.sizeBytes} /></span>
        {/if}
    </span>
    {#if subdirLabel}
        <span class="subdir">in {subdirLabel}</span>
    {/if}
    {#if shortcutHint || globalBinding}
        <div class="learn">
            <strong class="learn-intro">Something cool to learn about jumping to downloads:</strong>
            {#if shortcutHint}
                <span class="hint">In-app: Press <ShortcutChip key={shortcutHint} /> to jump here</span>
            {/if}
            {#if globalBinding}
                <span class="hint">In <em>any</em> app (global shortcut), press <ShortcutChip key={globalBinding} /></span>
                {#if showShortcutAnimation}
                    <div class="shortcut-animation">
                        <GlobalShortcutAnimation />
                    </div>
                {/if}
            {/if}
        </div>
    {/if}
    <div class="actions">
        <Button size="mini" variant="secondary" onclick={handleStopShowing}>Stop showing these</Button>
        <Button size="mini" variant="primary" onclick={handleJumpButton}>Jump to file</Button>
    </div>
</div>

<style>
    .toast-body {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        font-size: var(--font-size-sm);
    }

    .title {
        color: var(--color-text-primary);
        line-height: 1.4;
    }

    .file {
        font-family: var(--font-mono);
        font-size: var(--font-size-sm);
        background: none;
        padding: 0;
        color: var(--color-text-primary);
    }

    .size {
        margin-left: var(--spacing-xs);
        font-size: var(--font-size-xs);
    }

    .subdir {
        color: var(--color-text-secondary);
        font-size: var(--font-size-xs);
    }

    /* The teaching block: a relaxed vertical rhythm between the intro line, the
       two shortcut hints, and the animation so it reads as a calm little lesson
       rather than cramped microcopy. */
    .learn {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
        margin-top: var(--spacing-xs);
    }

    .learn-intro {
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        font-weight: 600;
    }

    .hint {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-xs);
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xxs);
    }

    .hint em {
        font-style: italic;
        color: var(--color-text-secondary);
    }

    /* The wider toast (set via `widthPx` at dispatch) gives the keyboard SVG
       room to read clearly. It's still capped so it doesn't span edge-to-edge. */
    .shortcut-animation {
        max-width: 320px;
    }

    /* Right-aligned button row, primary at the far right per the macOS
       default-button-bottom-right convention. */
    .actions {
        display: flex;
        justify-content: flex-end;
        gap: var(--spacing-sm);
        margin-top: var(--spacing-md);
    }
</style>
