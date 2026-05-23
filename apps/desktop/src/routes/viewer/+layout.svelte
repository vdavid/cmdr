<!--
  Viewer uses root layout (CSS + logger), plus a `ToastContainer` so toasts
  added by `addToast(...)` from the page (e.g. the "X bytes on your clipboard"
  copy-confirmation, save-as warnings, copy-failure surfacing) actually render
  in this window. The main route mounts its own; without one here, every toast
  the viewer adds was a silent no-op.
-->
<script lang="ts">
    import ToastContainer from '$lib/ui/toast/ToastContainer.svelte'
</script>

<slot />
<ToastContainer />
