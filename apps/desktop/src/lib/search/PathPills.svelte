<script lang="ts">
    /**
     * PathPills: A path rendered as a strip of clickable ancestor pills.
     *
     * Replaces the flat `parentPath` string in `SearchResults` rows. Each segment is a
     * small button; clicking navigates the active pane to that ancestor folder AND closes
     * the dialog (the parent wires both via `onPick`).
     *
     * Per search-fixup-brief item 4: the strip NEVER wraps to two lines. When the full
     * path doesn't fit its container, the middle pills collapse into a single "…" pill;
     * hovering that pill shows a tooltip listing the hidden pills, with the same
     * nav-to-ancestor behavior on click. Measurement uses `@chenglou/pretext` via
     * `createPretextMeasure` so we get pixel-accurate widths without DOM reflow.
     *
     * Per search-redesign-plan §3.8 (still in force):
     *   - Pills are NOT in the keyboard Tab order (`tabindex="-1"`).
     *   - macOS and Linux only: split strictly on `/`. No `\` handling.
     *   - Pill chrome: `--radius-sm`, `--spacing-xxs / --spacing-xs` padding,
     *     `--font-size-xs`, hover background = `--color-bg-tertiary`.
     */
    import { onDestroy, tick } from 'svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { createPretextMeasure } from '$lib/utils/shorten-middle'

    interface Props {
        /** Path to render (typically `entry.parentPath`; may also be the entry's own path). */
        path: string
        /**
         * Called when the user clicks a pill. Receives the absolute path to that ancestor.
         * The parent is expected to navigate the active pane and close the dialog.
         */
        onPick: (path: string) => void
    }

    const { path, onPick }: Props = $props()

    interface Segment {
        label: string
        fullPath: string
    }

    /**
     * Splits a POSIX-style path into `{ label, fullPath }` segments. Returns one segment
     * per directory component, each `fullPath` carrying the absolute path up to and
     * including that segment. Empty input or a bare `/` returns a single "/" pill.
     */
    function splitPath(input: string): Segment[] {
        if (!input) return []
        const isAbsolute = input.startsWith('/')
        const parts = input.split('/').filter((p) => p.length > 0)
        if (parts.length === 0) {
            return isAbsolute ? [{ label: '/', fullPath: '/' }] : []
        }
        const out: Segment[] = []
        let acc = ''
        for (const part of parts) {
            acc = isAbsolute || out.length > 0 ? `${acc}/${part}` : part
            out.push({ label: part, fullPath: acc })
        }
        return out
    }

    const segments = $derived(splitPath(path))

    // ── Measure-driven collapse ────────────────────────────────────────────────
    //
    // We measure the rendered text widths of every segment (plus separators and an
    // ellipsis pill) and figure out how many middle pills to hide so the strip fits
    // the column. Pretext is async; until it loads we render every segment with CSS
    // `flex-wrap: nowrap` + `overflow: hidden`, so the worst case before measurement
    // is a horizontally-clipped strip — never a two-line wrap.

    let container: HTMLSpanElement | undefined = $state()
    let containerWidth = $state(0)
    let measureWidth = $state<((text: string) => number) | null>(null)
    let pretextPromise: Promise<typeof import('@chenglou/pretext')> | null = null

    function loadPretext(): Promise<typeof import('@chenglou/pretext')> {
        if (!pretextPromise) {
            pretextPromise = import('@chenglou/pretext')
        }
        return pretextPromise
    }

    /**
     * Read the CSS font string from a DOM element. Mirrors `readFont` in
     * `shorten-middle-action.ts`; the inline `style.font` is empty in some Chromium
     * versions so we synthesize from size + family.
     */
    function readFont(node: HTMLElement): string {
        const style = getComputedStyle(node)
        if (style.font) return style.font
        return `${style.fontSize} ${style.fontFamily}`
    }

    /** Approximate width contribution of a pill rendering `label`. */
    function pillWidth(label: string): number {
        if (!measureWidth) return 0
        // Pill chrome: 4 px horizontal padding × 2 = ~8 px. We measure the text
        // and pad by 16 px total to cover the chrome plus a safety margin. The
        // estimate doesn't need to be perfect; overshooting just hides one more
        // pill than strictly necessary, never undershooting (no wrap).
        return measureWidth(label) + 16
    }

    /** Width of the `/` separator span (text width + gap on each side). */
    function sepWidth(): number {
        if (!measureWidth) return 0
        return measureWidth('/') + 4
    }

    interface Layout {
        leading: Segment[]
        collapsed: Segment[]
        trailing: Segment[]
    }

    /**
     * Decide which segments stay visible. We always keep the first and last
     * segments pinned (root/anchor and the immediate parent are the most useful
     * signals), then add middle pills back from the trailing edge inward until
     * the strip stops fitting.
     */
    const layout = $derived.by<Layout>(() => {
        const segs = segments
        if (segs.length <= 2 || !measureWidth || containerWidth <= 0) {
            return { leading: segs, collapsed: [], trailing: [] }
        }
        const sep = sepWidth()
        function totalWidth(visible: string[]): number {
            let w = 0
            for (let i = 0; i < visible.length; i++) {
                w += pillWidth(visible[i])
                if (i < visible.length - 1) w += sep
            }
            return w
        }
        // Try the no-collapse case first.
        const allLabels = segs.map((s) => s.label)
        if (totalWidth(allLabels) <= containerWidth) {
            return { leading: segs, collapsed: [], trailing: [] }
        }
        // Otherwise pin first + last, ellipsis in the middle, and re-add as many
        // trailing-side ancestors as fit.
        const first = segs[0]
        const last = segs[segs.length - 1]
        const middle = segs.slice(1, -1)
        const trailing = [last]
        const collapsed = [...middle]
        for (let i = middle.length - 1; i >= 0; i--) {
            const candidate = middle[i]
            const newTrailing = [candidate, ...trailing]
            const labels = [first.label, '…', ...newTrailing.map((s) => s.label)]
            if (totalWidth(labels) > containerWidth) break
            trailing.unshift(candidate)
            collapsed.pop() // remove the last entry, which is `candidate`
        }
        const finalLabels = [first.label, '…', ...trailing.map((s) => s.label)]
        if (totalWidth(finalLabels) > containerWidth) {
            // Even first + ellipsis + last doesn't fit. Drop the first too. The
            // ellipsis stays so we still signal "more here".
            const labelsNoFirst = ['…', ...trailing.map((s) => s.label)]
            if (totalWidth(labelsNoFirst) > containerWidth) {
                // Still doesn't fit. Drop the ellipsis too; keep only `last`. CSS
                // `overflow: hidden` will handle anything pathologically narrow.
                return { leading: [], collapsed: [first, ...collapsed], trailing }
            }
            return { leading: [], collapsed: [first, ...collapsed], trailing }
        }
        return { leading: [first], collapsed, trailing }
    })

    /**
     * HTML for the `…` pill's tooltip. Hidden pills render as clickable buttons; a
     * delegated `mousedown` handler routes the click back to `onPick`.
     */
    const collapsedTooltipHtml = $derived.by(() => {
        if (layout.collapsed.length === 0) return ''
        const items = layout.collapsed
            .map((seg) => {
                const safeLabel = escapeHtml(seg.label)
                const safePath = escapeHtml(seg.fullPath)
                return `<button type="button" class="hidden-pill" data-path="${safePath}" tabindex="-1">${safeLabel}</button>`
            })
            .join('<span class="hidden-sep" aria-hidden="true">/</span>')
        return `<div class="hidden-pills">${items}</div>`
    })

    function escapeHtml(s: string): string {
        return s
            .replace(/&/g, '&amp;')
            .replace(/</g, '&lt;')
            .replace(/>/g, '&gt;')
            .replace(/"/g, '&quot;')
            .replace(/'/g, '&#39;')
    }

    /**
     * Delegated mousedown handler for the tooltip's hidden-pill buttons. The tooltip
     * body lives in a singleton container appended to `<body>`; we route any click
     * inside a `.hidden-pill` to `onPick(path)`.
     */
    function handleTooltipMouseDown(e: MouseEvent): void {
        const target = (e.target as HTMLElement | null)?.closest('.hidden-pill') as HTMLElement | null
        if (!target) return
        const p = target.dataset.path
        if (!p) return
        e.preventDefault()
        e.stopPropagation()
        onPick(p)
    }

    let resizeObserver: ResizeObserver | undefined
    let mounted = false
    $effect(() => {
        if (!container || mounted) return
        mounted = true
        const el = container
        containerWidth = el.clientWidth
        resizeObserver = new ResizeObserver(() => {
            containerWidth = el.clientWidth
        })
        resizeObserver.observe(el)
        void tick().then(async () => {
            const pretext = await loadPretext().catch(() => null)
            if (!pretext) return
            measureWidth = createPretextMeasure(readFont(el), pretext)
            containerWidth = el.clientWidth
        })
        document.addEventListener('mousedown', handleTooltipMouseDown, true)
    })

    onDestroy(() => {
        resizeObserver?.disconnect()
        document.removeEventListener('mousedown', handleTooltipMouseDown, true)
    })
</script>

{#if segments.length > 0}
    <span class="path-pills" bind:this={container} aria-label={path}>
        {#each layout.leading as seg, i (seg.fullPath)}
            {#if i > 0}
                <span class="sep" aria-hidden="true">/</span>
            {/if}
            <button
                type="button"
                class="pill"
                tabindex="-1"
                title={seg.fullPath}
                onclick={(e) => {
                    e.stopPropagation()
                    onPick(seg.fullPath)
                }}
            >
                {seg.label}
            </button>
        {/each}
        {#if layout.collapsed.length > 0}
            {#if layout.leading.length > 0}
                <span class="sep" aria-hidden="true">/</span>
            {/if}
            <button
                type="button"
                class="pill pill-ellipsis"
                tabindex="-1"
                aria-label={`Hidden path segments: ${layout.collapsed.map((s) => s.label).join(', ')}`}
                use:tooltip={{ html: collapsedTooltipHtml }}
                onclick={(e) => {
                    e.stopPropagation()
                }}
            >
                …
            </button>
        {/if}
        {#each layout.trailing as seg, i (seg.fullPath)}
            {#if i > 0 || layout.leading.length > 0 || layout.collapsed.length > 0}
                <span class="sep" aria-hidden="true">/</span>
            {/if}
            <button
                type="button"
                class="pill"
                tabindex="-1"
                title={seg.fullPath}
                onclick={(e) => {
                    e.stopPropagation()
                    onPick(seg.fullPath)
                }}
            >
                {seg.label}
            </button>
        {/each}
    </span>
{/if}

<style>
    .path-pills {
        display: inline-flex;
        flex-wrap: nowrap;
        align-items: center;
        gap: var(--spacing-xxs);
        min-width: 0;
        max-width: 100%;
        overflow: hidden;
    }

    .sep {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-xs);
        user-select: none;
    }

    .pill {
        background: transparent;
        border: 0;
        padding: var(--spacing-xxs) var(--spacing-xs);
        border-radius: var(--radius-sm);
        font-size: var(--font-size-xs);
        font-family: inherit;
        color: var(--color-text-tertiary);
        line-height: 1.2;
        white-space: nowrap;
        flex-shrink: 0;
        transition:
            background var(--transition-base),
            color var(--transition-base);
    }

    .pill:hover {
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
    }

    /* The collapse pill (`…`) reads as "more here", with a subtly different bg
       so the eye finds it. The tooltip lists the hidden segments. */
    .pill-ellipsis {
        background: var(--color-bg-tertiary);
        color: var(--color-text-secondary);
    }

    /* Mouse focus ring: standard 2-layer accent ring (matches the rest of the app).
       Pills aren't in Tab order, so the keyboard branch never reaches this rule;
       click-driven focus still benefits from a visible ring. */
    .pill:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 1px;
    }

    /* Styles for the hidden-pill list rendered inside the tooltip body. The
       tooltip module sets the rest of the chrome (frosted-glass surface, radius,
       shadow) on the container; we just lay out the items so they read like the
       regular pills. `:global` because the tooltip body lives in a portal. */
    :global(.cmdr-tooltip .hidden-pills) {
        display: inline-flex;
        flex-wrap: wrap;
        align-items: center;
        gap: var(--spacing-xxs);
        max-width: 360px;
    }

    :global(.cmdr-tooltip .hidden-pill) {
        background: transparent;
        border: 0;
        padding: var(--spacing-xxs) var(--spacing-xs);
        border-radius: var(--radius-sm);
        font-size: var(--font-size-xs);
        font-family: inherit;
        color: var(--color-accent-text);
        line-height: 1.2;
        white-space: nowrap;
        cursor: default;
    }

    :global(.cmdr-tooltip .hidden-pill:hover) {
        background: var(--color-accent-subtle);
        color: var(--color-text-primary);
    }

    :global(.cmdr-tooltip .hidden-sep) {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-xs);
    }
</style>
