<script lang="ts">
    import { onMount, tick } from 'svelte'
    import {
        getSetting,
        setSetting,
        onSpecificSettingChange,
        VOLUME_TINT_COLORS,
        type SettingId,
        type VolumeTintColor,
    } from '$lib/settings'
    import { nextSwatchIndex } from './swatch-keyboard'

    interface Props {
        id: SettingId
        label: string
    }

    const { id, label }: Props = $props()

    let value = $state(getSetting(id) as VolumeTintColor)
    let open = $state(false)
    let buttonEl: HTMLButtonElement | undefined = $state()
    let popoverEl: HTMLDivElement | undefined = $state()

    onMount(() => {
        return onSpecificSettingChange(id, (_id, newValue) => {
            value = newValue as VolumeTintColor
        })
    })

    function colorLabel(c: VolumeTintColor): string {
        return c === 'none' ? 'No tint' : c.charAt(0).toUpperCase() + c.slice(1)
    }

    function openPopover() {
        open = true
        void tick().then(() => {
            // Focus the currently-selected swatch, or the first one
            const target = popoverEl?.querySelector<HTMLButtonElement>('[data-selected="true"]') ??
                popoverEl?.querySelector<HTMLButtonElement>('button[role="option"]')
            target?.focus()
        })
    }

    function closePopover(returnFocus = true) {
        open = false
        if (returnFocus) {
            // Return focus to the trigger
            void tick().then(() => buttonEl?.focus())
        }
    }

    function selectColor(color: VolumeTintColor) {
        value = color
        setSetting(id, color)
        closePopover()
    }

    function handleTriggerKeydown(event: KeyboardEvent) {
        if (event.key === 'Enter' || event.key === ' ' || event.key === 'ArrowDown') {
            event.preventDefault()
            openPopover()
        }
    }

    function handlePopoverKeydown(event: KeyboardEvent) {
        if (event.key === 'Escape') {
            event.preventDefault()
            closePopover()
            return
        }

        const items = Array.from(popoverEl?.querySelectorAll<HTMLButtonElement>('button[role="option"]') ?? [])
        if (items.length === 0) return

        const currentIndex = items.findIndex((el) => el === document.activeElement)
        const colsPerRow = 4

        const nextIndex = nextSwatchIndex(event.key, currentIndex, items.length, colsPerRow)
        if (nextIndex === null) return

        event.preventDefault()
        items[nextIndex]?.focus()
    }

    function handleDocumentPointerDown(event: PointerEvent) {
        if (!open) return
        const t = event.target as Node | null
        if (!t) return
        if (popoverEl?.contains(t) || buttonEl?.contains(t)) return
        closePopover(false)
    }

    $effect(() => {
        if (!open) return
        document.addEventListener('pointerdown', handleDocumentPointerDown, true)
        return () => document.removeEventListener('pointerdown', handleDocumentPointerDown, true)
    })

    const triggerLabel = $derived(`${label} (currently: ${colorLabel(value)})`)
</script>

<div class="picker-wrapper">
    <button
        bind:this={buttonEl}
        type="button"
        class="trigger"
        class:is-none={value === 'none'}
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-label={triggerLabel}
        style={value === 'none' ? undefined : `background-color: var(--color-tint-${value})`}
        onclick={() => (open ? closePopover() : openPopover())}
        onkeydown={handleTriggerKeydown}
    ></button>

    {#if open}
        <div
            bind:this={popoverEl}
            role="dialog"
            aria-label="Choose a tint color for {label}"
            class="popover"
            onkeydown={handlePopoverKeydown}
        >
            <div class="swatch-grid" role="listbox" aria-label="Tint colors">
                <button
                    type="button"
                    role="option"
                    aria-selected={value === 'none'}
                    aria-label="No tint"
                    data-selected={value === 'none'}
                    class="swatch is-none"
                    onclick={() => selectColor('none')}
                >
                    <span class="diagonal" aria-hidden="true"></span>
                </button>
                {#each VOLUME_TINT_COLORS as color (color)}
                    <button
                        type="button"
                        role="option"
                        aria-selected={value === color}
                        aria-label={colorLabel(color)}
                        data-selected={value === color}
                        class="swatch"
                        style={`background-color: var(--color-tint-${color})`}
                        onclick={() => selectColor(color)}
                    ></button>
                {/each}
            </div>
        </div>
    {/if}
</div>

<style>
    .picker-wrapper {
        position: relative;
        display: inline-block;
    }

    .trigger {
        width: 20px;
        height: 20px;
        border-radius: var(--radius-full);
        border: 1px solid var(--color-border-strong);
        background: transparent;
        cursor: default;
        padding: 0;
        display: inline-block;
        transition: transform var(--transition-fast);
    }

    .trigger:hover {
        transform: scale(1.1);
    }

    .trigger:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 2px;
    }

    .popover {
        position: absolute;
        top: calc(100% + var(--spacing-xs));
        right: 0;
        z-index: var(--z-dropdown);
        background: var(--color-bg-primary);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-md);
        padding: var(--spacing-sm);
        box-shadow: var(--shadow-md);
    }

    .swatch-grid {
        display: grid;
        grid-template-columns: repeat(4, 24px);
        gap: var(--spacing-xs);
    }

    .swatch {
        width: 24px;
        height: 24px;
        border-radius: var(--radius-full);
        border: 1px solid var(--color-border);
        padding: 0;
        cursor: default;
        position: relative;
        transition: transform var(--transition-fast);
    }

    .swatch:hover {
        transform: scale(1.15);
    }

    .swatch:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 2px;
    }

    .swatch[aria-selected='true'] {
        border-color: var(--color-text-primary);
        box-shadow: 0 0 0 2px var(--color-bg-primary), 0 0 0 3px var(--color-accent);
    }

    .swatch.is-none {
        background: var(--color-bg-primary);
    }

    /* Diagonal slash for the "no tint" option */
    .diagonal {
        position: absolute;
        inset: 0;
        display: block;
        background: linear-gradient(
            to top right,
            transparent calc(50% - 1px),
            var(--color-border-strong) calc(50% - 1px),
            var(--color-border-strong) calc(50% + 1px),
            transparent calc(50% + 1px)
        );
        border-radius: inherit;
    }
</style>
