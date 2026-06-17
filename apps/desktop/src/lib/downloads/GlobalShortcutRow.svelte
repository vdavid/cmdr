<script lang="ts">
    /**
     * The global go-to-latest-download hotkey, shown as a shortcut row inside
     * the Keyboard shortcuts section. This is where users edit the combo —
     * it's a system-wide hotkey, so it lives alongside the other shortcuts but
     * is marked `(global)` to set expectations (it fires from any app, and its
     * on/off switch lives under Behavior → File system watching).
     *
     * Renders only the row itself. The "Global" card heading is owned by the
     * `SectionCard` that wraps this component in `KeyboardShortcutsSection`, so
     * this component must NOT render its own heading (a second `<h3>` would
     * duplicate the card label and break heading order).
     *
     * Why this is a bespoke row, not a registry-`commands`-driven one:
     *  - The binding's home is `settings.json` (the Rust startup/focus refresh
     *    reads it from there before any window loads), not `shortcuts.json`.
     *  - A global Carbon hotkey doesn't go through the in-app keydown dispatch,
     *    has no in-app scope, and can't meaningfully conflict with in-app
     *    combos — so the scope/conflict machinery doesn't apply.
     *
     * Recording: the macOS WebView can capture the combo as a normal keydown
     * (we `preventDefault`). Global shortcuts always need at least one
     * modifier, so a bare key is rejected. The captured combo is stored in the
     * macOS-symbol form (`'⌃⌥⌘J'`) like the rest of Cmdr; `toAccelerator`
     * translates it for the plugin at the IPC boundary.
     */
    import { onMount } from 'svelte'
    import { commands } from '$lib/ipc/bindings'
    import { getAppLogger } from '$lib/logging/logger'
    import { formatKeyCombo, isModifierKey } from '$lib/shortcuts'
    import { tooltip } from '$lib/tooltip/tooltip'
    import {
        getGlobalGoToLatestBinding,
        setGlobalGoToLatestBinding,
        GLOBAL_GO_TO_LATEST_BINDING_KEY,
        GLOBAL_GO_TO_LATEST_ENABLED_KEY,
    } from './global-shortcut-setting'
    import { toAccelerator, DEFAULT_GLOBAL_GO_TO_LATEST_BINDING } from './global-shortcut-binding'
    import { getSetting, onSpecificSettingChange } from '$lib/settings'
    import { tString } from '$lib/intl/messages.svelte'
    import Trans from '$lib/intl/Trans.svelte'

    const log = getAppLogger('downloads')

    let binding = $state(getGlobalGoToLatestBinding())
    let editing = $state(false)
    let pendingKey = $state('')
    /** Registration feedback. Empty string hides the indicator. */
    let statusText = $state('')
    /**
     * Whether the current `statusText` is a warning (vs a neutral confirmation),
     * driving the warn styling. A typed flag set alongside the text, NOT a
     * substring match on the (localized) status copy.
     */
    let statusIsWarn = $state(false)

    const isModified = $derived(binding !== DEFAULT_GLOBAL_GO_TO_LATEST_BINDING)

    async function applyBinding(next: string): Promise<void> {
        // Reset-aware write: `setGlobalGoToLatestBinding` also clears `acknowledged`
        // so the new combo gets its own first-trigger warning.
        setGlobalGoToLatestBinding(next)
        binding = next
        // Live-apply: re-register with the backend right away. We pass the
        // current `enabled` so toggling-off elsewhere isn't overridden.
        const enabled = getSetting(GLOBAL_GO_TO_LATEST_ENABLED_KEY)
        const result = await commands.setGlobalGoToLatestShortcut(enabled, next)
        if (result.status === 'ok') {
            statusText = tString(
                result.data.status === 'registered'
                    ? 'downloads.shortcutRow.registered'
                    : 'downloads.shortcutRow.notRegistered',
            )
            statusIsWarn = false
        } else if (result.error.kind === 'invalidBinding') {
            statusText = tString('downloads.shortcutRow.invalidCombo')
            statusIsWarn = true
        } else {
            statusText = tString('downloads.shortcutRow.registerFailed', { reason: result.error.message })
            statusIsWarn = true
            log.warn('setGlobalGoToLatestShortcut failed: {error}', { error: JSON.stringify(result.error) })
        }
    }

    function startEditing(): void {
        editing = true
        pendingKey = ''
        statusText = ''
        statusIsWarn = false
    }

    function cancelEditing(): void {
        editing = false
        pendingKey = ''
    }

    function handleReset(): void {
        cancelEditing()
        void applyBinding(DEFAULT_GLOBAL_GO_TO_LATEST_BINDING)
    }

    function handleKeyCapture(event: KeyboardEvent): void {
        if (!editing) return
        event.preventDefault()
        event.stopPropagation()

        if (event.key === 'Escape') {
            cancelEditing()
            return
        }
        // Ignore pure modifier presses — wait for the full combo.
        if (isModifierKey(event.key)) return

        const combo = formatKeyCombo(event)
        // Global shortcuts always need a modifier; `toAccelerator` returns null
        // for a modifier-less combo. Reject it and keep recording.
        if (toAccelerator(combo) === null) {
            statusText = tString('downloads.shortcutRow.addModifier')
            statusIsWarn = true
            return
        }
        pendingKey = combo
        editing = false
        void applyBinding(combo)
    }

    onMount(() => {
        // Capture-phase listener so we intercept the combo before the settings
        // window's own keydown handlers (mirrors KeyboardShortcutsSection).
        function captureKeyDown(event: KeyboardEvent): void {
            if (!editing) return
            handleKeyCapture(event)
        }
        document.addEventListener('keydown', captureKeyDown, true)

        // Keep in sync if the binding changes elsewhere (e.g. a reset, or the
        // warn-toast path). The acknowledged-reset rule is enforced by the
        // setter, not here.
        const unsub = onSpecificSettingChange(GLOBAL_GO_TO_LATEST_BINDING_KEY, (_id, value) => {
            binding = value
        })

        return () => {
            document.removeEventListener('keydown', captureKeyDown, true)
            unsub()
        }
    })
</script>

{#snippet globalMarker(children: import('svelte').Snippet)}
    <span class="global-marker">{@render children()}</span>
{/snippet}

<div class="command-row">
    <div class="command-info">
        {#if isModified}
            <span class="modified-dot" use:tooltip={tString('downloads.shortcutRow.modifiedTooltip')}></span>
        {/if}
        <span class="command-name"
            ><Trans key="downloads.shortcutRow.commandName" snippets={{ marker: globalMarker }} /></span
        >
    </div>
    <div class="command-shortcuts">
        <button
            class="shortcut-pill"
            class:editing
            data-test="global-go-to-latest-binding"
            onclick={() => {
                if (editing) cancelEditing()
                else startEditing()
            }}
        >
            {#if editing}
                {pendingKey || tString('downloads.shortcutRow.pressKeys')}
            {:else}
                {binding}
            {/if}
        </button>
        {#if isModified}
            <button
                class="reset-shortcut"
                aria-label={tString('downloads.shortcutRow.resetTooltip')}
                use:tooltip={tString('downloads.shortcutRow.resetTooltip')}
                onclick={handleReset}
            >
                ↩
            </button>
        {/if}
        {#if statusText}
            <span class="shortcut-status" class:warn={statusIsWarn}>{statusText}</span>
        {/if}
    </div>
</div>

<style>
    /* The single row of the "Global" card. The card heading is the wrapping
       `SectionCard`'s `<h3>`; this component renders no heading of its own. No
       bottom border: it's the only row in its card. */
    .command-row {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: var(--spacing-xs) 0;
    }

    .command-info {
        flex: 1;
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
    }

    .modified-dot {
        width: 6px;
        height: 6px;
        border-radius: var(--radius-full);
        background: var(--color-accent);
    }

    .command-name {
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
    }

    .global-marker {
        color: var(--color-text-tertiary);
    }

    .command-shortcuts {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
    }

    .shortcut-pill {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
        padding: var(--spacing-xxs) var(--spacing-sm);
        background: var(--color-bg-tertiary);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        font-size: var(--font-size-xs);
        font-family: var(--font-system) sans-serif;
        color: var(--color-text-primary);
        cursor: default;
        min-width: 40px;
        text-align: center;
    }

    .shortcut-pill.editing {
        background: var(--color-accent);
        color: var(--color-accent-fg);
        border-color: var(--color-accent);
    }

    .shortcut-pill.editing:hover {
        background: var(--color-accent-hover);
        border-color: var(--color-accent-hover);
    }

    .reset-shortcut {
        width: 20px;
        height: 20px;
        padding: 0;
        border: 1px dashed var(--color-border);
        border-radius: var(--radius-sm);
        background: transparent;
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
        cursor: default;
        display: flex;
        align-items: center;
        justify-content: center;
    }

    .reset-shortcut:hover {
        color: var(--color-accent-text);
        border-color: var(--color-accent);
    }

    .shortcut-status {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-xs);
    }

    .shortcut-status.warn {
        color: var(--color-warning-text);
    }
</style>
