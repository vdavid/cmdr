<script lang="ts">
    /**
     * INFO toast shown when a "Go to path" jump landed on the nearest existing
     * ancestor because the requested path doesn't exist.
     *
     * Pure-prop-driven: every input is captured at toast-creation time and
     * never re-read. The back-shortcut in particular is snapshotted (not
     * subscribed live) so a remap that happens between the toast appearing and
     * the user reading it doesn't mutate the displayed hint mid-flight.
     */

    interface Props {
        /** The path the user typed, which doesn't exist. */
        requested: string
        /** The nearest existing ancestor we navigated to (worst case `/`). */
        landed: string
        /**
         * Display string for the effective `nav.back` shortcut, snapshotted at
         * toast-creation time. NOT reactive. Pass `''` to omit the hint line.
         */
        backShortcut: string
    }

    const { requested, landed, backShortcut }: Props = $props()
</script>

<div class="content">
    <span class="message">
        <code class="path">{requested}</code> doesn't exist, so we took you to <code class="path">{landed}</code>.
    </span>
    {#if backShortcut}
        <span class="hint">Press <kbd>{backShortcut}</kbd> to go back.</span>
    {/if}
</div>

<style>
    .content {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
        font-size: var(--font-size-sm);
    }

    .message {
        color: var(--color-text-primary);
        line-height: 1.4;
        word-break: break-all;
    }

    .path {
        font-family: var(--font-mono);
        font-size: var(--font-size-sm);
        background: none;
        padding: 0;
        color: var(--color-text-primary);
    }

    .hint {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-xs);
    }

    kbd {
        font-family: var(--font-mono);
        font-size: var(--font-size-xs);
        padding: 0 var(--spacing-xs);
        border-radius: var(--radius-xs);
        background: var(--color-bg-tertiary);
    }
</style>
