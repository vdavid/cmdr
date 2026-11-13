<!--
  A path on the card: cut with an ellipsis when it doesn't fit, its full text in a tooltip, and a quiet
  Copy. A text node, ❌ never markup: paths come from the item and its history.
-->
<script lang="ts">
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import CopyButton from '#lib/ui/CopyButton.svelte'
    import { copiedPath, displayPath } from '#lib/utils/display-path.js'

    interface Props {
        path: string
        /** The copy button's accessible name ("Copy where it came from"). */
        copyLabel: string
    }

    const { path, copyLabel }: Props = $props()
    const shown = $derived(displayPath(path))
</script>

<span class="path-line"
    ><span class="path" use:tooltip={shown}>{shown}</span><CopyButton text={copiedPath(path)} label={copyLabel} /></span
>

<style>
    .path-line {
        display: inline-flex;
        max-width: 100%;
        gap: var(--spacing-xs);
        align-items: center;
        vertical-align: bottom;
    }

    .path {
        min-width: 0;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }
</style>
