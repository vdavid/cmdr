<script lang="ts">
    import { Checkbox } from '@ark-ui/svelte/checkbox'
    import type { Snippet } from 'svelte'
    import Icon from '$lib/ui/Icon.svelte'

    /**
     * The house checkbox. A thin, presentational wrapper over Ark UI's `Checkbox` so the macOS-y
     * look lives in one place. Unlike a raw `<input type="checkbox">` it does NOT gray out when the
     * window loses focus, and it themes through the design tokens.
     *
     * Bind the state: `<Checkbox bind:checked={value} />`. Pass `children` to render an inline label
     * to the right of the box; omit it for a bare box (list rows, dense grids that own their label).
     */
    interface Props {
        checked?: boolean
        disabled?: boolean
        /** Renders the mixed (dash) state; overrides `checked` visually while set. */
        indeterminate?: boolean
        id?: string
        /** Accessible name when there's no visible `children` label. */
        ariaLabel?: string
        onCheckedChange?: (checked: boolean) => void
        children?: Snippet
    }

    /* eslint-disable prefer-const -- $bindable() requires `let` destructuring */
    let {
        checked = $bindable(false),
        disabled = false,
        indeterminate = false,
        id,
        ariaLabel,
        onCheckedChange,
        children,
    }: Props = $props()
    /* eslint-enable prefer-const */

    // Ark's `checked` accepts `boolean | 'indeterminate'`.
    const checkedState = $derived(indeterminate ? ('indeterminate' as const) : checked)
</script>

<Checkbox.Root
    class="checkbox-root"
    checked={checkedState}
    onCheckedChange={(details) => {
        checked = details.checked === true
        onCheckedChange?.(checked)
    }}
    {disabled}
    {id}
>
    <Checkbox.Control class="checkbox-control">
        <Checkbox.Indicator class="checkbox-indicator">
            <span class="checkbox-check"><Icon name="check" size={12} aria-hidden="true" /></span>
            <span class="checkbox-dash" aria-hidden="true"></span>
        </Checkbox.Indicator>
    </Checkbox.Control>
    {#if children}
        <Checkbox.Label class="checkbox-label">{@render children()}</Checkbox.Label>
    {/if}
    <!-- `aria-label` belongs on the INPUT: Ark points its `aria-labelledby` at
         `Checkbox.Label`, which doesn't exist when the caller passes no `children`, and
         a dangling reference leaves the control with NO accessible name. `aria-label`
         on the wrapping `<label>` names the label, not the control. `aria-labelledby`
         still wins when a visible label IS rendered, so passing both is safe. -->
    <Checkbox.HiddenInput aria-label={ariaLabel} />
</Checkbox.Root>

<style>
    /* Ark's `Root` is a `<label>`, `display: inline` by default, which baseline-aligns
       the box against the label text and leaves the box sitting low. Flex + `center`
       puts the box on the label's optical middle; the gap lives here too, since the
       label span is the box's SIBLING, not its container. */
    :global(.checkbox-root) {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-sm);
        cursor: default;
        /* Contain Ark's `HiddenInput` so it resolves against this control, not
           the window shell. See `Switch.svelte`'s `.switch-root` for the full
           why (the shell-scroll-under-traffic-lights bug). */
        position: relative;
    }

    :global(.checkbox-control) {
        flex-shrink: 0;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 16px;
        height: 16px;
        background: var(--color-bg-tertiary);
        border: 1px solid var(--color-control-border);
        border-radius: var(--radius-xs);
        cursor: default;
        transition:
            background-color var(--transition-base),
            border-color var(--transition-base);
    }

    :global(.checkbox-control[data-state='checked']),
    :global(.checkbox-control[data-state='indeterminate']) {
        background: var(--color-accent);
        border-color: var(--color-accent);
    }

    :global(.checkbox-control[data-state='checked']:hover),
    :global(.checkbox-control[data-state='indeterminate']:hover) {
        background: var(--color-accent-hover);
        border-color: var(--color-accent-hover);
    }

    /* Hover must push the outline FURTHER from the surface, so it keys off a text
       token: `--color-border-strong` is fainter than `--color-control-border` and
       would make hovering look like a disabled state. */
    :global(.checkbox-control:hover) {
        border-color: var(--color-text-secondary);
    }

    :global(.checkbox-control[data-disabled]) {
        cursor: not-allowed;
        opacity: 0.5;
    }

    :global(.checkbox-indicator) {
        display: flex;
        align-items: center;
        justify-content: center;
        color: var(--color-accent-fg);
    }

    .checkbox-check {
        display: flex;
        align-items: center;
        justify-content: center;
    }

    /* Unchecked reads as an empty square. Ark keeps the indicator mounted in every
       state, so hide the glyph ourselves instead of relying on it to unmount. */
    :global(.checkbox-control[data-state='unchecked']) .checkbox-check {
        display: none;
    }

    /* Indeterminate: hide the check, show a horizontal bar. */
    .checkbox-dash {
        display: none;
        width: 8px;
        height: 2px;
        border-radius: var(--radius-xs);
        background: var(--color-accent-fg);
    }

    :global(.checkbox-control[data-state='indeterminate']) .checkbox-check {
        display: none;
    }

    :global(.checkbox-control[data-state='indeterminate']) .checkbox-dash {
        display: block;
    }

    :global(.checkbox-label) {
        font-size: var(--font-size-md);
        cursor: default;
    }

    /* Ark UI uses data-focus attribute when the hidden input is focused */
    :global(.checkbox-control[data-focus]) {
        outline: 2px solid var(--color-accent);
        outline-offset: 2px;
        box-shadow: var(--shadow-focus);
    }
</style>
