<!--
  Test-only stand-in for a component that throws while it tears down.

  The boundary tests mount it inside each kind of boundary, then close the
  `{#if}` that holds it. That's the lived shape (ERR: `derived_references_self`
  as the Multi-rename sheet closed, which blanked the main window): Svelte
  routes a teardown throw to the nearest LIVE ancestor boundary, so only a
  boundary sitting above the `{#if}` can catch it.
-->
<script lang="ts">
    $effect(() => () => {
        throw new Error('component blew up while tearing down')
    })
</script>

<div data-testid="teardown-throw">Closing me throws</div>
