<!--
  Test-only host for `DialogBoundary`, in the shape every route uses: the
  boundary wraps the `{#if}` that mounts the dialog, beside a marker that proves
  the rest of the window survived.

  `view` is a `$state` object the test owns, so it can open, close, and swap the
  dialog between flushes. `onClose` is the test's own, so it decides whether
  "closing" really closes (the re-arm cap test keeps it open on purpose).
-->
<script lang="ts">
    import type { Component } from 'svelte'
    import DialogBoundary from '#lib/ui/boundary/DialogBoundary.svelte'

    interface Props {
        view: { open: boolean; dialog: Component }
        onClose: () => void
    }

    const { view, onClose }: Props = $props()
    const Dialog = $derived(view.dialog)
</script>

<p data-testid="window-alive">The window is still here</p>
<DialogBoundary name="fixture" {onClose}>
    {#if view.open}
        <Dialog />
    {/if}
</DialogBoundary>
