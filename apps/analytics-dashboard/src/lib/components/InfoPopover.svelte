<!--
  A small (i) next to a chart title or table header that explains the metric: what it is, where it comes
  from, and how far to trust it. Opens on hover, on keyboard focus, and on click/tap (which pins it open
  until a second tap, Escape, or a click elsewhere), so it works with a mouse, a keyboard, and a phone.

  Write the content as a glanceable stack, never one paragraph: a first <p> with the gist (drawn brighter),
  then short <p>s and <ul>s, each block optionally led by <span class="info-label">Source:</span> (or
  "Caveats:", "Targets:"). The panel spaces the blocks and bullets the lists.
-->
<script lang="ts">
    import type { Snippet } from 'svelte'

    interface Props {
        /** What the popover explains, for the button's accessible name ("About visitors"). */
        topic: string
        children: Snippet
        /** Which edge of the icon the panel aligns to; use 'end' near the right edge of a card. */
        align?: 'start' | 'end'
    }

    const { topic, children, align = 'start' }: Props = $props()

    const id = $props.id()
    let hovered = $state(false)
    let focused = $state(false)
    let pinned = $state(false)
    const open = $derived(hovered || focused || pinned)

    let root: HTMLSpanElement
    let panel: HTMLSpanElement

    /**
     * How far (px) to slide the panel sideways so it stays 16px inside the viewport: an (i) near a narrow
     * screen's edge would otherwise push its panel off-screen.
     */
    let shift = $state(0)
    $effect(() => {
        if (!open) return
        const anchor = root.getBoundingClientRect()
        const width = panel.offsetWidth
        const natural = align === 'end' ? anchor.right - width : anchor.left
        const clamped = Math.max(16, Math.min(natural, window.innerWidth - 16 - width))
        shift = clamped - natural
    })

    function onDocumentPointerDown(e: PointerEvent) {
        if (pinned && !root.contains(e.target as Node)) pinned = false
    }

    function onKeydown(e: KeyboardEvent) {
        if (e.key === 'Escape' && open) {
            pinned = false
            hovered = false
            focused = false
        }
    }
</script>

<svelte:document onpointerdown={onDocumentPointerDown} />

<span
    bind:this={root}
    class="relative inline-flex align-middle"
    role="presentation"
    onmouseenter={() => (hovered = true)}
    onmouseleave={() => (hovered = false)}
    onkeydown={onKeydown}
>
    <button
        type="button"
        class="inline-flex size-4 items-center justify-center rounded-full border border-text-tertiary/60 text-[10px] leading-none font-semibold text-text-tertiary hover:border-text-secondary hover:text-text-secondary focus-visible:ring-2 focus-visible:ring-accent focus-visible:outline-none"
        aria-label="About {topic}"
        aria-expanded={open}
        aria-controls={id}
        onclick={() => (pinned = !pinned)}
        onfocus={() => (focused = true)}
        onblur={() => (focused = false)}
    >
        i
    </button>
    <span
        {id}
        bind:this={panel}
        role="note"
        style:translate="{shift}px 0"
        class="info-panel absolute top-6 z-30 w-80 max-w-[calc(100vw-2rem)] rounded-lg border border-border bg-surface-elevated p-3 text-left text-xs/relaxed font-normal tracking-normal whitespace-normal text-text-secondary normal-case shadow-lg
            {align === 'end' ? 'right-0' : 'left-0'}
            {open ? 'block' : 'hidden'}"
    >
        {@render children()}
    </span>
</span>

<style>
    /* Short blocks (the gist first, then <p>s and <ul>s), spaced so a glance finds each one. */
    .info-panel > :global(p:first-child) {
        color: var(--color-text-primary);
    }

    .info-panel > :global(* + *) {
        margin-top: 0.5rem;
    }

    .info-panel :global(ul) {
        padding-left: 1rem;
        list-style: disc;
    }

    .info-panel :global(li + li) {
        margin-top: 0.125rem;
    }

    .info-panel :global(li::marker) {
        color: var(--color-text-tertiary);
    }

    .info-panel :global(.info-label) {
        font-weight: 500;
        color: var(--color-text-primary);
    }
</style>
