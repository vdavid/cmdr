<script lang="ts">
    import { onMount, tick } from 'svelte'
    import { getExtension } from '$lib/utils/filename-validation'

    interface Props {
        /** Current filename value (two-way bindable via onInput) */
        value: string
        /** Validation severity for border coloring */
        severity: 'ok' | 'error' | 'warning'
        /** Whether the shake animation is active */
        shaking: boolean
        /** Accessible label */
        ariaLabel: string
        /** Whether there's a validation error (for aria-invalid) */
        ariaInvalid?: boolean
        /** Validation message announced to screen readers via aria-live region */
        validationMessage?: string
        /** Increment to re-focus the input and restore selection (after dialog closes) */
        focusTrigger?: number
        /** Called when the value changes */
        onInput: (value: string) => void
        /** Called when Enter is pressed */
        onSubmit: () => void
        /** Called when Escape is pressed or focus leaves */
        onCancel: () => void
        /** Called when shake animation ends */
        onShakeEnd: () => void
    }

    const {
        value,
        severity,
        shaking,
        ariaLabel,
        ariaInvalid = false,
        validationMessage = '',
        focusTrigger = 0,
        onInput,
        onSubmit,
        onCancel,
        onShakeEnd,
    }: Props = $props()

    let inputElement: HTMLInputElement | undefined = $state()

    /** Focuses the input and selects the filename excluding the extension. */
    function focusAndSelect() {
        if (!inputElement) return
        inputElement.focus()
        const ext = getExtension(value)
        const nameWithoutExt = ext ? value.slice(0, -ext.length) : value
        inputElement.setSelectionRange(0, nameWithoutExt.length)
    }

    function getSeverityColor(): string {
        if (severity === 'error') return 'var(--color-error)'
        if (severity === 'warning') return 'var(--color-warning)'
        return 'var(--color-allow)'
    }

    /** Inline style: border color + box-shadow (avoids stylelint custom property restrictions) */
    const inputStyle = $derived.by(() => {
        const color = getSeverityColor()
        return `border-color: ${color}; box-shadow: 0 0 6px 1px ${color};`
    })

    function handleKeyDown(e: KeyboardEvent) {
        if (e.key === 'Enter') {
            e.preventDefault()
            e.stopPropagation()
            onSubmit()
            return
        }
        if (e.key === 'Escape') {
            e.preventDefault()
            e.stopPropagation()
            onCancel()
            return
        }
        if (e.key === 'Tab') {
            e.preventDefault()
            e.stopPropagation()
            onCancel()
            return
        }
        // Stop propagation of all keys to prevent app shortcuts
        // except standard text editing keys (Cmd+C/A/Z/X/V handled natively by the input)
        e.stopPropagation()
    }

    function handleInput(e: Event) {
        const target = e.target as HTMLInputElement
        onInput(target.value)
    }

    function handleBlur() {
        onCancel()
    }

    function handleAnimationEnd(e: AnimationEvent) {
        if (e.animationName === 'rename-shake') {
            onShakeEnd()
        }
    }

    onMount(async () => {
        await tick()
        focusAndSelect()
    })

    // Re-focus when focusTrigger increments (after a dialog closes and returns to editing)
    $effect(() => {
        void focusTrigger
        if (focusTrigger > 0) {
            void tick().then(() => {
                focusAndSelect()
            })
        }
    })
</script>

<input
    bind:this={inputElement}
    type="text"
    class="rename-input"
    class:shaking
    {value}
    aria-label={ariaLabel}
    aria-invalid={ariaInvalid}
    aria-describedby="rename-validation-live"
    style={inputStyle}
    oninput={handleInput}
    onkeydown={handleKeyDown}
    onblur={handleBlur}
    onanimationend={handleAnimationEnd}
/>
<span id="rename-validation-live" class="sr-only" aria-live="assertive">{validationMessage}</span>

<style>
    .sr-only {
        position: absolute;
        width: 1px;
        height: 1px;
        padding: 0;
        margin: -1px;
        overflow: hidden;
        clip-path: inset(50%);
        white-space: nowrap;
        border: 0;
    }

    .rename-input {
        width: 100%;
        height: 100%;
        padding: 0 var(--spacing-xxs);
        font: inherit;
        font-size: inherit;
        line-height: inherit;
        color: var(--color-text-primary);
        background: var(--color-bg-primary);
        border: 2px solid;
        border-radius: var(--radius-sm);
        outline: none;
        box-sizing: border-box;
        user-select: text;
        -webkit-user-select: text;
        cursor: text;
        animation: rename-glow 300ms ease-out;
    }

    .rename-input.shaking {
        animation: rename-shake 300ms ease-in-out;
    }

    /* Glow animation uses only transform (box-shadow set via inline style) */
    @keyframes rename-glow {
        0% {
            transform: scale(1.02);
        }

        100% {
            transform: scale(1);
        }
    }

    @keyframes rename-shake {
        0%,
        100% {
            transform: translateX(0);
        }

        20% {
            transform: translateX(-4px);
        }

        40% {
            transform: translateX(4px);
        }

        60% {
            transform: translateX(-3px);
        }

        80% {
            transform: translateX(3px);
        }
    }
</style>
