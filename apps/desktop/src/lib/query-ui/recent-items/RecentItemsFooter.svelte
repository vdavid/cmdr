<script lang="ts" generics="E">
    /**
     * RecentItemsFooter: chip strip at the bottom of the Query dialog showing the latest
     * recent entries plus an "All …" trailing chip that opens the popover.
     *
     * Generic over the entry shape `E`. Search instantiates it with `E = HistoryEntry` and
     * a Search-flavoured adapter; Selection (M7+) instantiates it with its own entry shape
     * and adapter. The adapter is the only thing that knows about the entry's internals.
     *
     * Each chip carries a small mode badge (`AI` / `Aa` / `.*`). Clicking a chip loads the
     * entry into the dialog's state and runs it. For AI entries, the click counts as the
     * user's explicit "yes, please run this" (search-redesign-plan §3.4 / §3.5). Right-click
     * opens a context menu with "Remove from history".
     *
     * Hidden when there are zero entries (the empty state already covers the discoverability
     * gap there).
     */
    import { onDestroy, tick } from 'svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { computeRecentChipsLayout } from '$lib/query-ui/recent-chips-layout'
    import { modeBadge } from './recent-items-utils'
    import type { RecentItemAdapter, RecentItemKey } from './recent-items-types'

    interface Props {
        entries: E[]
        /** Adapts an entry into the shape the component displays. */
        adapter: RecentItemAdapter<E>
        /** Stable identity for `{#each}` keying. */
        keyFn: RecentItemKey<E>
        /** True when the index isn't ready; chips render disabled to avoid no-op clicks. */
        disabled: boolean
        /** Called when a chip is activated. Parent loads + runs the entry. */
        onPick: (entry: E) => void
        /** Called when the user wants to remove an entry via right-click. */
        onRemove: (entry: E) => void
        /** Called when the user clicks "All …" or activates it via keyboard. */
        onOpenAll: () => void
        /**
         * Strip-leading label and trailing-button label / tooltip. Default to Search's
         * existing copy so the M3 rename is visually a no-op for Search; Selection (M7+)
         * passes its own copy ("Recent selections:" / "All selections…").
         */
        leadingLabel?: string
        trailingLabel?: string
        trailingTooltipText?: string
        trailingShortcut?: string
        ariaRegionLabel?: string
        ariaAllButtonLabel?: string
    }

    const {
        entries,
        adapter,
        keyFn,
        disabled,
        onPick,
        onRemove,
        onOpenAll,
        leadingLabel = 'Recent searches:',
        trailingLabel = 'All searches…',
        trailingTooltipText = 'Show all recent searches',
        trailingShortcut = '⌘H',
        ariaRegionLabel = 'Recent searches',
        ariaAllButtonLabel = 'All recent searches',
    }: Props = $props()

    /**
     * R3 U1: dynamic strip layout. The leading "Recent searches:" label and
     * trailing "All searches… ⌘H" button are ALWAYS rendered. The middle slot
     * fits as many chips as it can, dropping the rest silently.
     *
     * We cap the candidate list at 12 (round 2 sliced at 6 and let the strip
     * scroll); the layout helper picks the visible prefix. Going much higher
     * than 12 starts to feel chatty without adding signal — the full history
     * lives behind ⌘H.
     */
    const CANDIDATE_MAX = 12
    const candidates = $derived(entries.slice(0, CANDIDATE_MAX))

    let stripEl: HTMLDivElement | undefined = $state()
    let stripWidth = $state(0)
    let measurements = $state<{
        leadingLabelWidth: number
        trailingButtonWidth: number
        chipWidths: number[]
        itemGap: number
    } | null>(null)

    /**
     * Number of chips to render. Falls back to "show all candidates" until
     * measurement comes online (matches the path-pills uncollapsed fallback).
     * Once measurements land we use the greedy fit helper.
     */
    const visibleCount = $derived.by(() => {
        if (!measurements) return candidates.length
        const { leadingLabelWidth, trailingButtonWidth, chipWidths, itemGap } = measurements
        const layout = computeRecentChipsLayout({
            stripWidth,
            leadingLabelWidth,
            trailingButtonWidth,
            itemGap,
            chipWidths,
        })
        return layout.visibleCount
    })
    const visible = $derived(candidates.slice(0, visibleCount))

    function handleContextMenu(e: MouseEvent, entry: E): void {
        e.preventDefault()
        onRemove(entry)
    }

    /**
     * Re-measure widths from the rendered DOM. The chip widths come from the
     * `.recent-chip` siblings; the leading label and trailing button widths
     * come from their own slots. Triggered on mount and whenever the
     * candidate list changes.
     */
    async function remeasure(): Promise<void> {
        const el = stripEl
        if (!el) return
        await tick()
        // We momentarily render every candidate (the `visibleCount` fallback
        // is `candidates.length` while `measurements` is null) so we can
        // measure them. Once the measurements land, the derived visibleCount
        // re-computes and the strip drops overflow.
        const leadingLabel = el.querySelector<HTMLElement>('.recent-label')
        const trailingButton = el.querySelector<HTMLElement>('.all-searches')
        const chipEls = el.querySelectorAll<HTMLElement>('.recent-chip')
        if (!leadingLabel || !trailingButton || chipEls.length === 0) {
            measurements = null
            return
        }
        const cs = getComputedStyle(el)
        const gapPx = parseFloat(cs.columnGap || cs.gap || '0') || 0
        measurements = {
            leadingLabelWidth: leadingLabel.getBoundingClientRect().width,
            trailingButtonWidth: trailingButton.getBoundingClientRect().width,
            itemGap: gapPx,
            chipWidths: Array.from(chipEls).map((e) => e.getBoundingClientRect().width),
        }
    }

    let resizeObserver: ResizeObserver | undefined
    let mounted = false
    $effect(() => {
        if (!stripEl || mounted) return
        mounted = true
        const el = stripEl
        stripWidth = el.clientWidth
        resizeObserver = new ResizeObserver(() => {
            stripWidth = el.clientWidth
            // Re-measure on resize too: chip widths don't change but the
            // available middle slot does.
            void remeasure()
        })
        resizeObserver.observe(el)
        void remeasure()
    })

    // Re-measure when the candidate list changes (entries added/removed).
    $effect(() => {
        // Track the candidate identities so this effect re-fires on real
        // changes (not just metadata refresh).
        const _key = candidates.map((c) => keyFn(c)).join('|')
        void _key
        if (mounted) void remeasure()
    })

    onDestroy(() => {
        resizeObserver?.disconnect()
    })
</script>

{#if entries.length > 0}
    <!-- R3 U1: label + trailing button always rendered. The middle slot packs as many chips
         as fit; the rest drop silently. The adapter pre-builds each chip's label and tooltip
         so the component never reads entry internals. -->
    <div class="recent-footer" bind:this={stripEl} role="region" aria-label={ariaRegionLabel}>
        <span class="recent-label">{leadingLabel}</span>
        <span class="chip-row">
            {#each visible as entry (keyFn(entry))}
                {@const view = adapter(entry)}
                <!-- R3 U2: the chip's text is truncated via CSS (`text-overflow: ellipsis`
                     on `.chip-query`). The tooltip stacks the full label + the adapter-built
                     multi-line tooltip so the truncated chip stays readable on hover. -->
                <button
                    type="button"
                    class="recent-chip"
                    {disabled}
                    onclick={() => {
                        onPick(entry)
                    }}
                    oncontextmenu={(e) => {
                        handleContextMenu(e, entry)
                    }}
                    use:tooltip={`${view.label}\n${view.tooltip}`}
                    aria-label={view.ariaLabel}
                >
                    <span class="chip-badge">{modeBadge(view.mode)}</span>
                    <span class="chip-query">{view.label}</span>
                </button>
            {/each}
        </span>
        <button
            type="button"
            class="all-searches"
            {disabled}
            onclick={onOpenAll}
            use:tooltip={{ text: trailingTooltipText, shortcut: trailingShortcut }}
            aria-label={ariaAllButtonLabel}
        >
            {trailingLabel}<span class="shortcut-hint" aria-hidden="true">{trailingShortcut}</span>
        </button>
    </div>
{/if}

<style>
    /* No background / border-top here either: the parent `.dialog-footer` owns
       the single uniform footer surface and the hairline above it.
       R3 U1: stopped horizontal scroll; the middle slot packs as many chips as
       fit, the rest drop silently via `RecentSearchesFooter`'s layout helper.
       The label + trailing button are flex-grow:0; the middle row consumes
       what's left and clips with overflow:hidden as the safety net. */
    .recent-footer {
        display: flex;
        flex-wrap: nowrap;
        align-items: center;
        gap: var(--spacing-xs);
        padding: var(--spacing-sm) var(--spacing-lg);
        overflow: hidden;
        min-width: 0;
    }

    /* R3 U1: middle slot holding the visible chips. Grows to fill the strip
       between the leading label and trailing button; pre-overflow `nowrap` +
       `overflow: hidden` keep the row visually clean while the layout helper
       decides what's truly visible. */
    .chip-row {
        display: flex;
        flex: 1 1 auto;
        align-items: center;
        gap: var(--spacing-xs);
        min-width: 0;
        overflow: hidden;
        flex-wrap: nowrap;
    }

    /* D5: leading label so the user reads the strip as "Recent searches: …". */
    .recent-label {
        font-size: var(--font-size-sm);
        color: var(--color-text-tertiary);
        white-space: nowrap;
        margin-right: var(--spacing-xxs);
        flex-shrink: 0;
    }

    .recent-chip,
    .all-searches {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
        padding: var(--spacing-xxs) var(--spacing-sm);
        font-size: var(--font-size-sm);
        font-weight: 500;
        line-height: 1;
        color: var(--color-text-secondary);
        background: transparent;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        white-space: nowrap;
        max-width: 240px;
        flex-shrink: 0;
        transition:
            background var(--transition-base),
            border-color var(--transition-base),
            color var(--transition-base);
    }

    .recent-chip:not(:disabled):hover,
    .all-searches:not(:disabled):hover {
        background: var(--color-accent-subtle);
        border-color: var(--color-accent);
        color: var(--color-text-primary);
    }

    .recent-chip:disabled,
    .all-searches:disabled {
        opacity: 0.5;
        cursor: not-allowed;
    }

    .chip-badge {
        font-size: var(--font-size-xs);
        font-family: var(--font-mono);
        font-weight: 600;
        letter-spacing: 0.04em;
        padding: var(--spacing-xxs) var(--spacing-xs);
        background: var(--color-accent-subtle);
        color: var(--color-text-primary);
        border-radius: var(--radius-xs);
        line-height: 1;
    }

    .chip-query {
        line-height: 1;
        overflow: hidden;
        text-overflow: ellipsis;
        max-width: 180px;
    }

    .all-searches {
        font-style: italic;
        color: var(--color-text-tertiary);
    }

    /* Inline ⌘H hint after the "All searches…" label. Tertiary color so it
       reads as discoverability cue, not action label. */
    .shortcut-hint {
        margin-left: var(--spacing-xs);
        font-family: var(--font-mono);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
        opacity: 0.8;
        font-style: normal;
    }
</style>
