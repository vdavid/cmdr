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

    import type { Snippet } from 'svelte'
    import ShortcutChip from '$lib/ui/ShortcutChip.svelte'
    import Trans from '$lib/intl/Trans.svelte'

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

{#snippet pathCode(children: Snippet)}
    <code class="path">{@render children()}</code>
{/snippet}
{#snippet backChip(children: Snippet)}
    <!-- The <chip></chip> tag has no inner text, so `children` renders nothing;
         it's rendered anyway to keep the snippet's tag-handler signature. -->
    {@render children()}<ShortcutChip key={backShortcut} />
{/snippet}

<div class="content">
    <span class="message">
        <Trans
            key="goToPath.toast.landedOnAncestor"
            snippets={{ req: pathCode, land: pathCode }}
            params={{ requested, landed }}
        />
    </span>
    {#if backShortcut}
        <span class="hint"><Trans key="goToPath.toast.pressToGoBack" snippets={{ chip: backChip }} /></span>
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
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xxs);
    }
</style>
