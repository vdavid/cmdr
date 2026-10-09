<script lang="ts" module>
    // The vocabulary lives in `./menu-types` (not here) so non-Svelte consumers resolve it as
    // real types; re-exported for callers that import it alongside the component.
    export type { MenuItem, MenuSection, MenuIcon, MenuRowContext } from './menu-types'

    /** Unique per mounted menu, so `aria-activedescendant` points at this menu's own row. */
    let menuInstanceCount = 0
</script>

<script lang="ts" generics="T">
    /**
     * The house menu surface: a portaled, glass, keyboard-first popup built from sections of
     * rows. It renders nothing while closed, so a consumer writes no `{#if}`.
     *
     * This component owns only the DOM: positioning, scrolling, and the one measurement the
     * drag needs. Every behavior (open state, the cursor, keys, pointer mode, submenus,
     * reorder) belongs to the controller in `./menu-controller.svelte.ts`, which a consumer
     * builds with `createMenu(deps)` and passes in here.
     *
     * ❗ Deliberately NOT built on Ark/zag's `Menu` machine: that machine is trigger-driven and
     * doesn't reliably open (mounted-already-open) or close (controlled `open=false`) when
     * driven programmatically, which every caller here needs.
     */
    import { tick, untrack, type Snippet } from 'svelte'
    import { dependOn } from '$lib/utils/reactivity'
    import { Portal } from '@ark-ui/svelte/portal'
    import Icon from './Icon.svelte'
    import ShortcutChip from './ShortcutChip.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { formatInteger } from '$lib/intl/number-format'
    import { usePortalTarget } from './portal-target'
    import { placeBesideRow, placeOffAnchor, type PlacementRect } from './anchored-placement'
    import type { MenuController } from './menu-controller.svelte'
    import { disclosureRowValue } from './menu-navigation'
    import type { MenuItem, MenuRowContext, MenuSection } from './menu-types'

    /* eslint-disable @typescript-eslint/no-unnecessary-type-arguments -- `T` is this component's OWN
       generic and only looks equal to the `unknown` default these types carry. The auto-fixer strips
       `<T>` here, which erases the payload type from `menu` and from every snippet's `MenuRowContext`,
       so a consumer passing `createMenu<VolumeInfo>` gets `unknown` data back in its snippets. */
    interface Props {
        menu: MenuController<T>
        ariaLabel: string
        /** The surface's minimum width in px; it grows to fit its rows. */
        minWidth?: number
        /** Replaces the row's text (an inline rename field). */
        label?: Snippet<[MenuRowContext<T>]>
        /** Fills the right end of the row (badges, dots, an eject button). */
        trailing?: Snippet<[MenuRowContext<T>]>
        /** A sub-line under the row (a disk-space bar). */
        below?: Snippet<[MenuRowContext<T>]>
        /** Sits under the last section (a list-level warning). */
        footer?: Snippet
        /** Fills the right end of a section's disclosure row (a shortcut chip). */
        disclosureTrailing?: Snippet<[MenuSection<T>]>
        /**
         * The lowest the surface may reach, in viewport px, when that's above the window's
         * own edge: the volume switcher stops above its pane's footer rather than over it.
         */
        getBottomLimit?: () => number | undefined
    }

    const {
        menu,
        ariaLabel,
        minWidth = 220,
        label,
        trailing,
        below,
        footer,
        disclosureTrailing,
        getBottomLimit,
    }: Props = $props()

    menuInstanceCount += 1
    const instanceId = `menu-${String(menuInstanceCount)}`
    // Inside a modal the menu lands in its overlay: under `document.body` it would sit below
    // the scrim, and the modal's focus trap would pull focus straight back out of it.
    const portalTarget = usePortalTarget()
    const rowId = (value: string) => `${instanceId}-row-${value}`

    let surfaceEl: HTMLDivElement | undefined = $state()
    let submenuEl: HTMLDivElement | undefined = $state()
    let position = $state<{ left: number; top: number; maxHeight: number } | null>(null)
    let submenuPosition = $state<{ top: number; left: number; maxHeight: number } | null>(null)

    /**
     * The surface this menu hangs off, when its anchor lives inside ANOTHER menu — the
     * drive-index badge in a volume-switcher row opens its own menu from there. The outer
     * menu reads it back to tell this popup from a pointer-down that really left it.
     */
    const nestedIn = $derived.by(() => {
        const anchor = menu.anchor
        if (anchor?.kind !== 'element') return undefined
        return anchor.element.closest('[data-menu]')?.getAttribute('data-menu-instance') ?? undefined
    })

    /** Gap between the anchor and the surface, and the margin the surface keeps off the viewport. */
    const ANCHOR_GAP = 4
    const VIEWPORT_MARGIN = 8

    /**
     * The one measurement the controller can't make for itself: where a reorderable
     * section's rows sit on screen, which is what a drag's drop target is decided against.
     *
     * ❗ Without this registration a pointer drag reads an EMPTY midpoint list, so
     * `pointerReorderTarget` answers "no target" for every row and the drop silently puts
     * the row back where it started — the reorder looks wired up and does nothing. Rows
     * come back in document order, which is the section's own item order, so a midpoint
     * index lines up with the index the controller drags by.
     */
    menu.surface.bindSurface({
        getRowMidpoints: (sectionId: string) =>
            [
                ...(surfaceEl?.querySelectorAll(`[data-menu-section="${CSS.escape(sectionId)}"] [data-menu-row]`) ?? []),
            ].map((row) => {
                const rect = row.getBoundingClientRect()
                return rect.top + rect.height / 2
            }),
    })

    /** The rect the menu hangs off, whichever way it was opened, and the gap it keeps from it. */
    function anchorRect(): { rect: PlacementRect; gap: number } | null {
        const anchor = menu.anchor
        if (!anchor) return null
        if (anchor.kind === 'point') {
            return { rect: { left: anchor.x, right: anchor.x, top: anchor.y, bottom: anchor.y }, gap: 0 }
        }
        return { rect: anchor.element.getBoundingClientRect(), gap: ANCHOR_GAP }
    }

    /**
     * Pin the surface to the viewport: below the anchor when it fits, above when only that fits,
     * else the roomier side capped so a long list scrolls inside itself (`placeOffAnchor`).
     */
    async function fitToViewport(): Promise<void> {
        await tick()
        const anchor = anchorRect()
        if (!anchor || !surfaceEl) return
        position = placeOffAnchor({
            anchor: anchor.rect,
            // `scrollHeight`: the natural height even once a cap from the last pass applies.
            size: { width: surfaceEl.offsetWidth || minWidth, height: surfaceEl.scrollHeight },
            viewport: { width: window.innerWidth, height: window.innerHeight },
            gap: anchor.gap,
            margin: VIEWPORT_MARGIN,
            bottomLimit: getBottomLimit?.(),
        })
    }

    /**
     * Measure and focus on open; the container holds focus so `aria-activedescendant` is
     * announced.
     *
     * ❗ It depends on `surfaceEl`, ❌ not on `menu.isOpen` alone. Ark's `Portal` mounts its
     * children in a `tick().then(…)` of its own, so this can run its first pass before the
     * node exists — and with nothing to re-run it, the surface would keep the
     * `visibility: hidden` it starts with while the menu takes keys and shows NOTHING. jsdom
     * wins that race and WKWebView loses it, so only the E2E lane ever saw it (three shards
     * red on `[data-menu]` never becoming visible, 2026-09-16).
     */
    $effect(() => {
        const el = surfaceEl
        if (!menu.isOpen || !el) {
            position = null
            return
        }
        void fitToViewport().then(() => {
            el.focus()
        })
    })

    function handleResize(): void {
        if (menu.isOpen) void fitToViewport()
    }

    /**
     * Which sections are open, as one string, so opening or folding one re-fits the surface:
     * the rows it reveals can be wider than anything before, and the clamp was measured
     * without them.
     */
    const disclosureState = $derived(
        menu.sections.map((section) => (section.disclosure ? `${section.id}:${String(section.disclosure.expanded)}` : '')).join(','),
    )
    $effect(() => {
        dependOn(disclosureState)
        if (untrack(() => menu.isOpen && position !== null)) void fitToViewport()
    })

    /** Keep the cursor on screen as the keyboard walks a list taller than the surface. */
    $effect(() => {
        const value = menu.highlightedValue
        if (!menu.isOpen || value === null) return
        void tick().then(() => {
            surfaceEl?.querySelector(`[data-menu-row="${CSS.escape(value)}"]`)?.scrollIntoView({ block: 'nearest' })
        })
    })

    /**
     * A submenu is fixed-positioned off its parent row's rect (inside the scroller it would
     * clip), and measured first, hidden, so it can flip left or slide up to stay on screen.
     * Like the surface's, this depends on `submenuEl`: the node mounts after the value opens.
     */
    $effect(() => {
        const parent = menu.openSubmenuValue
        const el = submenuEl
        if (parent === null || !el) {
            submenuPosition = null
            return
        }
        void tick().then(() => {
            const row = surfaceEl?.querySelector(`[data-menu-row="${CSS.escape(parent)}"]`)
            if (!row) return
            submenuPosition = placeBesideRow({
                row: row.getBoundingClientRect(),
                size: { width: el.offsetWidth, height: el.scrollHeight },
                viewport: { width: window.innerWidth, height: window.innerHeight },
                gap: ANCHOR_GAP,
                // A few px of overlap, the way macOS hands a submenu off from its parent row.
                overlap: 5,
                margin: VIEWPORT_MARGIN,
            })
        })
    })

    /** A pointer-down outside the menu, its anchor, and the anchor's cluster closes it. */
    function handleDocumentPointerDown(event: PointerEvent): void {
        if (!menu.isOpen) return
        const target = event.target as Node | null
        if (!target) return
        if (surfaceEl?.contains(target)) return
        // ❗ The submenu is a SIBLING in the portal, not a child of the surface, so it needs
        // saying: without this, pressing a submenu row closed the menu on pointer-down and the
        // click behind it then activated nothing.
        if (submenuEl?.contains(target)) return
        const anchor = menu.anchor
        if (anchor?.kind === 'element' && anchor.element.contains(target)) return
        // The controls BESIDE the anchor belong to the menu too: pressing one (the switcher
        // chip's eject button) must act without the menu closing out from under it.
        if (menu.keepOpenWithin?.contains(target)) return
        // A menu opened from inside THIS one portals its surface to the body, so containment
        // can't see it, and it never left this menu. It names its host, which is how a
        // badge's menu in a switcher row stops closing the switcher.
        if (hostsPopupAt(target)) return
        menu.close()
    }

    /** Is the pointer-down inside a menu that hangs off a row of this one? */
    function hostsPopupAt(target: Node): boolean {
        const el = target instanceof Element ? target : target.parentElement
        return el?.closest('[data-menu]')?.getAttribute('data-menu-nested-in') === instanceId
    }

    /**
     * A row hosts its own controls (an eject button, a rename field), and those act for
     * themselves: a click on one never also activates the row, so no call site needs
     * `stopPropagation`.
     */
    function isOwnControl(event: Event): boolean {
        const target = event.target as HTMLElement | null
        return target?.closest('button, a, input, select, textarea') != null
    }

    /**
     * What a row says to assistive tech, top-level and submenu alike (see `MenuItemCheck`). A
     * toggle is a `menuitemcheckbox` whether or not it's on, so VoiceOver says "unchecked" on an
     * off one too; both containers qualify as its owner (a top-level row sits in a section's
     * `role="group"` inside the `role="menu"`, a submenu row in the submenu's `role="menu"`).
     */
    function rowA11y(item: MenuItem<unknown>): {
        role: 'menuitem' | 'menuitemcheckbox'
        checked: 'true' | 'false' | undefined
        current: 'location' | undefined
    } {
        const check = item.check
        if (check?.kind === 'toggle') {
            return { role: 'menuitemcheckbox', checked: check.checked ? 'true' : 'false', current: undefined }
        }
        return { role: 'menuitem', checked: undefined, current: check?.kind === 'current' ? 'location' : undefined }
    }

    /** Whether the row draws its checkmark: an on toggle, or the row you're on. */
    function showsCheckmark(item: MenuItem<unknown>): boolean {
        return item.check?.kind === 'current' || (item.check?.kind === 'toggle' && item.check.checked)
    }

    function rowContext(section: MenuSection<T>, item: MenuItem<T>, index: number): MenuRowContext<T> {
        return {
            item,
            section,
            index,
            highlighted: menu.highlightedValue === item.value && !menu.parentHighlightSuppressed,
            dragging: menu.draggingValue === item.value,
        }
    }

    /** The drop-line cue: the gap the grabbed row would land in, as a border on the bordering row. */
    function dropCue(section: MenuSection<T>, index: number): 'above' | 'below' | null {
        if (menu.draggingSectionId !== section.id || menu.dropSlot === null) return null
        if (menu.dropSlot === index) return 'above'
        if (menu.dropSlot === section.items.length && index === section.items.length - 1) return 'below'
        return null
    }

    /**
     * The accelerator column exists only where something uses it, so a menu without accelerators
     * keeps today's row. Once it's there, every row reserves it — including the ones with no
     * accelerator — so the labels stay in one line, the way the checkmark column already works.
     */
    const hasAccelerators = $derived(
        menu.sections.some((section) => section.items.some((item) => item.accelerator != null)),
    )

    /** The glyph column follows the same all-or-nothing rule, per surface. */
    const hasIcons = $derived(
        menu.sections.some(
            (section) => section.disclosure?.icon != null || section.items.some((item) => item.icon != null),
        ),
    )

    const submenuItems = $derived(
        menu.openSubmenuValue === null
            ? []
            : (menu.sections
                  .flatMap((section) => section.items)
                  .find((item) => item.value === menu.openSubmenuValue)?.submenu ?? []),
    )

    const submenuHasIcons = $derived(submenuItems.some((item) => item.icon != null))
</script>

<svelte:window onresize={handleResize} onpointerdown={handleDocumentPointerDown} />

<!-- A row's leading columns, shared by top-level and submenu rows so the two can't drift: the
     accelerator column (top-level only, and only where something declares one), the checkmark
     column (reserved on every row, checked or not), and the glyph, with a blank in its place on
     a row without one wherever a sibling has one. Every placeholder is what keeps the labels on
     one line. -->
{#snippet rowLead(item: MenuItem<unknown>, withAccelerators: boolean, withIcons: boolean)}
    {#if withAccelerators}
        <!-- `aria-keyshortcuts` on the row already says it, so the glyph is decoration. -->
        {#if item.accelerator}
            <span class="menu-accelerator" aria-hidden="true">{item.accelerator}</span>
        {:else}
            <span class="menu-accelerator-placeholder"></span>
        {/if}
    {/if}
    {#if showsCheckmark(item)}
        <span class="menu-check"><Icon name="check" size={14} aria-hidden="true" /></span>
    {:else}
        <span class="menu-check-placeholder"></span>
    {/if}
    {#if item.icon}
        {#if 'lucide' in item.icon}
            <span class="menu-icon"><Icon name={item.icon.lucide} size={16} aria-hidden="true" /></span>
        {:else}
            <img class="menu-icon-image" src={item.icon.src} alt="" />
        {/if}
    {:else if withIcons}
        <span class="menu-icon-placeholder"></span>
    {/if}
{/snippet}

{#if menu.isOpen}
    <Portal container={portalTarget()}>
        <div
            bind:this={surfaceEl}
            class="menu-surface"
            class:keyboard-mode={menu.keyboardMode}
            data-menu=""
            data-menu-instance={instanceId}
            data-menu-nested-in={nestedIn}
            data-keyboard-mode={menu.keyboardMode ? '' : undefined}
            role="menu"
            aria-label={ariaLabel}
            aria-activedescendant={menu.highlightedValue ? rowId(menu.highlightedValue) : undefined}
            tabindex="-1"
            style:left="{position?.left ?? 0}px"
            style:top="{position?.top ?? 0}px"
            style:max-height={position ? `${String(position.maxHeight)}px` : undefined}
            style:min-width="{minWidth}px"
            style:visibility={position ? 'visible' : 'hidden'}
            onmousemove={(event: MouseEvent) => {
                const row = (event.target as HTMLElement | null)?.closest('[data-menu-row]')
                menu.surface.pointerMoved(event, row?.getAttribute('data-menu-row') ?? null)
            }}
        >
            {#each menu.sections as section, sectionIndex (section.id)}
                {#if sectionIndex > 0}
                    <div class="menu-separator"></div>
                {/if}
                <div
                    role="group"
                    data-menu-section={section.id}
                    aria-label={section.disclosure?.label ?? section.heading ?? undefined}
                >
                    {#if section.disclosure}
                        {@const disclosure = section.disclosure}
                        {@const value = disclosureRowValue(section.id)}
                        {@const highlighted = menu.highlightedValue === value && !menu.parentHighlightSuppressed}
                        <!-- The section's own row, in place of a heading: a full row the cursor
                             lands on, whose pick shows or hides the rows under it. The chevron
                             takes the checkmark column, so the label lines up with the rows. -->
                        <!-- svelte-ignore a11y_mouse_events_have_key_events -->
                        <div
                            id={rowId(value)}
                            class="menu-row"
                            class:is-highlighted={highlighted}
                            role="menuitem"
                            tabindex="-1"
                            aria-expanded={disclosure.expanded}
                            data-menu-row={value}
                            data-menu-disclosure={disclosure.expanded ? 'expanded' : 'collapsed'}
                            data-highlighted={highlighted ? '' : undefined}
                            use:tooltip={disclosure.tooltip ?? ''}
                            onclick={(event: MouseEvent) => {
                                if (isOwnControl(event)) return
                                menu.surface.activate(value)
                            }}
                            onmouseover={() => {
                                menu.surface.hover(value)
                            }}
                        >
                            {#if hasAccelerators}
                                <span class="menu-accelerator-placeholder"></span>
                            {/if}
                            <span class="menu-disclosure-chevron" class:is-expanded={disclosure.expanded}></span>
                            {#if disclosure.icon && 'lucide' in disclosure.icon}
                                <span class="menu-icon"><Icon name={disclosure.icon.lucide} size={16} aria-hidden="true" /></span>
                            {:else if disclosure.icon}
                                <img class="menu-icon-image" src={disclosure.icon.src} alt="" />
                            {:else if hasIcons}
                                <span class="menu-icon-placeholder"></span>
                            {/if}
                            <span class="menu-label">{disclosure.label}</span>
                            <span class="menu-disclosure-count" data-menu-disclosure-count=""
                                >{formatInteger(section.items.length)}</span
                            >
                            <!-- eslint-disable-next-line @typescript-eslint/no-confusing-void-expression -- Svelte {@render} syntax -->
                            {#if disclosureTrailing}{@render disclosureTrailing(section)}{/if}
                        </div>
                    {:else if section.heading}
                        <div class="menu-heading" data-menu-heading="" aria-hidden="true">{section.heading}</div>
                    {/if}
                    <!-- A folded section's rows aren't rendered, walked, or claiming keys. -->
                    {#if section.disclosure?.expanded !== false}
                    {#if section.items.length === 0 && section.emptyLabel}
                        <!-- A real (empty) state, not a missing section: present, said, and unfocusable. -->
                        <div class="menu-empty" data-menu-empty="" role="menuitem" aria-disabled="true" tabindex="-1">
                            {section.emptyLabel}
                        </div>
                    {/if}
                    {#each section.items as item, index (item.value)}
                        {@const context = rowContext(section, item, index)}
                        {@const cue = dropCue(section, index)}
                        {@const a11y = rowA11y(item)}
                        <!-- svelte-ignore a11y_mouse_events_have_key_events -->
                        <div
                            id={rowId(item.value)}
                            class="menu-row"
                            class:is-highlighted={context.highlighted}
                            class:is-disabled={item.disabled}
                            class:is-dragging={context.dragging}
                            class:is-drop-above={cue === 'above'}
                            class:is-drop-below={cue === 'below'}
                            class:is-reorderable={section.reorderable}
                            role={a11y.role}
                            tabindex="-1"
                            aria-checked={a11y.checked}
                            aria-current={a11y.current}
                            aria-disabled={item.disabled ? 'true' : undefined}
                            aria-haspopup={item.submenu?.length ? 'menu' : undefined}
                            aria-expanded={item.submenu?.length ? menu.openSubmenuValue === item.value : undefined}
                            aria-keyshortcuts={[item.accelerator, item.shortcut].filter(Boolean).join(' ') || undefined}
                            data-menu-row={item.value}
                            data-accelerator={item.accelerator}
                            data-highlighted={context.highlighted ? '' : undefined}
                            data-checked={showsCheckmark(item) ? '' : undefined}
                            data-disabled={item.disabled ? '' : undefined}
                            data-dragging={context.dragging ? '' : undefined}
                            data-drop-cue={cue}
                            data-drop-slot={cue === null ? undefined : menu.dropSlot}
                            use:tooltip={item.tooltip ?? ''}
                            onclick={(event: MouseEvent) => {
                                if (isOwnControl(event)) return
                                menu.surface.activate(item.value)
                            }}
                            oncontextmenu={(event: MouseEvent) => {
                                event.preventDefault()
                                menu.surface.contextMenu(item.value)
                            }}
                            onmousedown={(event: MouseEvent) => {
                                if (isOwnControl(event)) return
                                menu.surface.startDrag(item.value, event)
                            }}
                            onmouseover={() => {
                                menu.surface.hover(item.value)
                            }}
                        >
                            <!-- eslint-disable-next-line @typescript-eslint/no-confusing-void-expression -- Svelte {@render} syntax -->
                            {@render rowLead(item, hasAccelerators, hasIcons)}
                            {#if label}
                                {@render label(context)}
                            {:else}
                                <span class="menu-label">{item.label}</span>
                            {/if}
                            {#if trailing}{@render trailing(context)}{/if}
                            {#if item.shortcut}
                                <span class="menu-shortcut" aria-hidden="true">
                                    <ShortcutChip key={item.shortcut} size="sm" />
                                </span>
                            {/if}
                            {#if item.submenu?.length}
                                <span class="menu-submenu-arrow"></span>
                            {/if}
                        </div>
                        {#if below}{@render below(context)}{/if}
                    {/each}
                    {/if}
                </div>
            {/each}
            {#if footer}{@render footer()}{/if}
        </div>

        {#if menu.openSubmenuValue !== null}
            <div
                bind:this={submenuEl}
                class="menu-surface menu-submenu"
                data-menu-submenu=""
                role="menu"
                aria-label={ariaLabel}
                style:top="{submenuPosition?.top ?? 0}px"
                style:left="{submenuPosition?.left ?? 0}px"
                style:max-height={submenuPosition ? `${String(submenuPosition.maxHeight)}px` : undefined}
                style:visibility={submenuPosition ? 'visible' : 'hidden'}
                onmouseleave={() => {
                    menu.surface.closeSubmenu()
                }}
            >
                {#each submenuItems as child (child.value)}
                    {@const childA11y = rowA11y(child)}
                    {#if child.separatorBefore}
                        <div class="menu-separator" role="separator"></div>
                    {/if}
                    <!-- svelte-ignore a11y_mouse_events_have_key_events -->
                    <div
                        class="menu-row"
                        class:is-highlighted={menu.submenuHighlightedValue === child.value}
                        class:is-disabled={child.disabled}
                        role={childA11y.role}
                        tabindex="-1"
                        aria-checked={childA11y.checked}
                        aria-current={childA11y.current}
                        aria-disabled={child.disabled ? 'true' : undefined}
                        data-menu-row={child.value}
                        data-highlighted={menu.submenuHighlightedValue === child.value ? '' : undefined}
                        data-checked={showsCheckmark(child) ? '' : undefined}
                        data-disabled={child.disabled ? '' : undefined}
                        use:tooltip={child.tooltip ?? ''}
                        onmousedown={(event: MouseEvent) => {
                            // Focus stays on the surface: this row unmounts after a pick, and
                            // focus on a node that left the document drops to <body>.
                            event.preventDefault()
                        }}
                        onmouseover={() => {
                            menu.surface.hoverSubmenu(child.value)
                        }}
                        onclick={() => {
                            menu.surface.activate(child.value)
                        }}
                    >
                        <!-- eslint-disable-next-line @typescript-eslint/no-confusing-void-expression -- Svelte {@render} syntax -->
                        {@render rowLead(child, false, submenuHasIcons)}
                        <span class="menu-label">{child.label}</span>
                    </div>
                {/each}
            </div>
        {/if}
    </Portal>
{/if}

<style>
    /* Frosted-glass surface, shared tokens with `Select` / the tooltip so every glass surface
       reads as one material, following the macOS Liquid Glass slider (`app.css` § Frosted-glass
       material); the blur drops under reduced transparency (the token flips opaque). The shape
       follows a native macOS 26+ menu: rounder corners, rows highlighted as inset pills. */
    .menu-surface {
        position: fixed;
        overflow-y: auto;
        padding: var(--spacing-xs) 0;
        background: var(--color-bg-glass);
        -webkit-backdrop-filter: var(--glass-backdrop);
        backdrop-filter: var(--glass-backdrop);
        border: 0.5px solid var(--color-border-glass);
        border-radius: var(--radius-menu);
        box-shadow: var(--shadow-glass), var(--shadow-glass-rim);
        z-index: var(--z-overlay);
        outline: none;
    }

    /* Above the parent surface, which it overlaps by a few px. */
    .menu-submenu {
        z-index: calc(var(--z-overlay) + 1);
        min-width: 220px;
    }

    .menu-heading {
        padding: var(--spacing-sm) var(--spacing-md) var(--spacing-xs);
        font-size: var(--font-size-sm);
        font-weight: 500;
        color: var(--color-text-tertiary);
        text-transform: uppercase;
        /*noinspection CssNonIntegerLengthInPixels*/
        letter-spacing: 0.5px;
    }

    /* Inset to the row text, and as faint as the surface's own hairline, like a native one. */
    .menu-separator {
        height: 1px;
        margin: var(--spacing-xs) var(--spacing-md);
        background-color: var(--color-border-glass);
    }

    .menu-row {
        position: relative;
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        margin: 0 var(--spacing-xs);
        padding: var(--spacing-sm);
        border-radius: var(--radius-md);
        cursor: default;
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
    }

    /* One cursor at a time: hover paints only while the keyboard isn't driving. */
    /*noinspection CssUnusedSymbol*/
    .menu-surface:not(.keyboard-mode) .menu-row:not(.is-disabled):hover,
    .menu-row.is-highlighted {
        background-color: var(--color-accent-subtle);
    }

    .menu-row.is-disabled {
        opacity: 0.5;
    }

    .menu-row.is-reorderable {
        cursor: grab;
    }

    /*noinspection CssUnusedSymbol*/
    .menu-row.is-dragging {
        cursor: grabbing;
        opacity: 0.5;
    }

    /* Drop-line cue: a border marking the gap the pointer is over. */
    /*noinspection CssUnusedSymbol*/
    .menu-row.is-drop-above {
        box-shadow: inset 0 2px 0 0 var(--color-accent);
    }

    /*noinspection CssUnusedSymbol*/
    .menu-row.is-drop-below {
        box-shadow: inset 0 -2px 0 0 var(--color-accent);
    }

    .menu-empty {
        padding: var(--spacing-sm) var(--spacing-md);
        color: var(--color-text-tertiary);
        font-style: italic;
        font-size: var(--font-size-sm);
        cursor: default;
        user-select: none;
    }

    .menu-check {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: calc(14px * var(--font-scale));
        flex-shrink: 0;
    }

    .menu-check-placeholder {
        width: 14px;
        flex-shrink: 0;
    }

    /* A plain tertiary digit, same 14px column as the checkmark beside it, so the two leading
       columns read as one gutter and a `1` and a `0` sit on the same axis. */
    .menu-accelerator {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: calc(14px * var(--font-scale));
        flex-shrink: 0;
        color: var(--color-text-tertiary);
    }

    .menu-accelerator-placeholder {
        width: 14px;
        flex-shrink: 0;
    }

    .menu-shortcut {
        margin-left: auto;
        padding-left: var(--spacing-sm);
        flex-shrink: 0;
    }

    .menu-icon,
    .menu-icon-image,
    .menu-icon-placeholder {
        width: var(--spacing-icon-size);
        height: var(--spacing-icon-size);
        flex-shrink: 0;
        object-fit: contain;
    }

    .menu-icon {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        color: var(--color-text-secondary);
    }

    .menu-label {
        flex: 1;
        min-width: 0;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    /* The disclosure chevron sits in the checkmark column (same 14px), so the section's label
       lines up with the rows it folds. A CSS triangle like the submenu arrow, pointing right
       while folded and down while open. */
    .menu-disclosure-chevron {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: calc(14px * var(--font-scale));
        flex-shrink: 0;
    }

    .menu-disclosure-chevron::before {
        content: '';
        border-top: 4px solid transparent;
        border-bottom: 4px solid transparent;
        border-left: 5px solid var(--color-text-tertiary);
        transition: transform var(--transition-base);
    }

    /*noinspection CssUnusedSymbol*/
    .menu-disclosure-chevron.is-expanded::before {
        transform: rotate(90deg);
    }

    .menu-disclosure-count {
        flex-shrink: 0;
        color: var(--color-text-tertiary);
        font-variant-numeric: tabular-nums;
    }

    @media (prefers-reduced-motion: reduce) {
        .menu-disclosure-chevron::before {
            transition: none;
        }
    }

    /* CSS triangle, not a font character: `›` renders at inconsistent sizes across fonts. */
    .menu-submenu-arrow {
        display: inline-block;
        width: 0;
        height: 0;
        margin-left: auto;
        border-top: 4px solid transparent;
        border-bottom: 4px solid transparent;
        border-left: 5px solid var(--color-text-tertiary);
        flex-shrink: 0;
    }
</style>
