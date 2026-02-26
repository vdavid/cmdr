<script lang="ts">
    import type { TabState, TabId } from './tab-types'

    interface Props {
        tabs: TabState[]
        activeTabId: TabId
        paneId: 'left' | 'right'
        maxTabs: number
        onTabSwitch: (tabId: TabId) => void
        onTabClose: (tabId: TabId) => void
        onTabMiddleClick: (tabId: TabId) => void
        onNewTab: () => void
        onContextMenu: (tabId: TabId, event: MouseEvent) => void
        onPaneFocus: () => void
    }

    const {
        tabs,
        activeTabId,
        paneId,
        maxTabs,
        onTabSwitch,
        onTabClose,
        onTabMiddleClick,
        onNewTab,
        onContextMenu,
        onPaneFocus,
    }: Props = $props()

    const isSingleTab = $derived(tabs.length === 1)
    const isAtMax = $derived(tabs.length >= maxTabs)

    /** Extracts the last path segment as a display name */
    function getFolderName(path: string): string {
        const segments = path.split('/')
        const last = segments[segments.length - 1]
        return last || path
    }

    function handleTabMouseDown(event: MouseEvent, tabId: TabId) {
        // Middle click
        if (event.button === 1) {
            event.preventDefault()
            onTabMiddleClick(tabId)
        }
    }

    function handleTabClick(event: MouseEvent, tabId: TabId) {
        // Only respond to primary click
        if (event.button !== 0) return
        if (tabId !== activeTabId) {
            onTabSwitch(tabId)
        }
    }

    function handleCloseClick(event: MouseEvent, tabId: TabId) {
        event.stopPropagation()
        onTabClose(tabId)
    }

    function handleContextMenu(event: MouseEvent, tabId: TabId) {
        event.preventDefault()
        onContextMenu(tabId, event)
    }
</script>

<div class="tab-bar" role="tablist" aria-label="{paneId} pane tabs" onclick={onPaneFocus}>
    <div class="tab-list">
        {#each tabs as tab, index (tab.id)}
            {@const isActive = tab.id === activeTabId}
            {@const isBeforeActive = index < tabs.length - 1 && tabs[index + 1].id === activeTabId}
            {@const isAfterActive = index > 0 && tabs[index - 1].id === activeTabId}
            <button
                class="tab"
                class:active={isActive}
                class:pinned={tab.pinned}
                class:before-active={isBeforeActive}
                class:after-active={isAfterActive}
                role="tab"
                aria-selected={isActive}
                title={tab.path}
                onmousedown={(e: MouseEvent) => {
                    handleTabMouseDown(e, tab.id)
                }}
                onclick={(e: MouseEvent) => {
                    handleTabClick(e, tab.id)
                }}
                oncontextmenu={(e: MouseEvent) => {
                    handleContextMenu(e, tab.id)
                }}
            >
                {#if tab.pinned}
                    <span class="pin-icon" title="Pinned" aria-label="Pinned">
                        <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
                            <path
                                d="M11 7V5a3 3 0 1 0-6 0v2H4a1 1 0 0 0-1 1v5a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1V8a1 1 0 0 0-1-1h-1zM6 5a2 2 0 1 1 4 0v2H6V5z"
                            />
                        </svg>
                    </span>
                {/if}
                <span class="tab-label">
                    {getFolderName(tab.path)}
                </span>
                {#if !isSingleTab}
                    <span
                        class="close-btn"
                        role="button"
                        tabindex="-1"
                        title="Close tab"
                        aria-label="Close tab"
                        onclick={(e: MouseEvent) => {
                            handleCloseClick(e, tab.id)
                        }}>&#xd7;</span
                    >
                {/if}
            </button>
        {/each}
    </div>
    <button
        class="new-tab-btn"
        aria-label="New tab"
        title="New tab (⌘T)"
        disabled={isAtMax}
        class:disabled={isAtMax}
        onclick={onNewTab}
    >
        &#x2b;
    </button>
</div>

<style>
    .tab-bar {
        display: flex;
        align-items: end;
        height: var(--spacing-tab-bar-height);
        min-height: var(--spacing-tab-bar-height);
        max-height: var(--spacing-tab-bar-height);
        background-color: var(--color-bg-secondary);
        border-bottom: 1px solid var(--color-border);
        padding: 0 var(--spacing-xxs);
        overflow: hidden;
    }

    .tab-list {
        display: flex;
        flex: 1;
        min-width: 0;
        align-items: end;
        overflow: hidden;
        gap: 1px;
    }

    .tab {
        position: relative;
        display: flex;
        align-items: center;
        gap: var(--spacing-xxs);
        min-width: 32px;
        max-width: 180px;
        flex: 1 1 0;
        height: 24px;
        padding: 0 var(--spacing-sm);
        border: none;
        border-radius: var(--radius-sm) var(--radius-sm) 0 0;
        background-color: transparent;
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
        font-family: var(--font-system);
        cursor: default;
        overflow: hidden;
        white-space: nowrap;
        container-type: inline-size;
        transition:
            background-color var(--transition-fast),
            color var(--transition-fast);
    }

    /* Subtle separator between inactive tabs — hidden next to the active tab */
    .tab:not(.active, .before-active, .after-active, :first-child)::before {
        content: '';
        position: absolute;
        left: -1px;
        top: 5px;
        bottom: 5px;
        width: 1px;
        background-color: var(--color-border-subtle);
    }

    .tab.active {
        background-color: color-mix(in oklch, var(--color-bg-primary), var(--color-accent) 4%);
        color: var(--color-text-primary);
        font-weight: 500;
        /* Extend down 1px to cover the tab-bar bottom border */
        height: 25px;
        margin-bottom: -1px;
        z-index: 1;
        box-shadow: 0 0 4px rgba(0, 0, 0, 0.04);
    }

    /* Accent top border on active tab */
    .tab.active::after {
        content: '';
        position: absolute;
        top: 0;
        left: 0;
        right: 0;
        height: 2px;
        background-color: var(--color-accent);
        border-radius: 1px 1px 0 0;
    }

    @media (prefers-color-scheme: dark) {
        .tab.active {
            background-color: color-mix(in oklch, var(--color-bg-primary), var(--color-accent) 7%);
            box-shadow: 0 0 4px rgba(0, 0, 0, 0.15);
        }
    }

    .tab:hover:not(.active) {
        background-color: color-mix(in srgb, var(--color-bg-tertiary), transparent 40%);
        color: var(--color-text-secondary);
    }

    /* Hide separator when hovering an inactive tab */
    .tab:hover:not(.active)::before {
        background-color: transparent;
    }

    .tab-label {
        flex: 1;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        text-align: center;
    }

    .tab.pinned {
        padding-left: 24px;
    }

    .pin-icon {
        position: absolute;
        left: 5px;
        top: 50%;
        transform: translateY(-50%);
        display: flex;
        align-items: center;
        color: var(--color-text-tertiary);
        opacity: 0.6;
        line-height: 0;
    }

    .close-btn {
        flex-shrink: 0;
        display: flex;
        align-items: center;
        justify-content: center;
        width: 14px;
        height: 14px;
        border-radius: var(--radius-full);
        font-size: var(--font-size-sm);
        line-height: 1;
        color: var(--color-text-tertiary);
        opacity: 0;
        transition: opacity var(--transition-fast);
    }

    /* Show close button on tab hover or when tab is active */
    .tab:hover .close-btn,
    .tab.active .close-btn {
        opacity: 1;
    }

    .close-btn:hover {
        background-color: color-mix(in srgb, var(--color-text-tertiary), transparent 80%);
        color: var(--color-text-primary);
    }

    /* Hide close button when tab is narrower than 80px via container query */
    @container (max-width: 80px) {
        .close-btn {
            display: none;
        }
    }

    .new-tab-btn {
        flex-shrink: 0;
        display: flex;
        align-items: center;
        justify-content: center;
        width: 24px;
        height: 20px;
        margin-bottom: 3px;
        border: none;
        border-radius: var(--radius-sm);
        background: none;
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
        font-weight: 300;
        cursor: default;
        transition:
            background-color var(--transition-fast),
            color var(--transition-fast);
    }

    .new-tab-btn:hover:not(.disabled) {
        background-color: color-mix(in srgb, var(--color-bg-tertiary), transparent 40%);
        color: var(--color-text-primary);
    }

    .new-tab-btn.disabled {
        opacity: 0.3;
        cursor: default;
    }
</style>
