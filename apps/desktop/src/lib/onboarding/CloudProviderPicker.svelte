<script lang="ts">
    import { tick } from 'svelte'
    import { cloudProviderPresets } from '$lib/settings'

    /**
     * Scrollable provider picker for the onboarding wizard's step 2.
     *
     * Renders all 15 cloud-provider presets as a single `<ul role="listbox">`. The
     * active option carries `tabindex=0` (roving), the rest `tabindex=-1`. Keyboard:
     *
     * - ArrowDown / ArrowUp / Home / End: move within the list.
     * - Type-to-jump: typing a prefix selects the first matching option name. The
     *   prefix buffer resets after 700 ms of inactivity. Per onboarding-revamp-plan
     *   § M3 step 1: the file-explorer's `type-to-jump-state.svelte.ts` factory is
     *   pane-coupled (cursor / snapshot deps), so an inline matcher is the right
     *   tradeoff here. 15 names, single comparator, no factory needed.
     *
     * The wizard owns the panel-level focus trap; we just expose this listbox as one
     * focusable on the Tab cycle.
     */

    interface Props {
        value: string
        onChange: (providerId: string) => void
    }

    const { value, onChange }: Props = $props()

    const TYPE_TO_JUMP_RESET_MS = 700

    let listEl: HTMLUListElement | undefined = $state()
    let typeBuffer = $state('')
    let typeBufferTimer: ReturnType<typeof setTimeout> | null = null

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
        // Focus the new option after the parent's reactive update lands.
        await tick()
        const opt = listEl?.querySelectorAll<HTMLLIElement>('li[role="option"]')[index]
        opt?.focus()
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
    aria-label="Cloud AI providers"
    tabindex="-1"
    onkeydown={handleKeydown}
>
    {#each cloudProviderPresets as preset, i (preset.id)}
        <li
            class="provider-option"
            class:active={preset.id === value}
            role="option"
            aria-selected={preset.id === value}
            tabindex={preset.id === value ? 0 : -1}
            data-provider-id={preset.id}
            onclick={() => {
                handleClick(preset.id)
            }}
            onfocus={() => {
                if (preset.id !== value) onChange(preset.id)
            }}
            data-index={i}
        >
            <span class="provider-name">{preset.name}</span>
        </li>
    {/each}
</ul>

<style>
    .provider-list {
        list-style: none;
        margin: 0;
        padding: var(--spacing-xs);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-md);
        background: var(--color-bg-primary);
        overflow-y: auto;
        height: 100%;
        min-height: 0;
    }

    .provider-list:focus-within {
        border-color: var(--color-accent);
        box-shadow: var(--shadow-focus);
    }

    .provider-option {
        padding: var(--spacing-sm) var(--spacing-md);
        border-radius: var(--radius-sm);
        color: var(--color-text-primary);
        font-size: var(--font-size-md);
        line-height: 1.3;
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

    .provider-option:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: -2px;
    }

    .provider-name {
        display: block;
    }
</style>
