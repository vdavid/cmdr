<script lang="ts">
    import type { Snippet } from 'svelte'
    import type { HTMLInputAttributes } from 'svelte/elements'
    import type { TextFieldRadius, TextFieldVariant, TextInputType } from './text-field-types'
    import Icon from './Icon.svelte'
    import type { IconName } from './icons/icon-map'

    /**
     * The house single-line text field. Every raw `<input type="text|password|…">`
     * in the app renders through this, so the border, radius, padding, caret,
     * selection, and focus ring are defined in exactly one place
     * (`app.css` § "Text fields").
     *
     * Presentational only: no validation, debouncing, or submit logic. The caller
     * keeps its own state and behavior, and reads the element back through
     * `bind:inputElement` when it needs imperative focus / select.
     */
    interface Props extends Omit<HTMLInputAttributes, 'type' | 'size' | 'value' | 'class' | 'style'> {
        /** Current text. Bindable; also accepted one-way alongside `oninput` for a controlled field. */
        value?: string
        type?: TextInputType
        radius?: TextFieldRadius
        variant?: TextFieldVariant
        /** Error state: red border and ring, plus `aria-invalid` on the control. */
        invalid?: boolean
        /** Caution state: amber border and ring. `invalid` wins when both are set. */
        warning?: boolean
        /** Render the value in the monospace face (paths, keys). The placeholder stays system-face. */
        mono?: boolean
        /** Decorative glyph before the text (the magnifier on search fields). */
        leadingIcon?: IconName
        /** Controls after the text: a clear button, a reveal toggle, a unit label. */
        trailing?: Snippet
        /** Accessible name when no visible `<label for>` points at `id`. */
        ariaLabel?: string
        /** One-off layout sizing on the frame (width, flex). Never token-worthy styling. */
        containerStyle?: string
        /** The underlying element, for imperative `focus()` / `select()`. */
        inputElement?: HTMLInputElement
    }

    /* eslint-disable prefer-const -- $bindable() requires `let` destructuring */
    let {
        value = $bindable(''),
        type = 'text',
        radius = 'lg',
        variant = 'default',
        invalid = false,
        warning = false,
        mono = false,
        leadingIcon,
        trailing,
        ariaLabel,
        containerStyle,
        inputElement = $bindable(),
        disabled = false,
        readonly: isReadonly = false,
        oninput,
        ...rest
    }: Props = $props()
    /* eslint-enable prefer-const */

    /**
     * The value is one-way bound plus an input handler, not `bind:value`: Svelte
     * forbids `bind:value` next to a dynamic `type`, and `type` genuinely varies at
     * runtime (the password field flips `password` ↔ `text` as it gains focus, and
     * a ladder of static-`type` branches would remount the input and drop that
     * focus). Semantics stay identical to `bind:value`: the field follows whatever
     * the caller renders, and `bind:value` keeps working for callers that want it.
     */
    function handleInput(event: Event & { currentTarget: EventTarget & HTMLInputElement }) {
        value = event.currentTarget.value
        oninput?.(event)
    }
</script>

<div
    class="text-field"
    class:text-field-radius-sm={radius === 'sm'}
    class:text-field-radius-md={radius === 'md'}
    class:text-field-radius-lg={radius === 'lg'}
    class:text-field-radius-full={radius === 'full'}
    class:text-field-chromeless={variant === 'chromeless'}
    class:text-field-warning={warning && !invalid}
    class:text-field-invalid={invalid}
    class:text-field-readonly={isReadonly}
    class:text-field-disabled={disabled}
    style={containerStyle}
>
    {#if leadingIcon}
        <span class="text-field-affix"><Icon name={leadingIcon} size={16} aria-hidden="true" /></span>
    {/if}
    <input
        bind:this={inputElement}
        {type}
        {value}
        class="text-field-control"
        class:text-field-control-mono={mono}
        {disabled}
        readonly={isReadonly}
        aria-label={ariaLabel}
        aria-invalid={invalid ? 'true' : undefined}
        oninput={handleInput}
        {...rest}
    />
    {#if trailing}
        <span class="text-field-affix">{@render trailing()}</span>
    {/if}
</div>
