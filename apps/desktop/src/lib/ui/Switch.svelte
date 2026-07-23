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
     *
     * Any extra `data-*` attribute passes straight through to the hidden `<input>`
     * (`<Switch data-test="media-net-optin" data-volume-id={id} />`). The input is the element
     * tests query and click: Ark marks the styled track `aria-hidden`, so a hook on the track
     * would sit on decoration. Nothing else spreads through, so the API stays a fixed shape.
     */
    interface Props {
        checked?: boolean
        disabled?: boolean
        id?: string
        /** Accessible name when there's no visible `children` label. */
        ariaLabel?: string
        /**
         * Annotate the parameter at inline call sites (`(next: boolean) => …`): svelte2tsx
         * doesn't contextually type an arrow passed to a component prop, so an unannotated
         * one is `any` and trips `@typescript-eslint/no-unsafe-argument`.
         */
        onCheckedChange?: (checked: boolean) => void
        children?: Snippet
        /** Test/automation hooks; they land on the hidden `<input>`, not the track. */
        [dataAttribute: `data-${string}`]: string | undefined
    }

    /* eslint-disable prefer-const -- $bindable() requires `let` destructuring */
    let {
        checked = $bindable(false),
        disabled = false,
        id,
        ariaLabel,
        onCheckedChange,
        children,
        ...dataAttributes
    }: Props = $props()
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
>
    <Switch.Control class="switch-control">
        <Switch.Thumb class="switch-thumb" />
    </Switch.Control>
    {#if children}
        <Switch.Label class="switch-label">{@render children()}</Switch.Label>
    {/if}
    <!-- Both attributes belong on the INPUT, which is the thing assistive tech sees.
         `role="switch"`: Ark ships a bare `input[type=checkbox]`, so without it a
         screen reader says "checkbox", not "switch, on".
         `aria-label`: Ark points the input's `aria-labelledby` at `Switch.Label`, which
         doesn't exist when the caller passes no `children` — a dangling reference
         leaves the control with NO accessible name. `aria-labelledby` still wins when
         a visible label IS rendered, so passing both is safe. -->
    <Switch.HiddenInput role="switch" aria-label={ariaLabel} {...dataAttributes} />
</Switch.Root>

<style>
    :global(.switch-root) {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-sm);
        cursor: default;
        /* Contain Ark's `HiddenInput`. Ark hides it sr-only style (`position:
           absolute` with `clip`, no `top`/`left`), so it resolves against the
           nearest positioned ancestor. Without this, that's the window shell,
           and the input's flow position (which can sit below the fold) makes
           the shell secretly scrollable; focusing it on click scrolls the whole
           window under the traffic lights. Keeping it local lets the real
           scroller clip it. */
        position: relative;
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
