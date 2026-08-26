<script lang="ts">
    import { getSetting, setSetting, onSpecificSettingChange, type SettingId, type SettingsValues } from '$lib/settings'
    import { tooltip } from '$lib/tooltip/tooltip'
    import Icon from '$lib/ui/Icon.svelte'
    import TextInput from '$lib/ui/TextInput.svelte'
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

    // Subscribe to setting changes (for external resets): only in uncontrolled mode
    onMount(() => {
        if (onchange) return
        return onSpecificSettingChange(id, (newValue) => {
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

<TextInput
    type={inputType}
    value={displayValue}
    oninput={handleInput}
    onfocus={() => (focused = true)}
    onblur={() => (focused = false)}
    {placeholder}
    {disabled}
    ariaLabel={ariaLabel}
    autocomplete="off"
    spellcheck={false}
    containerStyle="min-width: 180px"
>
    {#snippet trailing()}
        <button
            class="reveal-toggle"
            type="button"
            onclick={toggleReveal}
            {disabled}
            aria-label={toggleTooltip}
            use:tooltip={toggleTooltip}
        >
            <Icon name={revealed ? 'eye-off' : 'eye'} size={14} aria-hidden="true" />
        </button>
    {/snippet}
</TextInput>

<style>
    /* Compact icon button sized to the field's text line, so the trailing control
       doesn't stretch the field taller than every other one. */
    .reveal-toggle {
        display: flex;
        align-items: center;
        justify-content: center;
        width: 20px;
        height: 20px;
        border: none;
        border-radius: var(--radius-sm);
        background: transparent;
        color: var(--color-text-tertiary);
    }

    .reveal-toggle:hover:not(:disabled) {
        color: var(--color-text-primary);
        background: var(--color-bg-tertiary);
    }

    .reveal-toggle:disabled {
        opacity: 0.5;
        cursor: not-allowed;
    }

    .reveal-toggle:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 1px;
    }
</style>
