<script lang="ts" generics="E">
    /**
     * RecentItemsPopover: the query field's dropdown over the full recent-items history.
     *
     * Opens from the chevron in the query field, from `ArrowDown` in the query field (when
     * there are no results to walk), or from `⌘H`. Reuses the generic `Popover` for
     * positioning, focus trap, and Esc-only-closes-the-popover behavior — the same contract
     * as the filter chips, so the dialog's capture-phase Escape never closes the whole dialog
     * while this is open.
     *
     * Generic over the entry shape `E`. Search instantiates it with `E = HistoryEntry`;
     * Selection instantiates it with its own narrower entry. The adapter is the only
     * thing that knows about the entry's internals.
     *
     * The list is fuzzy-searched via `@leeoniya/ufuzzy`, the same library the command palette
     * uses. The haystack is `"{mode-badge} {label}"` per entry (label comes from the adapter),
     * so users can also filter by mode (`"AI screenshots"`, `".*temp"`).
     *
     * Keyboard contract (the dropdown is the primary navigation surface while open, so every
     * key it claims also `stopPropagation()`s — otherwise the host dialog's own handler moves
     * the results cursor underneath and Enter double-fires):
     *   - ↑ / ↓ move the cursor. Neither wraps.
     *   - ↑ on the TOP row exits: `onExitTop()` closes the dropdown and returns focus to the
     *     query field with its text untouched (nothing was picked).
     *   - Enter SELECTS the cursor row: it loads the entry into the dialog and closes the
     *     dropdown. It does NOT run the query; the user presses Enter again in the field
     *     when they're ready. Same for a click.
     *   - Everything else (typing, ⌘C / ⌘V / ⌘X, ←/→ with or without modifiers, Home/End)
     *     is left alone, so the filter field behaves like any other text field.
     *   - Esc closes, via the `Popover` wrapper.
     */
    import uFuzzy from '@leeoniya/ufuzzy'
    import { tString } from '$lib/intl/messages.svelte'
    import Trans from '$lib/intl/Trans.svelte'
    import Popover from '$lib/ui/Popover.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import ShortcutChip from '$lib/ui/ShortcutChip.svelte'
    import TextInput from '$lib/ui/TextInput.svelte'
    import { modeBadge } from './recent-items-utils'
    import type { RecentItemAdapter, RecentItemKey, RecentItemView } from './recent-items-types'

    interface Props {
        anchor: HTMLElement
        open: boolean
        entries: E[]
        /** Adapts an entry into the shape the row UI displays. */
        adapter: RecentItemAdapter<E>
        /** Stable identity for keying. */
        keyFn: RecentItemKey<E>
        onClose: () => void
        /** Loads the entry into the dialog. Never runs it; the caller closes the dropdown. */
        onPick: (entry: E) => void
        onRemove: (entry: E) => void
        /** ↑ on the topmost row: close and hand focus back to the query field, unchanged. */
        onExitTop: () => void
        /** Header / filter-input / empty-state copy. Defaults match Search. */
        filterPlaceholder?: string
        emptyMessage?: string
        ariaLabel?: string
        ariaListboxLabel?: string
    }

    const {
        anchor,
        open,
        entries,
        adapter,
        keyFn,
        onClose,
        onPick,
        onRemove,
        onExitTop,
        filterPlaceholder = tString('queryUi.recent.filterPlaceholder'),
        emptyMessage = tString('queryUi.recent.emptyMessage'),
        ariaLabel = tString('queryUi.recent.popoverAria'),
        ariaListboxLabel = tString('queryUi.recent.listboxAria'),
    }: Props = $props()

    // Tuned the same way as the command palette's fuzzy search.
    const fuzzy = new uFuzzy({ intraMode: 1, interIns: 3 })

    let query = $state('')
    let cursor = $state(0)

    // Reset state every time the popover re-opens so users land on a clean view.
    $effect(() => {
        if (open) {
            query = ''
            cursor = 0
        }
    })

    // Pre-build adapter views once per `entries` change; cheap relative to the user's typing
    // speed and lets the haystack + row UI share one source of truth.
    const views = $derived<RecentItemView[]>(entries.map((e) => adapter(e)))
    const haystack = $derived(views.map((v) => `${modeBadge(v.mode)} ${v.label}`))

    interface Match {
        entry: E
        view: RecentItemView
        indices: number[]
        haystackText: string
    }

    const matches = $derived.by<Match[]>(() => {
        const trimmed = query.trim()
        if (!trimmed) {
            // Empty query: show everything in original order (newest first).
            return entries.map((entry, i) => ({
                entry,
                view: views[i],
                indices: [],
                haystackText: haystack[i],
            }))
        }
        const [idxs, info, order] = fuzzy.search(haystack, trimmed)
        if (!idxs || !order) return []
        return order.map((orderIdx) => {
            const haystackIdx = idxs[orderIdx]
            const entry = entries[haystackIdx]
            const ranges = info.ranges[orderIdx]
            const indices: number[] = []
            for (let i = 0; i < ranges.length; i += 2) {
                const start = ranges[i]
                const end = ranges[i + 1]
                for (let j = start; j < end; j++) indices.push(j)
            }
            return { entry, view: views[haystackIdx], indices, haystackText: haystack[haystackIdx] }
        })
    })

    /** Clamp cursor whenever the match list shrinks below it. */
    $effect(() => {
        if (cursor >= matches.length) {
            cursor = Math.max(0, matches.length - 1)
        }
    })

    /** Highlight matched characters in the haystack text for the active match. */
    function renderHighlights(text: string, indices: number[]): { ch: string; matched: boolean }[] {
        const set = new Set(indices)
        return Array.from(text).map((ch, i) => ({ ch, matched: set.has(i) }))
    }

    /** Selects the cursor row without running it, then closes. */
    function pickCursorRow(): void {
        const m = matches[cursor]
        // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- runtime guard for empty matches
        if (!m) return
        onPick(m.entry)
        onClose()
    }

    function handleKeydown(e: KeyboardEvent): void {
        if (e.key === 'ArrowDown') {
            // No wrap at the bottom: the list has an end, and wrapping past it while the
            // user holds ↓ reads as the cursor teleporting.
            e.preventDefault()
            e.stopPropagation()
            cursor = Math.min(cursor + 1, Math.max(0, matches.length - 1))
        } else if (e.key === 'ArrowUp') {
            e.preventDefault()
            e.stopPropagation()
            if (cursor === 0) {
                // Past the top is the way out: back to the query field, text untouched.
                onExitTop()
                return
            }
            cursor -= 1
        } else if (e.key === 'Enter') {
            e.preventDefault()
            e.stopPropagation()
            pickCursorRow()
        }
    }

    function handleContextMenu(e: MouseEvent, entry: E): void {
        e.preventDefault()
        onRemove(entry)
    }
</script>

<!--
  One ShortcutChip per `<tag>` in the popover-hint message. Each renders the fixed
  key glyph (`key=`), not the tag''s inner content, so the chip is a literal-mode key
  chip regardless of the message text; the glyph also lives in the message so
  translators see it in context. `children` is intentionally ignored.
-->
{#snippet moveChip()}<ShortcutChip key="↑↓" size="sm" />{/snippet}
{#snippet selectChip()}<ShortcutChip key="Enter" size="sm" />{/snippet}

<Popover {anchor} {open} {onClose} {ariaLabel}>
    <div class="recent-popover" onkeydown={handleKeydown} role="search">
        <TextInput
            type="text"
            radius="sm"
            placeholder={filterPlaceholder}
            bind:value={query}
            ariaLabel={filterPlaceholder}
            spellcheck={false}
            autocomplete="off"
            autocapitalize="off"
        />
        <div class="results" role="listbox" aria-label={ariaListboxLabel}>
            {#if matches.length === 0}
                <div class="empty">{emptyMessage}</div>
            {:else}
                {#each matches as match, index (keyFn(match.entry))}
                    {@const badge = modeBadge(match.view.mode)}
                    {@const badgeLen = badge.length + 1}
                    <button
                        type="button"
                        class="result-row"
                        class:is-cursor={index === cursor}
                        role="option"
                        aria-selected={index === cursor}
                        use:tooltip={match.view.tooltip}
                        onclick={() => {
                            cursor = index
                            pickCursorRow()
                        }}
                        oncontextmenu={(e) => {
                            handleContextMenu(e, match.entry)
                        }}
                        onmousemove={() => {
                            cursor = index
                        }}
                    >
                        <span class="row-mode">{badge}</span>
                        <span class="row-body">
                            <span class="row-query">
                                {#each renderHighlights(match.haystackText.slice(badgeLen), match.indices.filter((i) => i >= badgeLen).map((i) => i - badgeLen)) as part, i (i)}
                                    {#if part.matched}
                                        <strong>{part.ch}</strong>
                                    {:else}
                                        {part.ch}
                                    {/if}
                                {/each}
                            </span>
                            <!-- The recall line: when it ran, how much it found, what it was
                                 narrowed by. The full tooltip still carries everything; this
                                 surfaces the two facts that actually identify a past search. -->
                            <span class="row-meta">
                                <span class="row-age">{match.view.ageLabel}</span>
                                {#if match.view.metaLabel}<span class="row-sep" aria-hidden="true">·</span><span
                                        class="row-detail">{match.view.metaLabel}</span
                                    >{/if}
                            </span>
                        </span>
                    </button>
                {/each}
            {/if}
        </div>
        <div class="hint">
            <Trans key="queryUi.recent.popoverHint" snippets={{ moveKey: moveChip, selectKey: selectChip }} />
        </div>
    </div>
</Popover>

<style>
    .recent-popover {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
        width: 480px;
        max-width: 90vw;
    }

    .results {
        display: flex;
        flex-direction: column;
        max-height: 360px;
        overflow-y: auto;
        scrollbar-width: thin;
    }

    .empty {
        padding: var(--spacing-md);
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
        text-align: center;
    }

    .result-row {
        display: flex;
        align-items: baseline;
        gap: var(--spacing-sm);
        padding: var(--spacing-xs) var(--spacing-sm);
        background: transparent;
        border: 0;
        text-align: left;
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
        border-radius: var(--radius-xs);
    }

    .result-row.is-cursor {
        background: var(--color-accent-subtle);
    }

    .row-mode {
        font-size: var(--font-size-xs);
        font-family: var(--font-mono);
        font-weight: 600;
        color: var(--color-text-secondary);
        flex-shrink: 0;
        width: 24px;
    }

    .row-body {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
        flex: 1;
        min-width: 0;
    }

    .row-query {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .row-query strong {
        font-weight: 600;
        color: var(--color-text-primary);
        background: var(--color-accent-subtle);
        border-radius: var(--radius-xs);
    }

    /* The meta line stays one quiet row: it's for recognition at a glance, not reading. */
    .row-meta {
        display: flex;
        align-items: baseline;
        gap: var(--spacing-xxs);
        min-width: 0;
        color: var(--color-text-tertiary);
        font-size: var(--font-size-xs);
    }

    .row-age {
        flex-shrink: 0;
    }

    .row-sep {
        flex-shrink: 0;
    }

    .row-detail {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .hint {
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        gap: var(--spacing-xxs);
        color: var(--color-text-tertiary);
        font-size: var(--font-size-xs);
        padding-top: var(--spacing-xxs);
        border-top: 1px solid var(--color-border-subtle);
    }
</style>
