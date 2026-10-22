<!--
  Test-only host for `PaneBoundary`, in the shape `DualPaneExplorer` uses: the
  boundary wraps the per-tab `{#key}` that remounts the pane, beside a marker
  standing in for the other pane, which has to keep working.

  `view` is a `$state` object the test owns: `pane` is what the left pane
  renders, `open` mounts or unmounts it, and `tabId` is the active tab.
-->
<script lang="ts">
    import type { Component } from 'svelte'
    import PaneBoundary from '#lib/file-explorer/pane/PaneBoundary.svelte'

    interface Props {
        view: { open: boolean; pane: Component; tabId: string }
    }

    const { view }: Props = $props()
    const Pane = $derived(view.pane)
</script>

<div data-testid="left-pane">
    <PaneBoundary paneId="left" resetKey={view.tabId}>
        {#key view.tabId}
            {#if view.open}
                <Pane />
            {/if}
        {/key}
    </PaneBoundary>
</div>
<p data-testid="other-pane">The other pane is still here</p>
