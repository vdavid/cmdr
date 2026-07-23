<script lang="ts">
    import { Switch } from '@ark-ui/svelte/switch'
    import type { Snippet } from 'svelte'

    /**
     * The house switch: a thin, presentational wrapper over Ark UI's `Switch` so the macOS-y
     * track-and-thumb look lives in one place. `SettingSwitch` wraps this and adds the settings
     * registry wiring; feature code binds it directly.
     *
     * Bind the state: `<Switch bind:checked={value} />`. Pass `children` to render an inline label
     * to the right of the track; omit it for a bare track (rows that own their label).
     *
     * Switch vs `Checkbox`: a switch reads as "this is on/off right now", a checkbox as "this
     * option is selected". Both are fine in a form; pick by which sentence the control tells.
     */
    interface Props {
        checked?: boolean
        disabled?: boolean
        id?: string
        /** Accessible name when there's no visible `children` label. */
        ariaLabel?: string
        onCheckedChange?: (checked: boolean) => void
        children?: Snippet
    }

    /* eslint-disable prefer-const -- $bindable() requires `let` destructuring */
    let { checked = $bindable(false), disabled = false, id, ariaLabel, onCheckedChange, children }: Props = $props()
    /* eslint-enable prefer-const */
</script>

<Switch.Root
    class="switch-root"
    {checked}
    onCheckedChange={(details) => {
        checked = details.checked
        onCheckedChange?.(checked)
    }}
    {disabled}
    {id}
    aria-label={ariaLabel}
>
    <Switch.Control class="switch-control">
        <Switch.Thumb class="switch-thumb" />
    </Switch.Control>
    {#if children}
        <Switch.Label class="switch-label">{@render children()}</Switch.Label>
    {/if}
    <Switch.HiddenInput />
</Switch.Root>

<style>
    :global(.switch-root) {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-sm);
        cursor: default;
    }

    :global(.switch-control) {
        display: inline-flex;
        align-items: center;
        flex-shrink: 0;
        width: 36px;
        height: 20px;
        background: var(--color-bg-tertiary);
        border-radius: var(--radius-full);
        padding: var(--spacing-xxs);
        cursor: default;
        transition: background-color var(--transition-base);
    }

    :global(.switch-control[data-state='checked']) {
        background: var(--color-accent);
    }

    :global(.switch-control[data-disabled]) {
        cursor: not-allowed;
        opacity: 0.5;
    }

    :global(.switch-thumb) {
        width: 16px;
        height: 16px;
        /* Literal white, not a token: the thumb stays white in both themes so it
           reads against the accent track. */
        background: white;
        border-radius: var(--radius-full);
        transition: transform var(--transition-base);
        box-shadow: var(--shadow-sm);
    }

    :global(.switch-control[data-state='checked'] .switch-thumb) {
        transform: translateX(16px);
    }

    :global(.switch-control[data-state='checked']:hover) {
        background: var(--color-accent-hover);
    }

    :global(.switch-label) {
        font-size: var(--font-size-md);
        cursor: default;
    }

    /* Ark UI uses data-focus attribute when the hidden input is focused */
    :global(.switch-control[data-focus]) {
        outline: 2px solid var(--color-accent);
        outline-offset: 2px;
        box-shadow: var(--shadow-focus);
    }
</style>
