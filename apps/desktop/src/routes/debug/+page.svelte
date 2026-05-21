<script lang="ts">
    import { onDestroy, onMount, tick } from 'svelte'
    import ToastContainer from '$lib/ui/toast/ToastContainer.svelte'
    import { trackOwnRect } from '$lib/window-positioning'
    import DebugAppearancePanel from './DebugAppearancePanel.svelte'
    import DebugClosedTabsPanel from './DebugClosedTabsPanel.svelte'
    import DebugDriveIndexPanel from './DebugDriveIndexPanel.svelte'
    import DebugErrorPreviewPanel from './DebugErrorPreviewPanel.svelte'
    import DebugHistoryPanel from './DebugHistoryPanel.svelte'
    import DebugSmbDiagnosticsPanel from './DebugSmbDiagnosticsPanel.svelte'
    import DebugToastPanel from './DebugToastPanel.svelte'

    type SectionId =
        | 'appearance'
        | 'drive-index'
        | 'smb-diagnostics'
        | 'toast-notifications'
        | 'navigation-history'
        | 'closed-tabs'
        | 'error-preview'

    /** Sidebar order. First entry is the default selection. */
    const SECTIONS: { id: SectionId; label: string }[] = [
        { id: 'appearance', label: 'Appearance' },
        { id: 'drive-index', label: 'Drive index' },
        { id: 'smb-diagnostics', label: 'SMB diagnostics' },
        { id: 'toast-notifications', label: 'Toast notifications' },
        { id: 'navigation-history', label: 'Navigation history' },
        { id: 'closed-tabs', label: 'Closed tabs' },
        { id: 'error-preview', label: 'Error pane preview' },
    ]

    let pageElement: HTMLElement | undefined = $state()
    let selected: SectionId = $state('appearance')
    let unlistenRectTracking: (() => void) | undefined

    onMount(async () => {
        const loadingScreen = document.getElementById('loading-screen')
        if (loadingScreen) loadingScreen.style.display = 'none'
        void tick().then(() => pageElement?.focus())

        // Save position/size while open so reopening lands in the same spot
        // (in-memory cache, reset on app start).
        unlistenRectTracking = await trackOwnRect('debug')
    })

    onDestroy(() => {
        unlistenRectTracking?.()
    })

    function handleKeydown(event: KeyboardEvent) {
        if (event.key === 'Escape') {
            void closeWindow()
        }
    }

    async function closeWindow() {
        try {
            const { getCurrentWindow } = await import('@tauri-apps/api/window')
            await getCurrentWindow().close()
        } catch {
            // Not in Tauri
        }
    }
</script>

<svelte:window onkeydown={handleKeydown} />

<main bind:this={pageElement} class="debug-window" tabindex="-1">
    <h1 class="sr-only">Debug</h1>
    <!-- Drag region for the top strip of the window (matches Settings). The
         traffic-light buttons sit on top as NSWindow chrome and stay clickable;
         this invisible band lets the user grab the rest of the title-bar zone. -->
    <div class="window-drag-region" data-tauri-drag-region aria-hidden="true"></div>

    <ToastContainer />

    <div class="debug-layout">
        <aside class="debug-sidebar">
            <div class="debug-sidebar-title">Debug</div>
            <nav class="debug-section-list" aria-label="Debug sections">
                {#each SECTIONS as section (section.id)}
                    <button
                        type="button"
                        class="debug-section-item"
                        class:selected={selected === section.id}
                        onclick={() => (selected = section.id)}
                        aria-current={selected === section.id ? 'page' : undefined}
                    >
                        {section.label}
                    </button>
                {/each}
            </nav>
        </aside>

        <div class="debug-content-wrapper">
            {#if selected === 'appearance'}
                <DebugAppearancePanel />
            {:else if selected === 'drive-index'}
                <DebugDriveIndexPanel />
            {:else if selected === 'smb-diagnostics'}
                <DebugSmbDiagnosticsPanel />
            {:else if selected === 'toast-notifications'}
                <DebugToastPanel />
            {:else if selected === 'navigation-history'}
                <DebugHistoryPanel />
            {:else if selected === 'closed-tabs'}
                <DebugClosedTabsPanel />
            {:else if selected === 'error-preview'}
                <DebugErrorPreviewPanel />
            {/if}
        </div>
    </div>
</main>

<style>
    /* stylelint-disable declaration-property-value-disallowed-list, declaration-property-value-allowed-list, color-no-hex -- Dev utility window */

    .debug-window {
        width: 100%;
        height: 100vh;
        /* Mirror Settings: translucent backdrop sitting on top of the
           NSVisualEffectView Sidebar material applied via setEffects() in
           `lib/debug/debug-window.ts`. Token is the settings-only one
           because both windows want the same glass look. */
        background: var(--color-bg-settings-primary);
        color: var(--color-text-primary);
        font-family: var(--font-system), sans-serif;
        font-size: var(--font-size-sm);
        overflow: hidden;
        display: flex;
        flex-direction: column;
        position: relative;
        /* Match the OS window corner radius (29 px) so the webview clip
           lines up with the NSWindow's rounded corners. */
        border-radius: var(--radius-xxl);
        outline: none;
    }

    .window-drag-region {
        position: absolute;
        top: 0;
        left: 0;
        right: 0;
        height: 50px;
        z-index: var(--z-dropdown);
    }

    .debug-layout {
        display: flex;
        flex: 1;
        overflow: hidden;
        padding: var(--spacing-sm);
    }

    .debug-sidebar {
        width: 200px;
        min-width: 200px;
        display: flex;
        flex-direction: column;
        background: linear-gradient(135deg, var(--color-bg-sidebar-from), var(--color-bg-sidebar-to));
        border-radius: var(--radius-xl);
        border: 1px solid var(--color-sidebar-border);
        box-shadow: var(--shadow-sidebar);
        /* Clears the traffic-light row (lights land at sidebar-local y ≈ 22 px). */
        padding-top: calc(var(--spacing-xl) + var(--spacing-md));
    }

    .debug-sidebar-title {
        padding: var(--spacing-xs) var(--spacing-md) var(--spacing-sm);
        font-size: var(--font-size-xs);
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.5px;
        color: var(--color-text-tertiary);
    }

    .debug-section-list {
        flex: 1;
        overflow-y: auto;
        padding: var(--spacing-xs);
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
    }

    .debug-section-item {
        display: block;
        width: 100%;
        padding: var(--spacing-xs) var(--spacing-sm);
        background: none;
        border: none;
        text-align: left;
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        font-family: inherit;
        border-radius: var(--radius-sm);
        cursor: default;
    }

    .debug-section-item:hover {
        background: var(--color-bg-tertiary);
    }

    .debug-section-item.selected {
        background: var(--color-accent);
        color: var(--color-accent-fg);
    }

    .debug-section-item.selected:hover {
        background: var(--color-accent-hover);
    }

    .debug-content-wrapper {
        flex: 1;
        overflow-y: auto;
        padding: var(--spacing-lg);
        outline: none;
        min-width: 0;
    }

    /* ── Shared section styles (used by every Debug*Panel.svelte) ────────
       Kept here so each panel can stay focused on its own content; lifting
       these out of the previous monolithic +page.svelte preserves the
       visual look the panels were built against. */

    :global(.debug-section) {
        margin-bottom: var(--spacing-2xl);
    }

    :global(.debug-section:last-child) {
        margin-bottom: 0;
    }

    :global(.debug-section h2) {
        margin: 0 0 var(--spacing-md);
        font-size: var(--font-size-sm);
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.5px;
        color: var(--color-text-secondary);
    }

    :global(.toggle-row) {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: var(--spacing-sm) var(--spacing-md);
        background: var(--color-bg-secondary);
        border-radius: var(--radius-md);
    }

    :global(.toggle-row:hover) {
        background: var(--color-bg-tertiary);
    }

    :global(.toggle-row span) {
        font-size: var(--font-size-md);
        color: var(--color-text-primary);
    }

    :global(.toggle-checkbox) {
        width: 18px;
        height: 18px;
        accent-color: var(--color-accent);
    }

    /* ── Drive-index panel (DebugDriveIndexPanel.svelte) ─────────────── */

    :global(.index-panel) {
        background: var(--color-bg-secondary);
        border-radius: var(--radius-md);
        padding: var(--spacing-md);
        display: flex;
        flex-direction: column;
        gap: 10px;
    }

    :global(.index-status-row) {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        font-size: var(--font-size-sm);
        flex-wrap: wrap;
    }

    :global(.status-badge) {
        display: inline-flex;
        align-items: center;
        padding: 2px var(--spacing-sm);
        border-radius: var(--radius-sm);
        font-size: var(--font-size-xs);
        font-weight: 600;
        color: var(--color-text-primary);
    }

    :global(.status-idle) {
        background: var(--color-bg-tertiary);
        color: var(--color-text-tertiary);
    }

    :global(.status-active) {
        background: var(--color-accent);
        color: var(--color-accent-fg);
    }

    :global(.status-live) {
        background: var(--color-accent);
        color: var(--color-accent-fg);
    }

    :global(.status-watcher) {
        background: var(--color-bg-tertiary);
        color: var(--color-text-secondary);
        border: 1px solid var(--color-border);
    }

    :global(.phase-timeline) {
        font-family: var(--font-mono);
        font-size: var(--font-size-xs);
        background: var(--color-bg-tertiary);
        border-radius: var(--radius-sm);
        padding: var(--spacing-sm);
        max-height: 240px;
        overflow-y: auto;
    }

    :global(.phase-header) {
        font-size: var(--font-size-xs);
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.5px;
        color: var(--color-text-tertiary);
        margin-bottom: var(--spacing-xs);
    }

    :global(.phase-timeline-row) {
        display: grid;
        grid-template-columns: 90px 110px 80px 1fr;
        gap: var(--spacing-sm);
        padding: 2px 0;
        align-items: baseline;
    }

    :global(.phase-timeline-row.phase-current) {
        font-weight: 600;
        color: var(--color-text-primary);
    }

    :global(.phase-time) {
        color: var(--color-text-tertiary);
    }

    :global(.phase-name) {
        color: var(--color-text-secondary);
    }

    :global(.phase-dur) {
        color: var(--color-text-secondary);
        text-align: right;
    }

    :global(.phase-stats) {
        color: var(--color-text-tertiary);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    :global(.phase-now-marker) {
        color: var(--color-accent);
    }

    :global(.no-history) {
        margin: 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        font-style: italic;
    }

    :global(.index-actions) {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
    }

    :global(.index-button) {
        padding: 4px var(--spacing-md);
        font-size: var(--font-size-sm);
        font-family: var(--font-system), sans-serif;
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
    }

    :global(.index-button:hover) {
        background: var(--color-bg-primary);
    }

    :global(.index-message) {
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    :global(.index-sub-header) {
        font-size: var(--font-size-xs);
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.5px;
        color: var(--color-text-tertiary);
        margin-top: 4px;
    }

    :global(.index-meta) {
        display: flex;
        flex-direction: column;
        gap: 3px;
        font-size: var(--font-size-sm);
    }

    :global(.index-meta-row) {
        display: flex;
        gap: var(--spacing-sm);
    }

    :global(.index-meta-label) {
        color: var(--color-text-tertiary);
        min-width: 120px;
    }

    :global(.index-meta-value) {
        color: var(--color-text-primary);
        font-family: var(--font-mono);
    }

    :global(.db-breakdown) {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-xs);
        margin-left: 4px;
    }

    /* ── Info icon (used by drive-index + SMB diagnostics) ──────────── */

    :global(.info-icon) {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 14px;
        height: 14px;
        font-size: var(--font-size-xs);
        font-weight: 600;
        font-style: italic;
        font-family: var(--font-system), sans-serif;
        border-radius: 50%;
        background: var(--color-bg-tertiary);
        color: var(--color-text-tertiary);
        cursor: help;
        vertical-align: middle;
        margin-left: 2px;
    }

    :global(.info-icon:hover) {
        background: var(--color-bg-primary);
        color: var(--color-text-secondary);
    }

    /* ── History + closed-tabs panels ────────────────────────────────── */

    :global(.history-panes),
    :global(.closed-tabs-panes) {
        display: flex;
        gap: var(--spacing-md);
    }

    :global(.history-pane),
    :global(.closed-tabs-pane) {
        flex: 1;
        background: var(--color-bg-secondary);
        border-radius: var(--radius-md);
        padding: var(--spacing-sm);
        min-width: 0;
    }

    :global(.history-pane.focused),
    :global(.closed-tabs-pane.focused) {
        outline: 2px solid var(--color-accent);
    }

    :global(.history-pane h3),
    :global(.closed-tabs-pane h3) {
        margin: 0 0 var(--spacing-sm);
        font-size: var(--font-size-sm);
        font-weight: 600;
        color: var(--color-text-secondary);
        text-transform: uppercase;
    }

    :global(.history-list),
    :global(.closed-tabs-list) {
        list-style: none;
        margin: 0;
        padding: 0;
        font-size: var(--font-size-sm);
        font-family: var(--font-mono);
    }

    :global(.history-list li),
    :global(.closed-tabs-list li) {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        padding: 3px 4px;
        border-radius: var(--radius-sm);
        color: var(--color-text-secondary);
        min-width: 0;
    }

    :global(.history-list li.current),
    :global(.closed-tabs-list li.top) {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
        font-weight: 600;
    }

    :global(.history-list li.future) {
        opacity: 0.5;
    }

    :global(.history-index) {
        flex-shrink: 0;
        width: 12px;
        text-align: center;
    }

    :global(.history-path),
    :global(.closed-tabs-path) {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    /* ── Toast debug panel ───────────────────────────────────────────── */

    :global(.toast-debug-panel) {
        background: var(--color-bg-secondary);
        border-radius: var(--radius-md);
        padding: var(--spacing-md);
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
    }

    :global(.toast-debug-row) {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
    }

    :global(.toast-debug-label) {
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        min-width: 110px;
    }

    :global(.toast-debug-count) {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        font-family: var(--font-mono);
    }

    /* ── Error preview panel ─────────────────────────────────────────── */

    :global(.error-preview-panel) {
        background: var(--color-bg-secondary);
        border-radius: var(--radius-md);
        padding: var(--spacing-md);
        display: flex;
        flex-direction: column;
        gap: 4px;
    }

    :global(.error-preview-actions) {
        display: flex;
        gap: var(--spacing-sm);
        margin-bottom: 4px;
    }

    :global(.error-group-header) {
        font-size: var(--font-size-xs);
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.5px;
        color: var(--color-text-tertiary);
        margin-top: var(--spacing-sm);
    }

    :global(.error-group-header:first-of-type) {
        margin-top: 0;
    }

    :global(.error-row) {
        display: flex;
        align-items: center;
        gap: 6px;
        padding: 2px 0;
        font-size: var(--font-size-xs);
    }

    :global(.error-label) {
        flex: 1;
        min-width: 0;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        font-family: var(--font-mono);
        color: var(--color-text-primary);
    }

    :global(.error-title) {
        color: var(--color-text-tertiary);
        margin-left: 4px;
        font-family: var(--font-system), sans-serif;
    }

    :global(.error-provider-select) {
        flex-shrink: 0;
        width: 110px;
        padding: 1px 4px;
        font-size: var(--font-size-xs);
        font-family: var(--font-system), sans-serif;
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
    }

    :global(.error-trigger-btn) {
        flex-shrink: 0;
        width: 24px;
        height: 22px;
        padding: 0;
        font-size: var(--font-size-xs);
        font-weight: 600;
        font-family: var(--font-system), sans-serif;
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        display: inline-flex;
        align-items: center;
        justify-content: center;
    }

    :global(.error-trigger-btn:hover) {
        background: var(--color-bg-primary);
    }
</style>
