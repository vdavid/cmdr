<script lang="ts">
    import type { Snippet } from 'svelte'
    import type { HTMLTextareaAttributes } from 'svelte/elements'
    import type { TextFieldRadius, TextFieldVariant } from './text-field-types'

    /**
     * The house multi-line text field: `TextInput`'s sibling, sharing its exact
     * chrome through the same `.text-field*` classes in `app.css` § "Text fields".
     *
     * A sibling rather than a `multiline` prop on `TextInput` because the two have
     * genuinely different contracts: a textarea has `rows` and `resize` and no
     * `type` / `leadingIcon`, and `bind:this` must resolve to `HTMLTextAreaElement`
     * for the call sites that focus and select imperatively. One shared stylesheet
     * keeps them visually identical anyway, which is the point of the primitive.
     */
    interface Props extends Omit<HTMLTextareaAttributes, 'value' | 'class' | 'style'> {
        /** Current text. Bindable; also accepted one-way alongside `oninput` for a controlled field. */
        value?: string
        radius?: TextFieldRadius
        variant?: TextFieldVariant
        /** Error state: red border and ring, plus `aria-invalid` on the control. */
        invalid?: boolean
        /** Caution state: amber border and ring. `invalid` wins when both are set. */
        warning?: boolean
        /** Render the value in the monospace face (log output, paths). */
        mono?: boolean
        /** Controls rendered after the text (a counter, an action). */
        trailing?: Snippet
        /** Accessible name when no visible `<label for>` points at `id`. */
        ariaLabel?: string
        /** Vertical resize grip. Off for fields whose host sizes them (auto-growing composers). */
        resizable?: boolean
        /** One-off layout sizing on the frame (width, height, flex). Never token-worthy styling. */
        containerStyle?: string
        /** The underlying element, for imperative `focus()` / `select()` / height measurement. */
        textareaElement?: HTMLTextAreaElement
    }

    /* eslint-disable prefer-const -- $bindable() requires `let` destructuring */
    let {
        value = $bindable(''),
        radius = 'lg',
        variant = 'default',
        invalid = false,
        warning = false,
        mono = false,
        trailing,
        ariaLabel,
        resizable = true,
        containerStyle,
        textareaElement = $bindable(),
        disabled = false,
        readonly: isReadonly = false,
        oninput,
        ...rest
    }: Props = $props()
    /* eslint-enable prefer-const */

    /** Same one-way-value-plus-`oninput` shape as `TextInput`; see the comment there. */
    function handleInput(event: Event & { currentTarget: EventTarget & HTMLTextAreaElement }) {
        value = event.currentTarget.value
        oninput?.(event)
    }
</script>

<div
    class="text-field text-field-multiline"
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
    <textarea
        bind:this={textareaElement}
        {value}
        class="text-field-control"
        class:text-field-control-mono={mono}
        class:text-field-control-fixed={!resizable}
        {disabled}
        readonly={isReadonly}
        aria-label={ariaLabel}
        aria-invalid={invalid ? 'true' : undefined}
        oninput={handleInput}
        {...rest}
    ></textarea>
    {#if trailing}
        <span class="text-field-affix">{@render trailing()}</span>
    {/if}
</div>
