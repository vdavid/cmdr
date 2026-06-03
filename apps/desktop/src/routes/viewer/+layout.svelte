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

<!--
  The viewer's title bar is the `ViewerToolbar`, which is taller than the main
  window's. Override `--titlebar-height` for the whole viewer subtree so the
  toolbar AND every modal backdrop in this window (`ViewerCopyDialogs`) read the
  same height: backdrops start exactly below the toolbar, keeping the OS
  window-drag region live while a dialog is open. This is the single source of
  truth for the viewer's bar height — `ViewerToolbar` sizes itself to it.
  `display: contents` sets the var without adding a layout box.
-->
<div class="viewer-window-root">
    <slot />
    <ToastContainer />
</div>

<style>
    .viewer-window-root {
        display: contents;

        --titlebar-height: 35px;
    }
</style>
