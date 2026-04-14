<script lang="ts">
    import { onMount, onDestroy, tick } from 'svelte'
    import ToastContainer from '$lib/ui/toast/ToastContainer.svelte'
    import DebugDriveIndexPanel from './DebugDriveIndexPanel.svelte'
    import DebugToastPanel from './DebugToastPanel.svelte'
    import DebugHistoryPanel from './DebugHistoryPanel.svelte'
    import DebugErrorPreviewPanel from './DebugErrorPreviewPanel.svelte'

    let pageElement: HTMLDivElement | undefined = $state()
    let isDarkMode = $state(true)

    onMount(async () => {
        // Hide the loading screen
        const loadingScreen = document.getElementById('loading-screen')
        if (loadingScreen) {
            loadingScreen.style.display = 'none'
        }

        // Focus the page so keyboard events work immediately
        void tick().then(() => {
            pageElement?.focus()
        })

        // Detect current system preference
        if (typeof window !== 'undefined') {
            isDarkMode = window.matchMedia('(prefers-color-scheme: dark)').matches
        }

        // Try to get current app theme setting
        try {
            const { getCurrentWindow } = await import('@tauri-apps/api/window')
            const theme = await getCurrentWindow().theme()
            if (theme) {
                isDarkMode = theme === 'dark'
            }
        } catch {
            // Not in Tauri environment or theme not set
        }
    })

    async function handleThemeToggle() {
        isDarkMode = !isDarkMode
        try {
            const { setTheme } = await import('@tauri-apps/api/app')
            await setTheme(isDarkMode ? 'dark' : 'light')
        } catch (error) {
            // eslint-disable-next-line no-console -- Debug window is dev-only
            console.error('Failed to set theme:', error)
        }
    }

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
            // Not in Tauri environment
        }
    }
</script>

<div
    bind:this={pageElement}
    class="debug-container"
    role="dialog"
    aria-label="Debug window"
    tabindex="-1"
    onkeydown={handleKeydown}
>
    <ToastContainer />
    <div class="debug-header">
        <h1>Debug</h1>
        <button class="close-button" onclick={closeWindow} aria-label="Close">&times;</button>
    </div>

    <div class="debug-content">
        <section class="debug-section">
            <h2>Appearance</h2>
            <label class="toggle-row">
                <span>Dark mode</span>
                <input type="checkbox" checked={isDarkMode} onchange={handleThemeToggle} class="toggle-checkbox" />
            </label>
        </section>

        <DebugDriveIndexPanel />
        <DebugToastPanel />
        <DebugHistoryPanel />
        <DebugErrorPreviewPanel />
    </div>
</div>

<style>
    /* stylelint-disable declaration-property-value-disallowed-list, declaration-property-value-allowed-list, color-no-hex -- Dev utility page */
    .debug-container {
        display: flex;
        flex-direction: column;
        height: 100vh;
        background: var(--color-bg-primary);
        color: var(--color-text-primary);
        font-family: var(--font-system), sans-serif;
        outline: none;
    }

    .debug-header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: 12px 16px;
        background: var(--color-bg-secondary);
        border-bottom: 1px solid var(--color-border-strong);
        /* Allow dragging the window from header */
        -webkit-app-region: drag;
    }

    .debug-header h1 {
        margin: 0;
        font-size: var(--font-size-md);
        font-weight: 600;
        color: var(--color-text-primary);
    }

    .close-button {
        -webkit-app-region: no-drag;
        background: none;
        border: none;
        color: var(--color-text-secondary);
        font-size: var(--font-size-xl);
        padding: 2px 8px;
        line-height: 1;
        border-radius: var(--radius-sm);
    }

    .close-button:hover {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }

    .debug-content {
        flex: 1;
        padding: 16px;
        overflow-y: auto;
    }

    .debug-section {
        margin-bottom: 24px;
    }

    .debug-section h2 {
        margin: 0 0 var(--spacing-md);
        font-size: var(--font-size-sm);
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.5px;
        color: var(--color-text-secondary);
    }

    .toggle-row {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: 8px 12px;
        background: var(--color-bg-secondary);
        border-radius: var(--radius-md);
    }

    .toggle-row:hover {
        background: var(--color-bg-tertiary);
    }

    .toggle-row span {
        font-size: var(--font-size-md);
        color: var(--color-text-primary);
    }

    .toggle-checkbox {
        width: 18px;
        height: 18px;
        accent-color: var(--color-accent);
    }

    /* History styles */
    :global(.history-panes) {
        display: flex;
        gap: 12px;
    }

    :global(.history-pane) {
        flex: 1;
        background: var(--color-bg-secondary);
        border-radius: var(--radius-md);
        padding: 8px;
        min-width: 0;
    }

    :global(.history-pane.focused) {
        outline: 2px solid var(--color-accent);
    }

    :global(.history-pane h3) {
        margin: 0 0 var(--spacing-sm);
        font-size: var(--font-size-sm);
        font-weight: 600;
        color: var(--color-text-secondary);
        text-transform: uppercase;
    }

    :global(.history-list) {
        list-style: none;
        margin: 0;
        padding: 0;
        font-size: var(--font-size-sm);
        font-family: var(--font-mono);
    }

    :global(.history-list li) {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        padding: 3px 4px;
        border-radius: var(--radius-sm);
        color: var(--color-text-secondary);
    }

    :global(.history-list li.current) {
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

    :global(.history-path) {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    :global(.no-history) {
        margin: 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        font-style: italic;
    }

    /* Drive index styles */
    :global(.index-panel) {
        background: var(--color-bg-secondary);
        border-radius: var(--radius-md);
        padding: 12px;
        display: flex;
        flex-direction: column;
        gap: 10px;
    }

    :global(.index-status-row) {
        display: flex;
        gap: 8px;
        align-items: center;
    }

    :global(.index-status) {
        font-size: var(--font-size-sm);
    }

    :global(.status-badge) {
        display: inline-flex;
        align-items: center;
        gap: 5px;
        padding: 2px 8px;
        border-radius: var(--radius-sm);
        font-size: var(--font-size-xs);
        font-weight: 600;
    }

    :global(.status-badge.active) {
        background: color-mix(in srgb, var(--color-accent) 20%, transparent);
        color: var(--color-accent);
    }

    :global(.status-badge.ready) {
        background: color-mix(in srgb, #4caf50 20%, transparent);
        color: #4caf50;
    }

    :global(.status-badge.neutral) {
        background: var(--color-bg-tertiary);
        color: var(--color-text-tertiary);
    }

    :global(.phase-duration) {
        font-weight: 400;
        margin-left: 4px;
        font-family: var(--font-mono);
    }

    :global(.phase-live-stat) {
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
        margin-left: 8px;
    }

    /* Phase timeline */
    :global(.phase-timeline) {
        max-height: 160px;
        overflow-y: auto;
        background: var(--color-bg-primary);
        border-radius: var(--radius-sm);
        padding: 6px;
        font-size: var(--font-size-xs);
        font-family: var(--font-mono);
    }

    :global(.phase-timeline-row) {
        display: flex;
        gap: 10px;
        padding: 2px 0;
        line-height: 1.4;
        color: var(--color-text-tertiary);
    }

    :global(.phase-timeline-row.phase-current) {
        color: var(--color-text-primary);
        font-weight: 600;
    }

    :global(.phase-time) {
        flex-shrink: 0;
        width: 60px;
    }

    :global(.phase-name) {
        flex-shrink: 0;
        width: 85px;
    }

    :global(.phase-dur) {
        flex-shrink: 0;
        width: 70px;
        text-align: right;
    }

    :global(.phase-stats) {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        color: var(--color-text-secondary);
    }

    :global(.phase-current .phase-stats) {
        color: var(--color-text-primary);
    }

    :global(.phase-now-marker) {
        color: var(--color-accent);
        font-weight: 600;
    }

    :global(.phase-now-marker::before) {
        content: '\2190 ';
    }

    :global(.index-actions) {
        display: flex;
        align-items: center;
        gap: 8px;
    }

    :global(.index-button) {
        padding: 4px 12px;
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
        gap: 8px;
    }

    :global(.index-meta-label) {
        color: var(--color-text-tertiary);
        min-width: 120px;
    }

    :global(.index-meta-value) {
        color: var(--color-text-primary);
        font-family: var(--font-mono);
    }

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

    :global(.db-breakdown) {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-xs);
        margin-left: 4px;
    }

    /* Toast debug styles */
    :global(.toast-debug-panel) {
        background: var(--color-bg-secondary);
        border-radius: var(--radius-md);
        padding: 12px;
        display: flex;
        flex-direction: column;
        gap: 8px;
    }

    :global(.toast-debug-row) {
        display: flex;
        align-items: center;
        gap: 8px;
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

    /* Error pane preview styles */
    :global(.error-preview-panel) {
        background: var(--color-bg-secondary);
        border-radius: var(--radius-md);
        padding: 12px;
        display: flex;
        flex-direction: column;
        gap: 4px;
    }

    :global(.error-preview-actions) {
        display: flex;
        gap: 8px;
        margin-bottom: 4px;
    }

    :global(.error-group-header) {
        font-size: var(--font-size-xs);
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.5px;
        color: var(--color-text-tertiary);
        margin-top: 8px;
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
