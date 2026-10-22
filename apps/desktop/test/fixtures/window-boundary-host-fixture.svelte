<!--
  Test-only host for `WindowBoundary`, in the shape the root layout uses: the
  boundary wraps everything the window shows. `view` is a `$state` object the
  test owns: `content` is what the window renders, `open` mounts or unmounts it.
-->
<script lang="ts">
    import type { Component } from 'svelte'
    import WindowBoundary from '#lib/error-reporter/WindowBoundary.svelte'

    interface Props {
        view: { open: boolean; content: Component }
    }

    const { view }: Props = $props()
    const Content = $derived(view.content)
</script>

<WindowBoundary>
    {#if view.open}
        <Content />
    {/if}
</WindowBoundary>
