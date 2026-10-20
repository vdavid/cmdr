<script lang="ts">
    /**
     * Lays the inline rename field over its row from OUTSIDE the listbox. ARIA makes an option's
     * children presentational, so a text field in the row is `nested-interactive` (VoiceOver may
     * read it as part of one flat option), and a listbox may own only options, so the field can't
     * sit between the rows either. So it lives in the list's scroll container, as the listbox's
     * sibling, absolutely placed over `anchor`: the row's empty name slot.
     *
     * Scroll-content coordinates, so scrolling carries the field with its row for free. What else
     * can move the slot is watched, never polled: a resize (the slot or the scroller), a DOM change
     * in the listbox (a watcher diff shifting rows, the virtual window's offset), and a running CSS
     * transition (a column width animating), followed frame by frame only while one runs.
     *
     * The host renders it once per anchor (`{#key}`), so a new row means a new editor, exactly as
     * when the field lived in the row; an anchor that unmounts (its row left the virtual window)
     * takes the field with it, which blurs it and discards the edit (`DETAILS.md` § Ending a rename
     * session).
     */
    import type { Snippet } from 'svelte'

    interface Props {
        /** The row's empty name slot; the field takes its box. */
        anchor: HTMLElement
        /** The list's scroll container (`position: relative`), this overlay's parent. */
        scroller: HTMLElement
        children: Snippet
    }

    const { anchor, scroller, children }: Props = $props()

    /** Longer than the lists' own transitions (Brief's column width, Full's grid columns: 300 ms). */
    const TRANSITION_FOLLOW_MS = 400

    interface Box {
        top: number
        left: number
        width: number
        height: number
    }

    let box = $state<Box | null>(null)

    function sameBox(a: Box | null, b: Box): boolean {
        return a !== null && a.top === b.top && a.left === b.left && a.width === b.width && a.height === b.height
    }

    $effect(() => {
        const slot = anchor
        const host = scroller
        const listbox = slot.closest('[role="listbox"]') ?? host
        let frame = 0
        let followUntil = 0
        // The last box written, kept off `$state` so reading it doesn't make this effect re-run.
        let placed: Box | null = null

        function place(): void {
            const a = slot.getBoundingClientRect()
            const s = host.getBoundingClientRect()
            const next: Box = {
                top: a.top - s.top - host.clientTop + host.scrollTop,
                left: a.left - s.left - host.clientLeft + host.scrollLeft,
                width: a.width,
                height: a.height,
            }
            // Only a real move writes: the overlay's own style change must not feed the observers.
            if (sameBox(placed, next)) return
            placed = next
            box = next
        }

        function schedule(): void {
            if (frame !== 0) return
            frame = requestAnimationFrame(() => {
                frame = 0
                place()
                if (performance.now() < followUntil) schedule()
            })
        }

        function followTransition(): void {
            followUntil = Math.max(followUntil, performance.now() + TRANSITION_FOLLOW_MS)
            schedule()
        }

        place()
        const resizes = new ResizeObserver(schedule)
        resizes.observe(slot)
        resizes.observe(host)
        const mutations = new MutationObserver(schedule)
        mutations.observe(listbox, { subtree: true, childList: true, attributes: true, attributeFilter: ['style', 'class'] })
        listbox.addEventListener('transitionrun', followTransition)
        listbox.addEventListener('transitionend', schedule)
        listbox.addEventListener('transitioncancel', schedule)
        return () => {
            cancelAnimationFrame(frame)
            resizes.disconnect()
            mutations.disconnect()
            listbox.removeEventListener('transitionrun', followTransition)
            listbox.removeEventListener('transitionend', schedule)
            listbox.removeEventListener('transitioncancel', schedule)
        }
    })
</script>

<!-- Hidden until placed, so the field never flashes at the scroller's corner. -->
<div
    class="rename-overlay"
    style:top={box ? `${String(box.top)}px` : undefined}
    style:left={box ? `${String(box.left)}px` : undefined}
    style:width={box ? `${String(box.width)}px` : undefined}
    style:height={box ? `${String(box.height)}px` : undefined}
    style:visibility={box ? undefined : 'hidden'}
>
    {@render children()}
</div>

<style>
    .rename-overlay {
        position: absolute;
        display: flex;
    }
</style>
