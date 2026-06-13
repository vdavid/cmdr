<!--
  The grouped, read-only shortcut list for the Keyboard shortcuts help window.

  One `SectionCard` per `CommandScope` (reusing the Settings editor's grouping
  and order), one row per command. Each row diffs the command's default vs
  current bindings via `diffShortcuts`: matching keys render as neutral chips,
  user-added/replaced keys as bold green "Added" chips, and turned-off defaults
  as dimmed, struck "Disabled" chips. It's read-only; editing lives in Settings.

  Live sync: every shortcut mutation (including cross-window edits from the
  Settings window, which ride the `shortcuts:changed` event) fires
  `onShortcutChange`, bumping a counter that re-derives the groups and re-keys
  the rows so their `{@const}` diffs recompute.
-->
<script lang="ts">
    import { tooltip } from '$lib/tooltip/tooltip'
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import { commands } from '$lib/commands/command-registry'
    import { getDefaultShortcuts, getEffectiveShortcuts, onShortcutChange } from '$lib/shortcuts'
    import { groupCommandsByScope } from '$lib/settings/sections/keyboard-shortcuts-grouping'
    import { diffShortcuts } from './shortcut-diff'

    interface Props {
        /** When true, hide commands that have no active (effective) shortcut right now. */
        hideEmpty: boolean
    }

    const { hideEmpty }: Props = $props()

    // `getEffectiveShortcuts` / `getDefaultShortcuts` are plain (non-reactive)
    // reads, so we re-derive on this counter, bumped by every shortcut change.
    let shortcutChangeCounter = $state(0)
    $effect(() =>
        onShortcutChange(() => {
            shortcutChangeCounter++
        }),
    )

    const groups = $derived.by(() => {
        void shortcutChangeCounter
        const visible = hideEmpty ? commands.filter((c) => getEffectiveShortcuts(c.id).length > 0) : commands
        return groupCommandsByScope(visible)
    })
</script>

{#each groups as group (group.scope)}
    <SectionCard label={group.title}>
        <!-- Re-key on the counter so each row's `{@const}` diff recomputes on a rebind. -->
        {#each group.commands as command (`${command.id}-${String(shortcutChangeCounter)}`)}
            {@const chips = diffShortcuts(getDefaultShortcuts(command.id), getEffectiveShortcuts(command.id))}
            <div class="row">
                <span class="name">{command.name}</span>
                <div class="chips">
                    {#if chips.length === 0}
                        <span class="none">No shortcut</span>
                    {:else}
                        {#each chips as chip (chip.key + chip.status)}
                            {#if chip.status === 'added'}
                                <kbd class="chip added" use:tooltip={'Added'}>{chip.key}</kbd>
                            {:else if chip.status === 'disabled'}
                                <kbd class="chip disabled" use:tooltip={'Disabled'}>{chip.key}</kbd>
                            {:else}
                                <kbd class="chip">{chip.key}</kbd>
                            {/if}
                        {/each}
                    {/if}
                </div>
            </div>
        {/each}
    </SectionCard>
{/each}

<style>
    .row {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--spacing-md);
        padding: var(--spacing-xs) 0;
        border-bottom: 1px solid var(--color-border-subtle);
    }

    .row:last-child {
        border-bottom: none;
    }

    .name {
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
        min-width: 0;
    }

    .chips {
        display: flex;
        flex-wrap: wrap;
        justify-content: flex-end;
        gap: var(--spacing-xs);
        flex-shrink: 0;
    }

    .chip {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        padding: var(--spacing-xxs) var(--spacing-sm);
        background: var(--color-bg-tertiary);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        font-family: var(--font-system) sans-serif;
        font-size: var(--font-size-xs);
        color: var(--color-text-primary);
        white-space: nowrap;
    }

    /* User-added or -replaced binding: stands out as a deliberate change.
       `--color-shortcut-added` is the themed AA-safe positive green. */
    .chip.added {
        color: var(--color-shortcut-added);
        border-color: color-mix(in srgb, var(--color-shortcut-added) 45%, var(--color-border));
        background: color-mix(in srgb, var(--color-shortcut-added) 12%, transparent);
        font-weight: 600;
    }

    /* Shipped default the user turned off: shown struck-through and dimmed so
       it reads as "this one no longer fires". */
    .chip.disabled {
        color: var(--color-text-tertiary);
        border-style: dashed;
        background: transparent;
        text-decoration: line-through;
    }

    .none {
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }
</style>
