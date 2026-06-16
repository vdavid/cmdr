<!--
  ShortcutChip: the one component that renders a keyboard shortcut in the UI.

  Two mutually exclusive modes:

    <ShortcutChip commandId="downloads.goToLatest" />   Dynamic: live effective first
                                                        shortcut, clickable by default.
    <ShortcutChip key="⏎" />                            Literal: a fixed key, never
                                                        clickable. Also used for toast
                                                        snapshots.

  Truthfulness rule: a `commandId` chip is a CLAIM about live app behavior ("pressing
  this does X"), so it reads the reactive store and renders NOTHING when the command has
  no binding (callers in prose must conditionalize the surrounding sentence). A `key`
  chip is just typography.

  Lazy-import constraint: the deep-link helper (`openShortcutCustomization`) pulls in
  `@tauri-apps/api/webviewWindow` and friends. A literal-mode chip ships in the
  capability-restricted viewer window, which has no window-creation permission, so the
  helper must NOT be statically imported. It's loaded via dynamic `import()` inside the
  click handler only. Don't turn this into a static import.
-->
<script lang="ts">
    import { commands, type CommandId } from '$lib/commands'
    import { getFirstShortcutReactive } from '$lib/shortcuts/reactive-shortcuts.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        /**
         * Dynamic mode: render the command's first effective shortcut, reactively.
         * Renders nothing when the command has no binding. Exactly one of
         * `commandId` / `key` must be set.
         */
        commandId?: CommandId
        /** Literal mode: a fixed key string. Never clickable. */
        key?: string
        /**
         * Whether a `commandId` chip opens Settings on click. Default `true` in
         * `commandId` mode, ignored (always non-clickable) in literal mode. Set
         * `false` when the chip sits inside another interactive control (palette
         * rows, F-key bar buttons) where a nested click target would double-activate.
         */
        clickable?: boolean
        /**
         * Visual density. `md` (default) is the standalone pill. `sm` tightens the
         * padding and corner radius for dense rows where several chips sit side by
         * side (the command palette caps at three).
         */
        size?: 'sm' | 'md'
    }

    const { commandId, key, clickable = true, size = 'md' }: Props = $props()

    if ((commandId === undefined) === (key === undefined)) {
        throw new Error('ShortcutChip: set exactly one of `commandId` or `key`')
    }

    const commandName = $derived(commandId ? (commands.find((c) => c.id === commandId)?.name ?? '') : '')
    // Reactive: re-reads when the user rebinds. In literal mode this stays the fixed key.
    const value = $derived(commandId ? getFirstShortcutReactive(commandId) : key)

    // Clickable only in `commandId` mode (a literal key has no command to customize).
    const isClickable = $derived(commandId !== undefined && clickable)

    async function handleClick(): Promise<void> {
        if (commandId === undefined) return
        // Lazy import keeps the Tauri window-creation surface out of the viewer bundle.
        const { openShortcutCustomization } = await import('$lib/settings/settings-window')
        await openShortcutCustomization(commandId)
    }
</script>

{#if value}
    {#if isClickable}
        <button
            type="button"
            class="shortcut-chip clickable"
            class:sm={size === 'sm'}
            aria-label={tString('ui.shortcutChip.customizeAria', { commandName })}
            onclick={handleClick}
            use:tooltip={tString('ui.shortcutChip.customizeTooltip')}
        >
            <kbd>{value}</kbd>
        </button>
    {:else}
        <kbd class="shortcut-chip" class:sm={size === 'sm'}>{value}</kbd>
    {/if}
{/if}

<style>
    /* Neutral pill modeled on the Settings `.shortcut-pill`, not the tooltip's accent
       chip (accent-on-glass is right in the dark tooltip but too loud repeated across
       the main UI). */
    .shortcut-chip {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        padding: var(--spacing-xxs) var(--spacing-sm);
        background: var(--color-bg-tertiary);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        font-family: var(--font-system);
        font-size: var(--font-size-xs);
        color: var(--color-text-primary);
        white-space: nowrap;
    }

    /* Dense variant: tighter padding + corner radius so several chips fit a row
       (the command palette shows up to three). */
    .shortcut-chip.sm {
        padding: 0 var(--spacing-xs);
        border-radius: var(--radius-xs);
    }

    /* The clickable variant is a real <button> wrapping the <kbd>. Reset the button
       chrome so it reads as the same pill; the inner <kbd> carries no border of its own. */
    button.shortcut-chip {
        margin: 0;
        font: inherit;
    }

    button.shortcut-chip > kbd {
        font: inherit;
        color: inherit;
        background: none;
        border: none;
        padding: 0;
    }

    /* Hover signals interactivity with accent border + text. Cursor stays `default`
       per the app-wide convention (only LinkButton opts into `cursor: pointer`). */
    .shortcut-chip.clickable:hover {
        border-color: var(--color-accent);
        color: var(--color-accent-text);
    }

    .shortcut-chip.clickable:focus-visible {
        outline: none;
        box-shadow: var(--shadow-focus);
    }
</style>
