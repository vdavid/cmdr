<script lang="ts">
    /**
     * The global go-to-latest-download hotkey, shown as a shortcut row inside
     * the Keyboard shortcuts section. This is where users edit the combo —
     * it's a system-wide hotkey, so it lives alongside the other shortcuts but
     * is marked `(global)` to set expectations (it fires from any app, and its
     * on/off switch lives under Behavior → File system watching).
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

    const log = getAppLogger('downloads')

    let binding = $state(getGlobalGoToLatestBinding())
    let editing = $state(false)
    let pendingKey = $state('')
    /** Registration feedback. Empty string hides the indicator. */
    let statusText = $state('')

    const isModified = $derived(binding !== DEFAULT_GLOBAL_GO_TO_LATEST_BINDING)

    async function applyBinding(next: string): Promise<void> {
        // Reset-aware write: `setGlobalGoToLatestBinding` also clears `acknowledged`
        // so the new combo gets its own first-trigger warning.
        setGlobalGoToLatestBinding(next)
        binding = next
        // Live-apply: re-register with the backend right away. We pass the
        // current `enabled` so toggling-off elsewhere isn't overridden.
        // eslint-disable-next-line @typescript-eslint/no-explicit-any -- registry key
        const enabled = getSetting(GLOBAL_GO_TO_LATEST_ENABLED_KEY as any) as boolean
        const result = await commands.setGlobalGoToLatestShortcut(enabled, next)
        if (result.status === 'ok') {
            statusText = result.data.status === 'registered' ? 'Registered' : 'Not registered'
        } else if (result.error.kind === 'invalidBinding') {
            statusText = `Couldn't register: invalid combo`
        } else {
            statusText = `Couldn't register: ${result.error.message}`
            log.warn('setGlobalGoToLatestShortcut failed: {error}', { error: JSON.stringify(result.error) })
        }
    }

    function startEditing(): void {
        editing = true
        pendingKey = ''
        statusText = ''
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
            statusText = 'Add a modifier (⌘, ⌃, ⌥, or ⇧)'
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

<div class="scope-group">
    <h3 class="scope-title">Global</h3>
    <div class="command-row">
        <div class="command-info">
            {#if isModified}
                <span class="modified-dot" use:tooltip={'Modified from default'}></span>
            {/if}
            <span class="command-name">Go to latest download <span class="global-marker">(global)</span></span>
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
                    {pendingKey || 'Press keys...'}
                {:else}
                    {binding}
                {/if}
            </button>
            {#if isModified}
                <button
                    class="reset-shortcut"
                    aria-label="Reset to default"
                    use:tooltip={'Reset to default'}
                    onclick={handleReset}
                >
                    ↩
                </button>
            {/if}
            {#if statusText}
                <span class="shortcut-status" class:warn={statusText.includes("Couldn't") || statusText.includes('Add')}
                    >{statusText}</span
                >
            {/if}
        </div>
    </div>
</div>

<style>
    .scope-group {
        margin-bottom: var(--spacing-lg);
    }

    .scope-title {
        font-size: var(--font-size-sm);
        font-weight: 600;
        color: var(--color-text-tertiary);
        margin: 0 0 var(--spacing-xs);
        text-transform: uppercase;
        letter-spacing: 0.5px;
    }

    .command-row {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: var(--spacing-xs) 0;
        border-bottom: 1px solid var(--color-border-subtle);
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
