<script lang="ts">
    import { getSetting, setSetting, onSpecificSettingChange, type SettingId, type SettingsValues } from '$lib/settings'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { onMount } from 'svelte'

    interface Props {
        id: SettingId
        placeholder?: string
        ariaLabel?: string
        disabled?: boolean
        /** External value (bypasses settings store when provided alongside `onchange`). */
        value?: string
        /** Called when the value changes. When provided, the component uses this instead of the settings store. */
        onchange?: (value: string) => void
    }

    const { id, placeholder = '', ariaLabel, disabled = false, value: externalValue, onchange }: Props = $props()

    let internalValue = $state(onchange ? (externalValue ?? '') : (getSetting(id) as string))
    let revealed = $state(false)
    let focused = $state(false)

    // Keep internal value in sync with external value when controlled
    $effect(() => {
        if (onchange && externalValue !== undefined) {
            internalValue = externalValue
        }
    })

    // Subscribe to setting changes (for external resets) — only in uncontrolled mode
    onMount(() => {
        if (onchange) return
        return onSpecificSettingChange(id, (_id, newValue) => {
            internalValue = newValue as string
        })
    })

    function handleInput(event: Event) {
        const input = event.target as HTMLInputElement
        internalValue = input.value
        if (onchange) {
            onchange(input.value)
        } else {
            setSetting(id, input.value as SettingsValues[typeof id])
        }
    }

    function toggleReveal() {
        revealed = !revealed
    }

    /** Masks all but the last 4 characters (like "••••••••sk-1234"). */
    function maskValue(val: string): string {
        const revealChars = 4
        if (val.length <= revealChars) return '\u2022'.repeat(val.length)
        return '\u2022'.repeat(val.length - revealChars) + val.slice(-revealChars)
    }

    // When not focused and not revealed, show a masked preview with last 4 chars visible.
    // When focused, use native password masking for secure input.
    // When revealed, show the full value as plain text.
    const inputType = $derived(focused && !revealed ? 'password' : 'text')
    const displayValue = $derived(revealed || focused ? internalValue : maskValue(internalValue))

    const toggleTooltip = $derived(revealed ? 'Hide value' : 'Show value')
</script>

<div class="password-input-wrapper">
    <input
        class="password-input"
        type={inputType}
        value={displayValue}
        oninput={handleInput}
        onfocus={() => (focused = true)}
        onblur={() => (focused = false)}
        {placeholder}
        {disabled}
        aria-label={ariaLabel}
        autocomplete="off"
        spellcheck="false"
    />
    <button
        class="toggle-button"
        type="button"
        onclick={toggleReveal}
        {disabled}
        aria-label={toggleTooltip}
        use:tooltip={toggleTooltip}
    >
        {#if revealed}
            <!-- Eye-off icon (hide) -->
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" aria-hidden="true">
                <path
                    d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19m-6.72-1.07a3 3 0 1 1-4.24-4.24"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                />
                <line
                    x1="1"
                    y1="1"
                    x2="23"
                    y2="23"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                />
            </svg>
        {:else}
            <!-- Eye icon (show) -->
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" aria-hidden="true">
                <path
                    d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                />
                <circle
                    cx="12"
                    cy="12"
                    r="3"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                />
            </svg>
        {/if}
    </button>
</div>

<style>
    .password-input-wrapper {
        display: flex;
        align-items: center;
        min-width: 180px;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        background: var(--color-bg-primary);
        transition: border-color var(--transition-base);
    }

    .password-input-wrapper:focus-within {
        border-color: var(--color-accent);
        box-shadow: var(--shadow-focus);
    }

    .password-input {
        flex: 1;
        padding: var(--spacing-sm) var(--spacing-md);
        border: none;
        border-radius: var(--radius-sm) 0 0 var(--radius-sm);
        background: transparent;
        color: var(--color-text-primary);
        font-size: var(--font-size-md);
        line-height: 1.4;
        outline: none;
    }

    .password-input::placeholder {
        color: var(--color-text-tertiary);
    }

    .password-input:disabled {
        opacity: 0.5;
        cursor: not-allowed;
    }

    .toggle-button {
        display: flex;
        align-items: center;
        justify-content: center;
        flex-shrink: 0;
        width: 32px;
        height: 32px;
        border: none;
        border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
        background: transparent;
        color: var(--color-text-tertiary);
        cursor: default;
        transition: color var(--transition-base);
    }

    .toggle-button:hover:not(:disabled) {
        color: var(--color-text-primary);
    }

    .toggle-button:disabled {
        opacity: 0.5;
        cursor: not-allowed;
    }

    .toggle-button:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: -2px;
        box-shadow: var(--shadow-focus);
    }
</style>
