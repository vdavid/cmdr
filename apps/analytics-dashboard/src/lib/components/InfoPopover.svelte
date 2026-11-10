<!--
  A small (i) next to a chart title or table header that explains the metric: what it is, where it comes
  from, and how far to trust it. Opens on hover, on keyboard focus, and on click/tap (which pins it open
  until a second tap, Escape, or a click elsewhere), so it works with a mouse, a keyboard, and a phone.
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
        role="note"
        class="absolute top-6 z-30 w-72 max-w-[calc(100vw-2rem)] rounded-lg border border-border bg-surface-elevated p-3 text-left text-xs/relaxed font-normal whitespace-normal text-text-secondary shadow-lg
            {align === 'end' ? 'right-0' : 'left-0'}
            {open ? 'block' : 'hidden'}"
    >
        {@render children()}
    </span>
</span>
