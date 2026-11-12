<script lang="ts">
    /**
     * INFO toast shown when a "Go to path" jump landed on the nearest existing
     * ancestor because the requested path doesn't exist.
     *
     * The back hint is a live `nav.back` chip: it follows a rebind, and the line
     * goes away when the command is unbound.
     */

    import type { Snippet } from 'svelte'
    import ShortcutChip from '#lib/ui/ShortcutChip.svelte'
    import Trans from '#lib/intl/Trans.svelte'
    import { commandHasShortcut } from '#lib/shortcuts/reactive-shortcuts.svelte.js'

    interface Props {
        /** The path the user typed, which doesn't exist. */
        requested: string
        /** The nearest existing ancestor we navigated to (worst case `/`). */
        landed: string
    }

    const { requested, landed }: Props = $props()
    const backBound = $derived(commandHasShortcut('nav.back'))
</script>

{#snippet pathCode(children: Snippet)}
    <code class="path">{@render children()}</code>
{/snippet}
{#snippet backChip(children: Snippet)}
    <!-- The <chip></chip> tag has no inner text, so `children` renders nothing;
         it's rendered anyway to keep the snippet's tag-handler signature. -->
    {@render children()}<ShortcutChip commandId="nav.back" clickable={false} />
{/snippet}

<div class="content">
    <span class="message">
        <Trans
            key="goToPath.toast.landedOnAncestor"
            snippets={{ req: pathCode, land: pathCode }}
            params={{ requested, landed }}
        />
    </span>
    {#if backBound}
        <span class="hint"><Trans key="goToPath.toast.pressToGoBack" snippets={{ chip: backChip }} /></span>
    {/if}
</div>

<style>
    .content {
        font-size: var(--font-size-sm);
    }

    .message {
        color: var(--color-text-primary);
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
        display: flex;
        align-items: center;
        gap: var(--spacing-xxs);
    }
</style>
