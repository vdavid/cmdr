<!--
  Test-only stand-in for a dialog that throws while rendering.

  `DialogManager.svelte.test.ts` mocks one real dialog with this so the error
  boundary is exercised through the production component rather than a
  hand-built harness. Throwing from the instance script is exactly the shape the
  lived bug had: Svelte's `each_key_duplicate` fires during the render flush,
  before anything reaches the screen.
-->
<script lang="ts">
    throw new Error('dialog blew up while rendering')
</script>
