<script lang="ts">
    import { tick } from 'svelte'
    import { cloudProviderPresets } from '$lib/settings'
    import { tString } from '$lib/intl/messages.svelte'

    /**
     * Scrollable provider picker for the onboarding wizard's step 2.
     *
     * Renders all 15 cloud-provider presets as a single `<ul role="listbox">`. The list
     * is ONE tab stop (`tabindex=0` on the `<ul>`); the options themselves are never
     * focused. The visually-active option is tracked with `aria-activedescendant`, the
     * standard managed-focus listbox pattern. This keeps Tab clean: Tab moves into the
     * list, Tab again moves straight out to the setup column. Keyboard inside:
     *
     * - ArrowDown / ArrowUp / Home / End: move the active option (and the selection).
     * - Type-to-jump: typing a prefix selects the first matching option name. The
     *   prefix buffer resets after 700 ms of inactivity. The file-explorer's
     *   `type-to-jump-state.svelte.ts` factory is pane-coupled (cursor / snapshot deps),
     *   so an inline matcher is the right tradeoff here: 15 names, single comparator.
     *
     * Earlier this list used a roving `tabindex` and moved real focus onto each option,
     * which made Tab feel like it was "captured" inside the list and coupled selection to
     * focus. The activedescendant pattern fixes both.
     */

    interface Props {
        value: string
        onChange: (providerId: string) => void
    }

    const { value, onChange }: Props = $props()

    const TYPE_TO_JUMP_RESET_MS = 700

    /** Stable per-option DOM id, referenced by `aria-activedescendant`. */
    function optionId(providerId: string): string {
        return `onboarding-provider-${providerId}`
    }

    let listEl: HTMLUListElement | undefined = $state()
    let typeBuffer = $state('')
    let typeBufferTimer: ReturnType<typeof setTimeout> | null = null

    const activeOptionId = $derived(optionId(value))

    function clearTypeBuffer(): void {
        typeBuffer = ''
        if (typeBufferTimer) {
            clearTimeout(typeBufferTimer)
            typeBufferTimer = null
        }
    }

    function bumpTypeBufferTimer(): void {
        if (typeBufferTimer) clearTimeout(typeBufferTimer)
        typeBufferTimer = setTimeout(() => {
            clearTypeBuffer()
        }, TYPE_TO_JUMP_RESET_MS)
    }

    function indexOf(providerId: string): number {
        return cloudProviderPresets.findIndex((p) => p.id === providerId)
    }

    async function selectByIndex(index: number): Promise<void> {
        if (index < 0 || index >= cloudProviderPresets.length) return
        const preset = cloudProviderPresets[index]
        if (preset.id !== value) onChange(preset.id)
        // Keep the (still-focused) list scrolled to the active option. No `.focus()`:
        // focus stays on the `<ul>`; the active option is conveyed via aria-activedescendant.
        await tick()
        const opt = listEl?.querySelector<HTMLLIElement>(`#${CSS.escape(optionId(preset.id))}`)
        opt?.scrollIntoView({ block: 'nearest' })
    }

    function handleKeydown(event: KeyboardEvent): void {
        const current = indexOf(value)
        if (event.key === 'ArrowDown') {
            event.preventDefault()
            event.stopPropagation()
            void selectByIndex(Math.min(current + 1, cloudProviderPresets.length - 1))
            clearTypeBuffer()
            return
        }
        if (event.key === 'ArrowUp') {
            event.preventDefault()
            event.stopPropagation()
            void selectByIndex(Math.max(current - 1, 0))
            clearTypeBuffer()
            return
        }
        if (event.key === 'Home') {
            event.preventDefault()
            event.stopPropagation()
            void selectByIndex(0)
            clearTypeBuffer()
            return
        }
        if (event.key === 'End') {
            event.preventDefault()
            event.stopPropagation()
            void selectByIndex(cloudProviderPresets.length - 1)
            clearTypeBuffer()
            return
        }
        // Type-to-jump: single printable char, no modifiers (Shift is OK so users can
        // type capitals). We append, then find the first prefix match (case-insensitive).
        if (event.key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey) {
            const ch = event.key.toLowerCase()
            // Ignore whitespace (none of the provider names start with one).
            if (ch === ' ' || ch === '\t') return
            typeBuffer += ch
            bumpTypeBufferTimer()
            const hitIndex = cloudProviderPresets.findIndex((p) => p.name.toLowerCase().startsWith(typeBuffer))
            if (hitIndex >= 0) {
                event.preventDefault()
                event.stopPropagation()
                void selectByIndex(hitIndex)
            }
        }
    }

    function handleClick(providerId: string): void {
        if (providerId !== value) onChange(providerId)
    }
</script>

<ul
    bind:this={listEl}
    class="provider-list"
    role="listbox"
    aria-label={tString('onboarding.cloudPicker.listAria')}
    tabindex="0"
    aria-activedescendant={activeOptionId}
    onkeydown={handleKeydown}
>
    {#each cloudProviderPresets as preset (preset.id)}
        <li
            id={optionId(preset.id)}
            class="provider-option"
            class:active={preset.id === value}
            role="option"
            aria-selected={preset.id === value}
            data-provider-id={preset.id}
            onclick={() => {
                handleClick(preset.id)
            }}
        >
            <span class="provider-name">{preset.name}</span>
        </li>
    {/each}
</ul>

<style>
    .provider-list {
        list-style: none;
        margin: 0;
        padding: var(--spacing-xxs);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-md);
        background: var(--color-bg-primary);
        overflow-y: auto;
        /* Fills the remaining height of the picker column (title sits above it). */
        flex: 1;
        min-height: 0;
    }

    .provider-list:focus-visible {
        outline: none;
        border-color: var(--color-accent);
        box-shadow: var(--shadow-focus);
    }

    .provider-option {
        padding: var(--spacing-xxs) var(--spacing-sm);
        border-radius: var(--radius-sm);
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        line-height: 1.5;
        transition: background var(--transition-base), color var(--transition-base);
    }

    .provider-option:hover {
        background: var(--color-bg-tertiary);
    }

    .provider-option.active {
        background: var(--color-accent-subtle);
        color: var(--color-text-primary);
        font-weight: 500;
    }

    /* The list is the focus owner, so the active option shows the focus ring while the
       `<ul>` has focus (mirrors a native listbox's highlighted row). */
    .provider-list:focus-visible .provider-option.active {
        outline: 2px solid var(--color-accent);
        outline-offset: -2px;
    }

    .provider-name {
        display: block;
    }
</style>
