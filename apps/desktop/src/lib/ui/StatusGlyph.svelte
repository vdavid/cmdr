<script lang="ts">
    /**
     * A small glyph that marks a condition on the thing it sits beside: a TCC-restricted
     * folder in a file row, a volume in the breadcrumb, a symlinked subtree in the status
     * bar. `InfoTip`'s non-interactive twin.
     *
     * ❌ It is a `<span>`, never a `<button>`, and that's the whole point of it being a
     * separate component. These live in recycled virtual-scroll rows, and one tab stop per
     * row would wreck the keyboard model of a file list with 10,000 entries. If the glyph
     * should be reachable and openable, you want `InfoTip`.
     *
     * `label` is what it means. It becomes the accessible name (the glyph carries no text)
     * and, unless `showTooltip` is off, the hover explanation too: a marker's meaning and
     * its explanation are the same sentence.
     */
    import Icon from './Icon.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import type { IconName } from './icons/icon-map'

    interface Props {
        /** Which glyph. Registered in `icons/icon-map.ts`, like every other glyph. */
        name: IconName
        /** What the condition is, in one sentence. Required: the glyph carries no text. */
        label: string
        /**
         * Off when something around the marker ALREADY carries the same tooltip, so the two
         * don't stack. The breadcrumb's whole volume row does, and the row is the honest
         * target there: the italic dimmed label needs the explanation as much as the glyph.
         * The accessible name stays either way.
         */
        showTooltip?: boolean
    }

    const { name, label, showTooltip = true }: Props = $props()
</script>

<!-- Size is fixed, ❌ not a prop: these markers sit in the app's small-text surfaces (file
     rows, the status bar, the breadcrumb), and four sites drifting to four sizes is what the
     component exists to stop. A surface that needs another size changes it here, on purpose. -->
<span class="status-glyph" role="img" aria-label={label} use:tooltip={showTooltip ? label : ''}>
    <Icon {name} size={12} />
</span>

<style>
    .status-glyph {
        display: inline-flex;
        align-items: center;
        margin-left: var(--spacing-xxs);
        /* The glyph inherits the row's color and dims relative to it, so it brightens along
           with a selected or cursor row instead of staying a fixed gray on a highlighted
           one. Gotcha: `pnpm check a11y-contrast` can't fold `opacity` into a computed
           color, so it reads this glyph at full strength and won't warn if the dimming ever
           takes it under the non-text floor. Keep the number where it is. */
        opacity: 0.7;
        /* An `inline-flex` box has no text baseline, so the browser synthesizes one at its
           bottom edge; `text-bottom` is what lands a 12px glyph on the bottom of the line it
           follows. Inert in the breadcrumb, where the marker is a flex item. */
        vertical-align: text-bottom;
        /* The breadcrumb's volume row is a tight flex line that would otherwise squeeze it. */
        flex-shrink: 0;
    }
</style>
