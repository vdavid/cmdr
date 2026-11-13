<!--
  A path on the card: cut with an ellipsis when it doesn't fit, its full text in a tooltip. A text node, ❌ never
  markup: paths come from the item and its history. Its row puts the Copy button at the end of the line.
-->
<script lang="ts">
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import { displayPath } from '#lib/utils/display-path.js'

    interface Props {
        path: string
    }

    const { path }: Props = $props()
    const shown = $derived(displayPath(path))
</script>

<span class="path" use:tooltip={shown}>{shown}</span>

<style>
    .path {
        display: inline-block;
        max-width: 100%;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        vertical-align: bottom;
    }
</style>
