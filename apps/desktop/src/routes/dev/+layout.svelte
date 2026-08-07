<script lang="ts">
    /**
     * Page chrome for the standalone `/dev/*` catalog routes (Components,
     * Graphics).
     *
     * These pages are ALSO imported as plain components by the Debug window,
     * which wraps them in its own padded content area. A SvelteKit layout applies
     * to the ROUTE only, never to a component import, so this is where the
     * standalone pages get a frame without double-padding the embedded ones.
     *
     * The top inset is the load-bearing part: a `/dev/*` route has no title bar of
     * its own, so with no padding the first row of content sits under the macOS
     * traffic lights, which the OS draws over the webview's top-left corner.
     */
</script>

<div class="dev-route">
    <slot />
</div>

<style>
    .dev-route {
        padding: var(--spacing-lg);
        padding-top: calc(var(--titlebar-height) + var(--spacing-sm));
    }
</style>
