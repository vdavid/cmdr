<script lang="ts">
    /**
     * The Multi-rename sheet's presets: one button (F2) that opens the Presets menu
     * (saved presets numbered 1–9, built-ins, Reset all fields, Save current as…, and a
     * Rename / Update / Delete submenu per saved preset), plus the name popover that
     * ⌘S and Rename… open. The sheet owns the keys and calls `pressOpenKey` / `openSave`;
     * everything else lives here.
     */
    import { onDestroy, onMount } from 'svelte'
    import Button from '$lib/ui/Button.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import Menu from '$lib/ui/Menu.svelte'
    import Popover from '$lib/ui/Popover.svelte'
    import ShortcutChip from '$lib/ui/ShortcutChip.svelte'
    import TextInput from '$lib/ui/TextInput.svelte'
    import { createMenu } from '$lib/ui/menu-controller.svelte'
    import type { MenuItem, MenuRowContext } from '$lib/ui/menu-types'
    import { claimKey } from '$lib/shortcuts/claim-key'
    import { getFirstShortcutReactive } from '$lib/shortcuts/reactive-shortcuts.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { getAppLogger } from '$lib/logging/logger'
    import type { MultiRenameState } from './multi-rename-state.svelte'
    import { createKeyRoadEcho, presetKeyOf, type KeyRoad } from './preset-keys'
    import { presetMenuSections, presetNameClash, type PresetAction, type PresetNameMode } from './preset-menu'
    import { BUILT_IN_PRESETS } from './spec'

    interface Props {
        tool: MultiRenameState
    }

    const { tool }: Props = $props()

    const log = getAppLogger('multiRename')

    let wrapperEl = $state<HTMLSpanElement>()
    // The house `Button` doesn't hand out its element, so the menu and the popover find it.
    let buttonEl = $state<HTMLButtonElement>()
    let nameInput = $state<HTMLInputElement>()

    let nameMode = $state<PresetNameMode | null>(null)
    let name = $state('')
    /** The preset Enter would replace, once the user was told; editing the name forgets it. */
    let confirmedClash = $state<string | null>(null)

    const openShortcut = $derived(getFirstShortcutReactive('multiRename.openPresets'))

    // F2 can arrive twice for one press: the keydown, and File > Rename's accelerator.
    const isEcho = createKeyRoadEcho()

    const loadedName = $derived.by(() => {
        const loaded = tool.loaded
        if (!loaded) return null
        if (loaded.kind === 'saved') return tool.presets.find((p) => p.id === loaded.id)?.name ?? null
        const builtIn = BUILT_IN_PRESETS.find((p) => p.id === loaded.id)
        return builtIn ? tString(builtIn.nameKey) : null
    })

    const clash = $derived(nameMode ? presetNameClash(nameMode, name, tool.presetNamed) : undefined)

    const menu = createMenu<PresetAction>({
        getSections: () =>
            presetMenuSections({
                saved: tool.presets,
                builtIns: BUILT_IN_PRESETS.map((p) => ({ id: p.id, name: tString(p.nameKey) })),
                loaded: tool.loaded,
                current: tool.spec,
                labels: {
                    reset: tString('multiRename.presets.reset'),
                    saveAs: tString('multiRename.presets.saveAs'),
                    rename: tString('multiRename.presets.rename'),
                    update: tString('multiRename.presets.update'),
                    delete: tString('multiRename.presets.delete'),
                },
            }),
        onSelect: (item: MenuItem<PresetAction>) => {
            if (item.data) act(item.data)
        },
        onKey: (event: KeyboardEvent) => {
            // F2 again closes the menu (unless it's the echo of the F2 that opened it), and ⌘S
            // goes straight to saving, as it does anywhere in the sheet.
            const key = presetKeyOf(event)
            if (key === 'openMenu') {
                pressOpenKey('keyboard')
                return true
            }
            if (key === 'save') {
                menu.close()
                openSave()
                return true
            }
            return false
        },
        restoreFocus: () => {
            buttonEl?.focus()
        },
    })

    onMount(() => {
        buttonEl = wrapperEl?.querySelector('button') ?? undefined
    })

    onDestroy(() => {
        menu.destroy()
    })

    function act(action: PresetAction): void {
        switch (action.kind) {
            case 'load':
                tool.loadPreset(action.preset)
                return
            case 'reset':
                tool.resetFields()
                return
            case 'saveAs':
                openSave()
                return
            case 'rename':
                openName({ kind: 'rename', id: action.id }, tool.presets.find((p) => p.id === action.id)?.name ?? '')
                return
            case 'update':
                void run('update', () => tool.updatePreset(action.id))
                return
            case 'delete':
                void run('delete', () => tool.deletePreset(action.id))
                return
        }
    }

    async function run(what: string, change: () => Promise<void>): Promise<void> {
        try {
            await change()
        } catch (e) {
            log.warn("couldn't {what} a multi-rename preset: {reason}", { what, reason: String(e) })
        }
    }

    /** Opens or closes the Presets menu under its button (a click). */
    function openMenu(): void {
        if (nameMode !== null || !buttonEl) return
        menu.toggleUnder(buttonEl)
    }

    /** F2, from the sheet's keydown or File > Rename's accelerator: toggles the menu, once per press. */
    export function pressOpenKey(road: KeyRoad): void {
        if (isEcho(road)) return
        openMenu()
    }

    /** Opens the name popover to save the fields (⌘S), prefilled with the loaded saved preset's name. */
    export function openSave(): void {
        const loaded = tool.loaded
        openName({ kind: 'save' }, loaded?.kind === 'saved' ? (loadedName ?? '') : '')
    }

    function openName(mode: PresetNameMode, prefill: string): void {
        if (nameMode !== null) return
        name = prefill
        confirmedClash = null
        nameMode = mode
        // The popover focuses its first field; select the prefill so typing replaces it.
        queueMicrotask(() => {
            requestAnimationFrame(() => nameInput?.select())
        })
    }

    function closeName(): void {
        nameMode = null
    }

    async function submitName(): Promise<void> {
        const mode = nameMode
        if (!mode || name.trim() === '') return
        if (clash && confirmedClash !== clash.id) {
            confirmedClash = clash.id
            return
        }
        closeName()
        buttonEl?.focus()
        if (mode.kind === 'save') await run('save', () => tool.savePreset(name))
        else await run('rename', () => tool.renamePreset({ id: mode.id, name }))
    }

    function handleNameKeydown(e: KeyboardEvent): void {
        // Enter here saves (or confirms the replace); it never starts a rename of files.
        if (e.key !== 'Enter' || e.isComposing) return
        claimKey(e)
        void submitName()
    }
</script>

<span class="presets" bind:this={wrapperEl}>
    <Button
        onclick={openMenu}
        tooltipContent={{ text: tString('multiRename.presets.tooltip'), shortcut: openShortcut }}
    >
        <span class="presets-label">
            {tString('multiRename.presets.title')}
            <Icon name="chevron-down" size={14} aria-hidden="true" />
            {#if loadedName !== null}
                <span class="loaded-name">
                    {tool.edited ? tString('multiRename.presets.edited', { preset: loadedName }) : loadedName}
                </span>
            {/if}
        </span>
    </Button>
</span>

<Menu {menu} ariaLabel={tString('multiRename.presets.title')} minWidth={240}>
    {#snippet trailing(ctx: MenuRowContext<PresetAction>)}
        {#if ctx.item.data?.kind === 'saveAs'}
            <ShortcutChip commandId="multiRename.savePreset" clickable={false} size="sm" />
        {/if}
    {/snippet}
</Menu>

{#if buttonEl}
    <Popover
        anchor={buttonEl}
        open={nameMode !== null}
        onClose={closeName}
        ariaLabel={nameMode?.kind === 'rename'
            ? tString('multiRename.presets.renameTitle')
            : tString('multiRename.presets.saveTitle')}
    >
        <div class="name-form">
            <label class="field">
                <span class="label">{tString('multiRename.presetName')}</span>
                <TextInput
                    bind:inputElement={nameInput}
                    value={name}
                    oninput={(e: Event) => {
                        name = (e.currentTarget as HTMLInputElement).value
                        confirmedClash = null
                    }}
                    onkeydown={handleNameKeydown}
                    ariaLabel={tString('multiRename.presetName')}
                    warning={clash !== undefined && confirmedClash === clash.id}
                />
            </label>
            {#if clash !== undefined && confirmedClash === clash.id}
                <p class="clash" role="alert">{tString('multiRename.presets.clash', { preset: clash.name })}</p>
            {/if}
            <div class="actions">
                <Button variant="primary" size="mini" disabled={name.trim() === ''} onclick={() => { void submitName() }}>
                    {#if clash !== undefined && confirmedClash === clash.id}
                        {tString('multiRename.presets.replace')}
                    {:else if nameMode?.kind === 'rename'}
                        {tString('multiRename.presets.renameConfirm')}
                    {:else}
                        {tString('multiRename.presets.save')}
                    {/if}
                </Button>
            </div>
        </div>
    </Popover>
{/if}

<style>
    .presets {
        display: inline-flex;
    }

    .presets-label {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xxs);
        max-width: 320px;
    }

    .loaded-name {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        color: var(--color-text-secondary);
    }

    .name-form {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
        width: 260px;
    }

    .field {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
    }

    .label {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .clash {
        margin: 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .actions {
        display: flex;
        justify-content: flex-end;
    }
</style>
