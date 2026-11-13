<!--
  The card's next actions, in the footer beside Close: the ones Rust shows, then a "More" button holding the rest
  (`cmdr_about::actions`). Pure rendering: which actions exist, their order, and the split are the wire's. ❌ Nothing
  here focuses an action on open: the card's body takes focus, and Move to trash is never first anyway.

  Every action leaves the card, so `onAct` closes it first (`actFromCard`): focus lands where the action went.
-->
<script lang="ts">
    import { onDestroy } from 'svelte'
    import type { AboutAction, Actions } from '#lib/ipc/bindings.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import Button from '#lib/ui/Button.svelte'
    import Icon from '#lib/ui/Icon.svelte'
    import Menu from '#lib/ui/Menu.svelte'
    import { createMenu } from '#lib/ui/menu-controller.svelte.js'
    import type { MenuItem } from '#lib/ui/menu-types.js'
    import { actionLabel, actionTooltip } from './about-actions'

    interface Props {
        actions: Actions | null
        onAct: (action: AboutAction) => void
    }

    const { actions, onAct }: Props = $props()

    const shown = $derived(actions?.shown ?? [])
    const more = $derived(actions?.more ?? [])

    let moreButton: HTMLElement | undefined = $state()

    const menu = createMenu<AboutAction>({
        getSections: () => [
            {
                id: 'more',
                items: more.map(
                    (action): MenuItem<AboutAction> => ({
                        value: action.type,
                        label: actionLabel(action),
                        tooltip: actionTooltip(action) ?? undefined,
                        data: action,
                    }),
                ),
            },
        ],
        onSelect: (item) => {
            if (item.data) onAct(item.data)
        },
        restoreFocus: () => {
            moreButton?.querySelector('button')?.focus()
        },
    })

    onDestroy(() => {
        menu.destroy()
    })

    function toggleMore(): void {
        const button = moreButton?.querySelector('button')
        if (button) menu.toggleUnder(button)
    }
</script>

<div class="actions" data-about-actions>
    {#each shown as action (action.type)}
        {@const tooltip = actionTooltip(action)}
        <span class="action" data-action={action.type}>
            <Button
                variant="secondary"
                tooltipContent={tooltip ?? undefined}
                onclick={() => {
                    onAct(action)
                }}>{actionLabel(action)}</Button
            >
        </span>
    {/each}
    {#if more.length > 0}
        <span class="action" data-action="more" bind:this={moreButton}>
            <Button variant="secondary" onclick={toggleMore}>
                <span class="more-label"
                    >{tString('aboutThis.action.more')}<Icon name="chevron-down" size={14} aria-hidden="true" /></span
                >
            </Button>
        </span>
    {/if}
</div>

<Menu {menu} ariaLabel={tString('aboutThis.action.moreMenu')} minWidth={200} />

<style>
    .actions {
        display: flex;
        flex: 1 1 auto;
        flex-wrap: nowrap;
        gap: var(--spacing-md);
        min-width: 0;
    }

    /* ❌ No shrinking or ellipsis: a label is whole or the row drops below (the footer's own rule). */
    .action {
        display: inline-flex;
        flex: none;
        white-space: nowrap;
    }

    .more-label {
        display: inline-flex;
        gap: var(--spacing-xxs);
        align-items: center;
    }
</style>
