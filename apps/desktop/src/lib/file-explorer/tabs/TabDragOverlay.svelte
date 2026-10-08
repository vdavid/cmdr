<script lang="ts">
    /**
     * Everything a tab drag draws outside the bars: the ghost that follows the pointer,
     * the line marking where the tab would land, and the cursor. `DualPaneExplorer`
     * mounts one, fed by `tab-drag-controller.svelte.ts`.
     *
     * The layer covers the whole window while a drag is on. That's what keeps the rest
     * of the app still (no hover states, no tooltips waking up under the pointer), and
     * it gives the cursor one owner: "not allowed" over a bar that won't take the tab.
     */
    import type { TabDragView } from './tab-drag-controller.svelte'

    interface Props {
        view: TabDragView | null
    }

    const { view }: Props = $props()
</script>

{#if view}
    <div class="tab-drag-layer" class:refused={view.refused} aria-hidden="true">
        {#if view.line}
            <div
                class="drop-line"
                style:left="{view.line.left}px"
                style:top="{view.line.top}px"
                style:height="{view.line.height}px"
            ></div>
        {/if}
        <div
            class="ghost is-dragging"
            class:cannot-drop={view.overPane === null || view.refused}
            style:left="{view.ghost.left}px"
            style:top="{view.ghost.top}px"
            style:width="{view.ghost.width}px"
            style:height="{view.ghost.height}px"
        >
            <span class="ghost-label">{view.label}</span>
        </div>
    </div>
{/if}

<style>
    .tab-drag-layer {
        position: fixed;
        inset: 0;
        z-index: var(--z-overlay);
        cursor: default;
    }

    .tab-drag-layer.refused {
        cursor: not-allowed;
    }

    /* Centered on the slot: a 2px line, pulled back by half its width. */
    .drop-line {
        position: fixed;
        width: 2px;
        margin-left: -1px;
        background-color: var(--color-accent);
        pointer-events: none;
    }

    /* Dressed as the active tab (same surface, same accent band on top), lifted off the
       bar by a shadow so it reads as the thing in your hand. */
    .ghost {
        position: fixed;
        display: flex;
        align-items: center;
        box-sizing: border-box;
        padding: 0 var(--spacing-sm);
        background:
            linear-gradient(to bottom, var(--color-accent) 0 2px, transparent 2px), var(--color-bg-secondary);
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        font-family: var(--font-system);
        font-weight: 500;
        box-shadow: var(--shadow-md);
        pointer-events: none;
    }

    /* See-through, because the ghost sits right on the landing line and an opaque one
       hides it. `is-dragging` marks it as drag feedback for the contrast checker. */
    .ghost.is-dragging {
        opacity: 0.6;
    }

    /* Off the bars a release cancels, and a bar can refuse the tab: either way the ghost
       fades further to say it has nowhere to land. */
    .ghost.cannot-drop {
        opacity: 0.3;
    }

    .ghost-label {
        flex: 1;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        text-align: center;
    }

    @media (prefers-reduced-motion: no-preference) {
        .ghost {
            transition: opacity var(--transition-fast);
        }
    }
</style>
