<!--
  Test-only host for the Ask Cmdr rail's boundary, in the exact shape
  `routes/(main)/+page.svelte` mounts it: a `DialogBoundary` above the
  `{#if askCmdrState.open}`, closing through the real `closeRail`, beside a
  marker that proves the rest of the window survived.

  `rail` stands in for `AskCmdrRail`, so a test can mount a child that throws
  while it renders or while it tears down.
-->
<script lang="ts">
    import type { Component } from 'svelte'
    import DialogBoundary from '#lib/ui/boundary/DialogBoundary.svelte'
    import { askCmdrState, closeRail } from '#lib/ask-cmdr/ask-cmdr-trigger.svelte.js'
    import { tString } from '#lib/intl/messages.svelte.js'

    interface Props {
        view: { rail: Component }
    }

    const { view }: Props = $props()
    const Rail = $derived(view.rail)
</script>

<p data-testid="window-alive">The window is still here</p>
<DialogBoundary name="askCmdrRail" onClose={closeRail} closedMessage={tString('askCmdr.railBoundary.closedToast')}>
    {#if askCmdrState.open}
        <Rail />
    {/if}
</DialogBoundary>
