<script lang="ts">
    import type { TabState, TabId } from './tab-types'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { deriveTabLabel } from './tab-label'
    import { getVolumes } from '$lib/stores/volume-store.svelte'

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

    const volumeNameById = $derived(new Map(getVolumes().map((v) => [v.id, v.name])))

    function tabTooltipText(tab: TabState): string {
        const volumeName = volumeNameById.get(tab.volumeId)
        return volumeName ? `${volumeName} · ${tab.path}` : tab.path
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

    /** Double-click on an empty area of the tab bar opens a new tab.
     *  Empty area = the bar's padding strip, the trailing flex space inside `.tab-list`,
     *  and the 3px top spacer. We bail when the click hits a tab, the close button,
     *  or the new-tab button so those still behave as expected. */
    function handleTabBarDblClick(event: MouseEvent) {
        if (event.button !== 0) return
        const target = event.target as Element | null
        if (!target) return
        if (target.closest('.tab, .close-btn, .new-tab-btn')) return
        onNewTab()
    }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="tab-bar" onclick={onPaneFocus} ondblclick={handleTabBarDblClick}>
    <div class="tab-list" role="tablist" aria-label="{paneId} pane tabs">
        {#each tabs as tab, index (tab.id)}
            {@const isActive = tab.id === activeTabId}
            {@const isAfterActive = index > 0 && tabs[index - 1].id === activeTabId}
            <button
                class="tab"
                class:active={isActive}
                class:pinned={tab.pinned}
                class:unreachable={!!tab.unreachable}
                class:after-active={isAfterActive}
                role="tab"
                aria-selected={isActive}
                use:tooltip={tabTooltipText(tab)}
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
                {#if isActive}
                    <!-- Chrome-style "shoulders": small concave quarter-
                         circle wedges that stick out past the active tab's
                         bottom corners, carving a smooth rounded notch into
                         the adjacent inactive tabs. They share the active
                         tab's bg color so the tab reads as "flowing into"
                         the path bar surface below. -->
                    <span class="tab-shoulder tab-shoulder-left" aria-hidden="true"></span>
                    <span class="tab-shoulder tab-shoulder-right" aria-hidden="true"></span>
                {/if}
                {#if tab.unreachable}
                    <span class="warning-icon" use:tooltip={'Unreachable'} aria-label="Unreachable">
                        <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
                            <path
                                d="M8 1a.75.75 0 0 1 .65.375l6.25 10.75A.75.75 0 0 1 14.25 13H1.75a.75.75 0 0 1-.65-1.125L7.35 1.375A.75.75 0 0 1 8 1zm0 4a.75.75 0 0 0-.75.75v3a.75.75 0 0 0 1.5 0v-3A.75.75 0 0 0 8 5zm0 6.5a.75.75 0 1 0 0-1.5.75.75 0 0 0 0 1.5z"
                            />
                        </svg>
                    </span>
                {:else if tab.pinned}
                    <span class="pin-icon" use:tooltip={'Pinned'} aria-label="Pinned">
                        <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
                            <path
                                d="M11 7V5a3 3 0 1 0-6 0v2H4a1 1 0 0 0-1 1v5a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1V8a1 1 0 0 0-1-1h-1zM6 5a2 2 0 1 1 4 0v2H6V5z"
                            />
                        </svg>
                    </span>
                {/if}
                <span class="tab-label">
                    {deriveTabLabel(tab.path)}
                </span>
                {#if !isSingleTab}
                    <span
                        class="close-btn"
                        aria-hidden="true"
                        use:tooltip={'Close tab'}
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
        use:tooltip={{ text: 'New tab', shortcut: '⌘T' }}
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
        /* Tabs sit flush with the window title-bar — no top spacer. Tabs
         * anchor to the bar's content-area bottom (align-items: end) and
         * match the bar height exactly via `--spacing-tab-bar-height`. */
        height: var(--spacing-tab-bar-height);
        min-height: var(--spacing-tab-bar-height);
        max-height: var(--spacing-tab-bar-height);
        /* Bar bg matches the inactive tabs so adjacent inactive tabs blend
           into the bar around them. The active tab (`bg-secondary`, same
           as the col header / path bar below) is the only contrasting
           surface in the strip. Side-effect: the active tab's
           `bg-secondary` no longer stacks on a second `bg-secondary` layer
           from the bar, so its effective opacity now matches the path bar
           and col header exactly. */
        background-color: var(--color-bg-tab-inactive);
        padding: 0 var(--spacing-xxs);
        /* Need `overflow: visible` so the active-tab shoulders can extend
           outside the tab into the surrounding gap + inactive-tab area. */
        overflow: visible;
    }

    .tab-list {
        display: flex;
        flex: 1;
        min-width: 0;
        align-items: end;
        /* `overflow: visible` so the active-tab shoulders can extend
           outside the list into the surrounding (gap + inactive-tab)
           area without being clipped. */
        overflow: visible;
        /* 5 px gap = 2 px margin + 1 px separator + 2 px margin between
           adjacent tabs. The `.tab::before` separator (above) sits inside
           this gap at `left: -3px`. */
        /* stylelint-disable-next-line declaration-property-value-disallowed-list */
        gap: 5px;
    }

    .tab {
        position: relative;
        display: flex;
        align-items: center;
        gap: var(--spacing-xxs);
        min-width: 32px;
        max-width: 180px;
        flex: 1 1 0;
        /* Tabs fill the entire bar height. With `.tab-bar { align-items: end }`
         * and the tab matching the bar height, the colored top edge of the
         * active tab is always at the bar's top, flush below the (fixed)
         * window title-bar at every text scale. */
        height: var(--spacing-tab-bar-height);
        padding: 0 var(--spacing-sm);
        border: none;
        border-radius: var(--radius-sm) var(--radius-sm) 0 0;
        /* Inactive tabs get a distinct "recessed" bg: slightly darker than
           `--color-bg-secondary` in light mode, slightly lighter in dark
           mode. `.tab.active` below overrides this with `bg-secondary` so
           the selected tab merges with the path bar. */
        background-color: var(--color-bg-tab-inactive);
        color: var(--color-text-secondary);
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

    /* Faint hairline separator centered in the gap between two adjacent
       inactive tabs. The selector skips: the active tab itself, the
       inactive tab immediately *after* the active one (its left side
       meets the active tab, where we hide the gap), and the first child
       (no preceding tab). `.before-active` IS allowed — its left side
       meets another inactive tab. ~70 % of tab height, centered by
       top/bottom 15 %. */
    .tab:not(.active, .after-active, :first-child)::before {
        content: '';
        position: absolute;
        left: -3px;
        top: 15%;
        bottom: 15%;
        width: 1px;
        background-color: var(--color-tab-separator);
    }

    /* Remove the gap on either side of the active tab. `.tab-list { gap:
       5px }` paints a 5 px strip of `--color-bg-tab-inactive` between
       every two tabs, which reads as a hard color boundary right next to
       the active tab. Pulling the active tab's margins by `-5 px` (= the
       gap) closes the strip; the Chrome-style shoulders then bridge to
       the adjacent inactive tabs. */
    .tab.active {
        /* Same bg as the path bar below (`--color-bg-secondary`), so the
           active tab visually merges with the chrome row underneath. The
           accent line at the top + the rounded "shoulder" elements at the
           bottom are what make it read as "tab, not gap". */
        background-color: var(--color-bg-secondary);
        color: var(--color-text-primary);
        font-weight: 500;
        /* Bar height + 1px so the active tab covers any seam with the
           path bar; the extra px hangs below via `margin-bottom: -1px`. */
        height: calc(var(--spacing-tab-bar-height) + 1px);
        /* stylelint-disable-next-line declaration-property-value-disallowed-list */
        margin-bottom: -1px;
        /* Absorb the `.tab-list { gap: 5px }` on either side: the gap
           paints `--color-bg-tab-inactive` between tabs, which reads as a
           hard color stripe right next to the active tab. Negative
           margins close that strip so the active tab visually butts up
           against (or under, via shoulders) the adjacent inactive tabs.
           First-child / last-child overrides below handle the ends of
           the tab list where there's no gap to absorb. */
        /* stylelint-disable-next-line declaration-property-value-disallowed-list -- exact gap absorption, must match `.tab-list { gap }` */
        margin-left: -5px;
        /* stylelint-disable-next-line declaration-property-value-disallowed-list -- exact gap absorption, must match `.tab-list { gap }` */
        margin-right: -5px;
        z-index: 1;
        /* Let the Chrome-style shoulders extend past the tab's left/right
           edges into the surrounding inactive-tab area. */
        overflow: visible;
    }

    .tab.active:first-child {
        margin-left: 0;
    }

    .tab.active:last-child {
        margin-right: 0;
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
        /* stylelint-disable-next-line declaration-property-value-disallowed-list -- partial radius, no token */
        border-radius: 1px 1px 0 0;
    }

    /* Chrome-style "shoulders": pseudo-elements that stick out past the
       active tab's bottom-left and bottom-right corners with the same bg
       as the tab, then mask out a quarter-circle so the visible shape is
       a concave curve. This carves a smooth rounded notch out of the
       adjacent inactive tab's surface — the active tab reads as "flowing
       into" the path bar below while the inactive tabs around it bend
       away. Won't be visible when there's no adjacent inactive tab
       (`:first-child` / `:last-child` ends of the tab list), which is
       fine — the bar's own bg matches the active tab anyway. */
    .tab-shoulder {
        position: absolute;
        /* Explicit `top: auto` so `bottom: 0` is the only vertical anchor —
           inherited cascade (or a future stacking-context tweak) can't shove
           these to the top accidentally. */
        top: auto;
        bottom: 0;
        width: 8px;
        height: 8px;
        background-color: var(--color-bg-secondary);
        pointer-events: none;
    }

    /* Left shoulder: 8 × 8 box that sticks out 8 px to the left of the
       active tab's bottom-left corner. The mask keeps only the
       *bottom-right* curved triangle of the box visible (= the area
       closest to the tab); the rest is transparent. That visible chunk
       forms a smooth convex bulge extending the active tab's
       bottom-left corner outward into the adjacent inactive tab's
       surface. */
    .tab-shoulder-left {
        left: -8px;
        /* `transparent` inside the top-left quarter-disc (away from tab),
           `black` outside (= near tab → opaque, visible). */
        /* stylelint-disable-next-line declaration-property-value-disallowed-list -- mask uses raw px in radial-gradient args */
        mask-image: radial-gradient(circle at top left, transparent 8px, black 8px);
        /* stylelint-disable-next-line declaration-property-value-disallowed-list -- vendor-prefixed mask, WKWebView fallback */
        -webkit-mask-image: radial-gradient(circle at top left, transparent 8px, black 8px);
    }

    /* Right shoulder: mirror image of the left — visible bottom-left
       curved triangle near the tab's bottom-right corner. */
    .tab-shoulder-right {
        right: -8px;
        /* stylelint-disable-next-line declaration-property-value-disallowed-list -- mask uses raw px in radial-gradient args */
        mask-image: radial-gradient(circle at top right, transparent 8px, black 8px);
        /* stylelint-disable-next-line declaration-property-value-disallowed-list -- vendor-prefixed mask, WKWebView fallback */
        -webkit-mask-image: radial-gradient(circle at top right, transparent 8px, black 8px);
    }

    /* Hover on inactive tabs: a tiny shift toward the *opposite* end of
       the lightness scale. Light mode darkens by 5 %, dark mode lightens
       by 5 %. Either way the inactive tab visibly responds to the cursor.
       Label flips to `--color-text-primary` so the contrast against the
       slightly-darker (light) hover bg stays well above WCAG AA. */
    .tab:hover:not(.active) {
        background-color: color-mix(in srgb, var(--color-bg-tab-inactive), black 5%);
        color: var(--color-text-primary);
    }

    @media (prefers-color-scheme: dark) {
        .tab:hover:not(.active) {
            background-color: color-mix(in srgb, var(--color-bg-tab-inactive), white 5%);
        }
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
        padding-left: var(--spacing-xl);
    }

    .tab.unreachable {
        padding-left: var(--spacing-xl);
    }

    .warning-icon {
        position: absolute;
        left: 5px;
        top: 50%;
        transform: translateY(-50%);
        display: flex;
        align-items: center;
        color: var(--color-warning);
        opacity: 0.8;
        line-height: 0;
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
        transition:
            opacity var(--transition-fast),
            background-color var(--transition-fast),
            color var(--transition-fast);
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
        margin-bottom: var(--spacing-xxs);
        border: none;
        border-radius: var(--radius-sm);
        background: none;
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
        font-weight: 400;
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
