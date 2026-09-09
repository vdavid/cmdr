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
    }

    const { label, text, children, size = 14 }: Props = $props()

    /**
     * The tooltip adopts THIS element, never the `hidden` wrapper around it: an adopted
     * element keeps its own attributes, so handing over the hidden host would show an
     * empty tooltip.
     */
    let contentEl = $state<HTMLDivElement>()

    const param = $derived<TooltipParam>(children ? { contentEl } : (text ?? ''))
</script>

<button type="button" class="info-tip" aria-label={label} use:tooltip={param}>
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
