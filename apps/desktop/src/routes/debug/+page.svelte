<script lang="ts">
    import { onDestroy, onMount, tick } from 'svelte'
    import ToastContainer from '$lib/ui/toast/ToastContainer.svelte'
    import { trackOwnRect } from '$lib/window-positioning'
    import { initializeSettings } from '$lib/settings'
    import { initAccentColor, cleanupAccentColor } from '$lib/accent-color'
    import { initReduceTransparency, cleanupReduceTransparency } from '$lib/reduce-transparency'
    import { getAppLogger } from '$lib/logging/logger'
    import DebugAppearancePanel from './DebugAppearancePanel.svelte'
    import DebugClosedTabsPanel from './DebugClosedTabsPanel.svelte'
    import DebugDialogsPanel from './DebugDialogsPanel.svelte'
    import DebugDriveIndexPanel from './DebugDriveIndexPanel.svelte'
    import DebugErrorPreviewPanel from './DebugErrorPreviewPanel.svelte'
    import DebugHistoryPanel from './DebugHistoryPanel.svelte'
    import DebugOperationLogPanel from './DebugOperationLogPanel.svelte'
    import DebugSmbDiagnosticsPanel from './DebugSmbDiagnosticsPanel.svelte'
    import DebugToastPanel from './DebugToastPanel.svelte'
    import ComponentsCatalog from '../dev/components/+page.svelte'
    import GraphicsCatalog from '../dev/graphics/+page.svelte'

    /** Section ids. The `components-*` ids are children of the `'components'` parent. */
    type SectionId =
        | 'appearance'
        | 'drive-index'
        | 'smb-diagnostics'
        | 'toast-notifications'
        | 'operation-log'
        | 'navigation-history'
        | 'closed-tabs'
        | 'error-preview'
        // "Soft dialogs", NOT "Dialogs": `components-dialogs` below is already
        // labelled that (the ModalDialog primitive catalog), and two identical
        // sidebar labels in an instrument about UI quality is a bug.
        | 'soft-dialogs'
        | 'components'
        | 'components-buttons'
        | 'components-links'
        | 'components-groups'
        | 'components-toggle-group'
        | 'components-checkbox'
        | 'components-radio-group'
        | 'components-select'
        | 'components-combobox'
        | 'components-popover'
        | 'components-filter-popover'
        | 'components-chip'
        | 'components-dialogs'
        | 'components-toasts'
        | 'components-progress'
        | 'components-loading'
        | 'components-tooltips'
        | 'components-size-badges'
        | 'components-status-badge'
        | 'components-date-label'
        | 'components-shortcut-chip'
        | 'components-commandbox'
        | 'components-empty-states'
        | 'graphics'
        | 'graphics-icons'
        | 'graphics-spinners'
        | 'graphics-status-badges'
        | 'graphics-illustrations'
        | 'graphics-animations'
        | 'graphics-drive-indexing'

    interface Section {
        id: SectionId
        label: string
        children?: { id: SectionId; label: string }[]
    }

    /** Sidebar order. First entry is the default selection. */
    const SECTIONS: Section[] = [
        { id: 'appearance', label: 'Appearance' },
        { id: 'drive-index', label: 'Drive index' },
        { id: 'smb-diagnostics', label: 'SMB diagnostics' },
        { id: 'toast-notifications', label: 'Toast notifications' },
        { id: 'operation-log', label: 'Operation log' },
        { id: 'navigation-history', label: 'Navigation history' },
        { id: 'closed-tabs', label: 'Closed tabs' },
        { id: 'error-preview', label: 'Error pane preview' },
        { id: 'soft-dialogs', label: 'Soft dialogs' },
        {
            id: 'components',
            label: 'Components',
            children: [
                { id: 'components-buttons', label: 'Buttons' },
                { id: 'components-links', label: 'Links' },
                { id: 'components-groups', label: 'Groups' },
                { id: 'components-toggle-group', label: 'Toggle group' },
                { id: 'components-checkbox', label: 'Checkbox' },
                { id: 'components-radio-group', label: 'Radio group' },
                { id: 'components-select', label: 'Select' },
                { id: 'components-combobox', label: 'Combobox' },
                { id: 'components-popover', label: 'Popover' },
                { id: 'components-filter-popover', label: 'Filter popover' },
                { id: 'components-chip', label: 'Chip' },
                { id: 'components-dialogs', label: 'Dialogs' },
                { id: 'components-toasts', label: 'Toasts' },
                { id: 'components-progress', label: 'Progress' },
                { id: 'components-loading', label: 'Loading' },
                { id: 'components-tooltips', label: 'Tooltips' },
                { id: 'components-size-badges', label: 'Size badges' },
                { id: 'components-status-badge', label: 'Status badge' },
                { id: 'components-date-label', label: 'Date label' },
                { id: 'components-shortcut-chip', label: 'Shortcut chip' },
                { id: 'components-commandbox', label: 'CommandBox' },
                { id: 'components-empty-states', label: 'Empty states' },
            ],
        },
        {
            id: 'graphics',
            label: 'Graphics',
            children: [
                { id: 'graphics-icons', label: 'Icons' },
                { id: 'graphics-spinners', label: 'Spinners' },
                { id: 'graphics-status-badges', label: 'Status badges' },
                { id: 'graphics-illustrations', label: 'Illustrations' },
                { id: 'graphics-animations', label: 'Animations' },
                { id: 'graphics-drive-indexing', label: 'Drive indexing status' },
            ],
        },
    ]

    const log = getAppLogger('debug')

    let pageElement: HTMLElement | undefined = $state()
    let selected: SectionId = $state('appearance')
    let unlistenRectTracking: (() => void) | undefined

    /** Sub-anchor for the catalog page (the bit after `components-`), or null for top of catalog. */
    const catalogAnchor = $derived.by((): string | null => {
        if (selected === 'components') return null
        if (selected.startsWith('components-')) return selected.slice('components-'.length)
        return null
    })

    const isComponentsView = $derived.by(
        () => selected === 'components' || selected.startsWith('components-'),
    )

    function handleSectionInView(subId: string | null) {
        const target: SectionId = subId === null ? 'components' : (`components-${subId}` as SectionId)
        if (selected !== target) selected = target
    }

    /** Sub-anchor for the graphics catalog (the bit after `graphics-`), or null for top of catalog. */
    const graphicsAnchor = $derived.by((): string | null => {
        if (selected === 'graphics') return null
        if (selected.startsWith('graphics-')) return selected.slice('graphics-'.length)
        return null
    })

    const isGraphicsView = $derived.by(
        () => selected === 'graphics' || selected.startsWith('graphics-'),
    )

    function handleGraphicsSectionInView(subId: string | null) {
        const target: SectionId = subId === null ? 'graphics' : (`graphics-${subId}` as SectionId)
        if (selected !== target) selected = target
    }

    onMount(async () => {
        const loadingScreen = document.getElementById('loading-screen')
        if (loadingScreen) loadingScreen.style.display = 'none'
        void tick().then(() => pageElement?.focus())

        try {
            // Load settings before the accent init reads `appearance.appColor`, so the
            // window follows the app-wide accent (system color or Cmdr gold) and tracks
            // it live, instead of resting on the Cmdr-gold CSS fallback. Light/dark/system
            // mode already applies app-wide via Tauri's `setTheme`, so it needs no work here.
            await initializeSettings()
            await initAccentColor()
            await initReduceTransparency()
        } catch (error) {
            log.error('Failed to initialize debug window appearance: {error}', { error })
        }

        // Save position/size while open so reopening lands in the same spot
        // (in-memory cache, reset on app start).
        unlistenRectTracking = await trackOwnRect('debug')
    })

    onDestroy(() => {
        unlistenRectTracking?.()
        cleanupAccentColor()
        cleanupReduceTransparency()
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
                    {#if section.children}
                        {#each section.children as child (child.id)}
                            <button
                                type="button"
                                class="debug-section-item debug-section-child"
                                class:selected={selected === child.id}
                                onclick={() => (selected = child.id)}
                                aria-current={selected === child.id ? 'page' : undefined}
                            >
                                {child.label}
                            </button>
                        {/each}
                    {/if}
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
            {:else if selected === 'operation-log'}
                <DebugOperationLogPanel />
            {:else if selected === 'navigation-history'}
                <DebugHistoryPanel />
            {:else if selected === 'closed-tabs'}
                <DebugClosedTabsPanel />
            {:else if selected === 'error-preview'}
                <DebugErrorPreviewPanel />
            {:else if selected === 'soft-dialogs'}
                <DebugDialogsPanel />
            {:else if isComponentsView}
                <ComponentsCatalog targetAnchor={catalogAnchor} onSectionInView={handleSectionInView} />
            {:else if isGraphicsView}
                <GraphicsCatalog targetAnchor={graphicsAnchor} onSectionInView={handleGraphicsSectionInView} />
            {/if}
        </div>
    </div>
</main>

<style>
    /* stylelint-disable declaration-property-value-disallowed-list -- Dev utility window */

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

    .debug-section-child {
        padding-left: var(--spacing-lg);
        color: var(--color-text-secondary);
    }

    .debug-section-child.selected {
        color: var(--color-accent-fg);
    }

    .debug-content-wrapper {
        flex: 1;
        overflow-y: auto;
        padding: var(--spacing-lg);
        outline: none;
        min-width: 0;
    }

    /* ── Shared styles used by more than one Debug*Panel.svelte child ────
       These stay here (as :global) because several panels — or the parent
       layout — rely on them. Panel-exclusive rules live in each panel's own
       scoped \3c style>; only genuinely shared selectors remain global here. */

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

    /* Small action button + inline status message shared across panels:
       `.index-button` by drive-index, operation-log, toast, error-preview, and
       SMB diagnostics; `.index-message` by drive-index and operation-log. */
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

    /* Empty-state line shared by the drive-index, navigation-history, and
       operation-log panels. */
    :global(.no-history) {
        margin: 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        font-style: italic;
    }

    /* Status pill. Only the drive-index panel renders it today, but it's kept
       central per the shared-classes convention. Variants come from `phaseStyle`
       ('active' | 'ready' | 'neutral'), plus a literal 'ready' / 'neutral' on
       the watcher badge. */
    :global(.status-badge) {
        display: inline-flex;
        align-items: center;
        padding: 2px var(--spacing-sm);
        border-radius: var(--radius-sm);
        font-size: var(--font-size-xs);
        font-weight: 600;
        color: var(--color-text-primary);
    }

    :global(.status-badge.active) {
        background: var(--color-accent);
        color: var(--color-accent-fg);
    }

    :global(.status-badge.ready) {
        background: var(--color-accent-subtle);
        color: var(--color-text-primary);
    }

    :global(.status-badge.neutral) {
        background: var(--color-bg-tertiary);
        color: var(--color-text-tertiary);
    }

    /* Info icon, shared by the drive-index and SMB diagnostics panels. */
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
</style>
