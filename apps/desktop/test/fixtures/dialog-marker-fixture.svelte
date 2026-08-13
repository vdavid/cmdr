<!--
  Test-only stand-in for a dialog whose PRESENCE is what a test asserts.

  `DialogManager.svelte.test.ts` mocks `TransferProgressDialog` with this to
  count how many progress dialogs the real `DialogManager` renders, without
  dragging in a component that dispatches a backend operation on mount. Every
  prop is accepted and only `adoptOperationId` is echoed, so a test can tell the
  adopted arm from the dispatching one.
-->
<script lang="ts">
    const { adoptOperationId }: { adoptOperationId?: string } = $props()
</script>

<div data-testid="progress-dialog" data-adopted={adoptOperationId ?? ''}></div>
