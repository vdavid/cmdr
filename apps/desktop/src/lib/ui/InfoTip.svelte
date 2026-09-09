<script lang="ts">
    /**
     * An info glyph that parks a long explanation behind a tooltip. Reach for it when a
     * label needs a sentence (or three) of context that would bloat the surface if it were
     * written inline: onboarding's toggles lead with a half-line and keep the rest here.
     *
     * Two ways to give it a body: `text` for a plain string, or a `children` snippet for
     * real paragraphs and lists. The snippet renders into a hidden host and is handed to
     * the tooltip as a live `contentEl`, so the markup and its styling stay the caller's.
     *
     * It's a `<button>`, so the body is one Tab away as well as one hover: the tooltip
     * action opens on focus too, which a native `title` never does.
     */
    import type { Snippet } from 'svelte'
    import Icon from './Icon.svelte'
    import { tooltip, type TooltipParam } from '$lib/tooltip/tooltip'

    interface Props {
        /** Accessible name for the glyph. It carries no visible text, so this is required. */
        label: string
        /** Plain-text body. Ignored when `children` is given. */
        text?: string
        /** Rich body: real `<p>` / `<ol>` markup, styled by the caller. */
        children?: Snippet
        /** Glyph size in px. Match the text it sits beside. */
        size?: number
        /**
         * Where the glyph sits relative to what it explains.
         *
         * - `inline` (the default): it follows inline text, and a length `vertical-align`
         *   drops it onto that text's optical middle.
         * - `radio-row`: it's a `RadioGroup`'s `itemTrailing` slot, whose row is a flex
         *   container as tall as a label plus its description. Centring against the whole
         *   row leaves the glyph hanging below the words it belongs to, so this variant
         *   gives it `RadioGroup`'s own label line box and item padding and top-aligns it,
         *   putting it on the LABEL's line. ❌ Don't reach for it in a row whose first line
         *   isn't `--font-size-sm` text with `--spacing-xs` above it; the numbers are
         *   `RadioGroup`'s.
         */
        align?: 'inline' | 'radio-row'
    }

    const { label, text, children, size = 14, align = 'inline' }: Props = $props()

    /**
     * The tooltip adopts THIS element, never the `hidden` wrapper around it: an adopted
     * element keeps its own attributes, so handing over the hidden host would show an
     * empty tooltip.
     */
    let contentEl = $state<HTMLDivElement>()

    const param = $derived<TooltipParam>(children ? { contentEl } : (text ?? ''))
</script>

<button
    type="button"
    class="info-tip"
    class:radio-row={align === 'radio-row'}
    aria-label={label}
    use:tooltip={param}
>
    <Icon name="info" {size} aria-hidden="true" />
</button>
{#if children}
    <div hidden>
        <div bind:this={contentEl} class="info-tip-content">{@render children()}</div>
    </div>
{/if}

<style>
    .info-tip {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        /* It always follows something it belongs to, and a word space alone leaves it
           crowding the last letter. Small enough that a flex row's own gap stays in charge. */
        margin-left: var(--spacing-xs);
        /* Drops the glyph onto the optical middle of the line it follows. ❌ Padding can't do
           this: an `inline-flex` box takes its baseline from its first item, and an `<svg>`
           has no text baseline, so the browser synthesizes one from the box's bottom edge —
           padding then grows the box and the line box slides it right back up, for no visible
           change at all. A length `vertical-align` moves the box itself, which is the only
           thing that shifts the glyph. Inert wherever the tip is a flex item (settings rows),
           where the container's own alignment is in charge. */
        vertical-align: calc(-1 * var(--spacing-xxs));
        padding: 0;
        border: none;
        background: transparent;
        color: var(--color-text-tertiary);
        transition: color var(--transition-base);
    }

    /* A `RadioGroup` row is a flex container, so `vertical-align` above is already inert
       here and the container's alignment is in charge. Top-aligning alone would still miss:
       the row is as tall as a label plus its description, so the glyph needs the label's own
       line box (`RadioGroup`'s `--font-size-sm`) and the item's own vertical padding to land
       on the words rather than a couple of pixels under them. `content-box` keeps that
       padding outside the line box instead of eating it. */
    .info-tip.radio-row {
        align-self: flex-start;
        flex: none;
        margin-left: 0;
        height: calc(var(--font-size-sm) * var(--font-line-height-prose));
        padding: var(--spacing-xs) 0;
        box-sizing: content-box;
    }

    .info-tip:hover {
        color: var(--color-text-primary);
    }

    .info-tip:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 2px;
        border-radius: var(--radius-xs);
    }

    /* The tooltip's own box is narrow, so the body column just fills it; the paragraph and
       list rhythm inside comes from whoever owns the snippet. */
    .info-tip-content {
        display: block;
    }
</style>
