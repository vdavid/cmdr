<script lang="ts">
    /**
     * Multi-Rename Tool (⌃M), Total Commander's: a name mask and an extension
     * mask with placeholders, search & replace, a case step, removing diacritics,
     * a counter, presets, and a live preview of every row. Start renames the
     * rows that are ready as one operation (the queue shows it; Undo reverses it).
     *
     * Keyboard-first: the name mask has focus on open, Tab walks the fields, the
     * preview follows every keystroke, Enter starts, Esc closes.
     */
    import { onDestroy, onMount } from 'svelte'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import Checkbox from '$lib/ui/Checkbox.svelte'
    import NumberInput from '$lib/ui/NumberInput.svelte'
    import Select from '$lib/ui/Select.svelte'
    import TextInput from '$lib/ui/TextInput.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { getAppLogger } from '$lib/logging/logger'
    import type { MultiRenameError, MultiRenameStarted, PreviewRow } from '$lib/tauri-commands'
    import type { CaseChange } from '$lib/ipc/bindings'
    import { createMultiRenameState, type MultiRenameTarget } from './multi-rename-state.svelte'
    import { BUILT_IN_PRESETS, DEFAULT_SPEC, insertAtCaret } from './spec'

    interface Props {
        target: MultiRenameTarget
        onApplied: (started: MultiRenameStarted) => void
        onClose: () => void
    }

    const { target, onApplied, onClose }: Props = $props()

    const log = getAppLogger('multiRename')

    // One sheet renames one target; a new target remounts it.
    const tool = createMultiRenameState(target)

    let nameMaskInput = $state<HTMLInputElement>()
    let presetName = $state('')
    let selectedPresetId = $state('')

    /** How many preview rows the table draws; the rest still rename. */
    const SHOWN_ROWS = 1000

    const PLACEHOLDERS = ['[N]', '[E]', '[P]', '[C]', '[YMD]', '[hms]'] as const

    const caseItems = $derived([
        { value: 'unchanged', label: tString('multiRename.case.unchanged') },
        { value: 'lower', label: tString('multiRename.case.lower') },
        { value: 'upper', label: tString('multiRename.case.upper') },
        { value: 'firstUpper', label: tString('multiRename.case.firstUpper') },
        { value: 'words', label: tString('multiRename.case.words') },
    ])

    const presetItems = $derived([
        { value: 'default', label: tString('multiRename.preset.default') },
        ...BUILT_IN_PRESETS.map((p) => ({ value: p.id, label: tString(p.nameKey) })),
        ...tool.presets.map((p) => ({ value: p.id, label: p.name })),
    ])

    const canStart = $derived(tool.counts.ready > 0 && tool.error === null && !tool.pending && !tool.applying)
    const shownError = $derived(tool.error ?? tool.applyError)
    const hiddenProblems = $derived(
        tool.rows
            .slice(SHOWN_ROWS)
            .filter((r) => r.status.type !== 'ready' && r.status.type !== 'unchanged').length,
    )

    onMount(() => {
        void tool.loadPresets()
        nameMaskInput?.focus()
    })

    onDestroy(() => {
        tool.dispose()
    })

    function insertPlaceholder(placeholder: string): void {
        const caret = nameMaskInput?.selectionStart ?? null
        const next = insertAtCaret(tool.spec.nameMask, placeholder, caret)
        tool.update({ nameMask: next.mask })
        queueMicrotask(() => {
            nameMaskInput?.focus()
            nameMaskInput?.setSelectionRange(next.caret, next.caret)
        })
    }

    function loadPreset(id: string): void {
        selectedPresetId = id
        if (id === 'default') {
            tool.load(DEFAULT_SPEC)
            return
        }
        const builtIn = BUILT_IN_PRESETS.find((p) => p.id === id)
        const saved = tool.presets.find((p) => p.id === id)
        const spec = builtIn?.spec ?? saved?.spec
        if (spec) tool.load(spec)
        if (saved) presetName = saved.name
    }

    async function savePreset(): Promise<void> {
        if (presetName.trim() === '') return
        try {
            await tool.savePreset(presetName)
        } catch (e) {
            log.warn("couldn't save a multi-rename preset: {reason}", { reason: String(e) })
            return
        }
        const saved = tool.presets.find((p) => p.name.trim().toLowerCase() === presetName.trim().toLowerCase())
        if (saved) selectedPresetId = saved.id
    }

    async function deletePreset(): Promise<void> {
        if (!tool.presets.some((p) => p.id === selectedPresetId)) return
        try {
            await tool.deletePreset(selectedPresetId)
        } catch (e) {
            log.warn("couldn't delete a multi-rename preset: {reason}", { reason: String(e) })
            return
        }
        selectedPresetId = ''
        presetName = ''
    }

    function handlePresetNameKeydown(e: KeyboardEvent): void {
        // Enter here saves the preset; it never starts a rename.
        if (e.key !== 'Enter' || e.isComposing) return
        e.preventDefault()
        e.stopPropagation()
        void savePreset()
    }

    async function start(): Promise<void> {
        if (!canStart) return
        const started = await tool.apply()
        if (started) onApplied(started)
    }

    function handleKeydown(e: KeyboardEvent): void {
        // Enter in a text field starts, as TC's Start! does; a button or menu keeps its own Enter.
        if (e.key !== 'Enter' || e.isComposing || e.metaKey || e.ctrlKey || e.altKey || e.shiftKey) return
        if (!(e.target instanceof HTMLInputElement) || e.target.type === 'checkbox') return
        e.preventDefault()
        void start()
    }

    function statusText(row: PreviewRow): string {
        switch (row.status.type) {
            case 'ready':
                return ''
            case 'unchanged':
                return tString('multiRename.status.unchanged')
            case 'duplicate':
                return tString('multiRename.status.duplicate')
            case 'targetExists':
                return tString('multiRename.status.targetExists')
            case 'invalidName':
                return row.status.reason.type === 'disallowedCharacter'
                    ? tString('multiRename.status.disallowedCharacter', { character: row.status.reason.character })
                    : tString('multiRename.status.invalidName')
        }
    }

    function errorText(error: MultiRenameError): string {
        switch (error.type) {
            case 'spec':
                if (error.error.type === 'badRegex') return tString('multiRename.error.badRegex')
                return error.error.error.type === 'unclosed'
                    ? tString('multiRename.error.unclosed')
                    : tString('multiRename.error.unknown', { placeholder: error.error.error.placeholder })
            case 'gone':
                return tString('multiRename.error.gone')
            case 'nothingToRename':
                return tString('multiRename.error.nothingToRename')
            case 'notConnected':
                return tString('multiRename.error.notConnected')
            case 'previewOutOfDate':
                return tString('multiRename.error.previewOutOfDate')
            case 'readOnly':
                return tString('multiRename.error.readOnly')
            case 'couldntStart':
            case 'timedOut':
            case 'internal':
                return tString('multiRename.error.couldntStart')
        }
    }
</script>

<ModalDialog
    titleId="multi-rename-title"
    dialogId="multi-rename"
    resizable
    containerStyle="width: min(1100px, 92vw); height: min(760px, 88vh)"
    fillBody
    onclose={onClose}
    onkeydown={handleKeydown}
>
    {#snippet title()}{tString('multiRename.title')}{/snippet}

    <div class="sheet">
        <div class="masks">
            <label class="field grow">
                <span class="label">{tString('multiRename.nameMask')}</span>
                <TextInput
                    mono
                    bind:inputElement={nameMaskInput}
                    value={tool.spec.nameMask}
                    oninput={(e: Event) => { tool.update({ nameMask: (e.currentTarget as HTMLInputElement).value }) }}
                    ariaLabel={tString('multiRename.nameMask')}
                    invalid={tool.error?.type === 'spec' && tool.error.error.type === 'nameMask'}
                />
            </label>
            <label class="field">
                <span class="label">{tString('multiRename.extensionMask')}</span>
                <TextInput
                    mono
                    value={tool.spec.extensionMask}
                    oninput={(e: Event) => { tool.update({ extensionMask: (e.currentTarget as HTMLInputElement).value }) }}
                    ariaLabel={tString('multiRename.extensionMask')}
                    invalid={tool.error?.type === 'spec' && tool.error.error.type === 'extensionMask'}
                />
            </label>
        </div>
        <div class="placeholders" role="group" aria-label={tString('multiRename.insertPlaceholder')}>
            {#each PLACEHOLDERS as placeholder (placeholder)}
                <Button size="mini" onclick={() => { insertPlaceholder(placeholder) }}>{placeholder}</Button>
            {/each}
        </div>

        <div class="row">
            <label class="field grow">
                <span class="label">{tString('multiRename.search')}</span>
                <TextInput
                    value={tool.spec.search}
                    oninput={(e: Event) => { tool.update({ search: (e.currentTarget as HTMLInputElement).value }) }}
                    ariaLabel={tString('multiRename.search')}
                    invalid={tool.error?.type === 'spec' && tool.error.error.type === 'badRegex'}
                />
            </label>
            <label class="field grow">
                <span class="label">{tString('multiRename.replace')}</span>
                <TextInput
                    value={tool.spec.replace}
                    oninput={(e: Event) => { tool.update({ replace: (e.currentTarget as HTMLInputElement).value }) }}
                    ariaLabel={tString('multiRename.replace')}
                />
            </label>
        </div>
        <div class="row options">
            <Checkbox checked={tool.spec.caseSensitive} onCheckedChange={(v: boolean) => { tool.update({ caseSensitive: v }) }}>
                {tString('multiRename.caseSensitive')}
            </Checkbox>
            <Checkbox checked={tool.spec.firstOnly} onCheckedChange={(v: boolean) => { tool.update({ firstOnly: v }) }}>
                {tString('multiRename.firstOnly')}
            </Checkbox>
            <Checkbox checked={tool.spec.includeExtension} onCheckedChange={(v: boolean) => { tool.update({ includeExtension: v }) }}>
                {tString('multiRename.includeExtension')}
            </Checkbox>
            <Checkbox checked={tool.spec.regex} onCheckedChange={(v: boolean) => { tool.update({ regex: v }) }}>
                {tString('multiRename.regex')}
            </Checkbox>
            <Checkbox checked={tool.spec.substitute} onCheckedChange={(v: boolean) => { tool.update({ substitute: v }) }}>
                {tString('multiRename.substitute')}
            </Checkbox>
        </div>

        <div class="row options">
            <div class="field">
                <span class="label">{tString('multiRename.case')}</span>
                <Select
                    items={caseItems}
                    value={tool.spec.case}
                    onChange={(v: string) => { tool.update({ case: v as CaseChange }) }}
                    ariaLabel={tString('multiRename.case')}
                />
            </div>
            <Checkbox checked={tool.spec.removeDiacritics} onCheckedChange={(v: boolean) => { tool.update({ removeDiacritics: v }) }}>
                {tString('multiRename.removeDiacritics')}
            </Checkbox>
            <div class="field">
                <span class="label">{tString('multiRename.counterStart')}</span>
                <NumberInput
                    value={tool.spec.counterStart}
                    min={-1000000}
                    max={1000000}
                    onChange={(n: number) => { tool.update({ counterStart: n }) }}
                    ariaLabel={tString('multiRename.counterStart')}
                />
            </div>
            <div class="field">
                <span class="label">{tString('multiRename.counterStep')}</span>
                <NumberInput
                    value={tool.spec.counterStep}
                    min={-1000}
                    max={1000}
                    onChange={(n: number) => { tool.update({ counterStep: n }) }}
                    ariaLabel={tString('multiRename.counterStep')}
                />
            </div>
            <div class="field">
                <span class="label">{tString('multiRename.counterDigits')}</span>
                <NumberInput
                    value={tool.spec.counterDigits}
                    min={1}
                    max={10}
                    onChange={(n: number) => { tool.update({ counterDigits: n }) }}
                    ariaLabel={tString('multiRename.counterDigits')}
                />
            </div>
        </div>

        <div class="row">
            <div class="field grow">
                <span class="label">{tString('multiRename.presets')}</span>
                <Select
                    items={presetItems}
                    value={selectedPresetId}
                    onChange={loadPreset}
                    placeholder={tString('multiRename.presetPlaceholder')}
                    ariaLabel={tString('multiRename.presets')}
                />
            </div>
            <label class="field grow">
                <span class="label">{tString('multiRename.presetName')}</span>
                <TextInput
                    value={presetName}
                    oninput={(e: Event) => { presetName = (e.currentTarget as HTMLInputElement).value }}
                    onkeydown={handlePresetNameKeydown}
                    ariaLabel={tString('multiRename.presetName')}
                />
            </label>
            <Button onclick={() => { void savePreset() }} disabled={presetName.trim() === ''}>
                {tString('multiRename.savePreset')}
            </Button>
            <Button
                onclick={() => { void deletePreset() }}
                disabled={!tool.presets.some((p) => p.id === selectedPresetId)}
            >
                {tString('multiRename.deletePreset')}
            </Button>
        </div>

        {#if shownError}
            <p class="error" role="alert">{errorText(shownError)}</p>
        {/if}

        <div class="preview" role="region" aria-label={tString('multiRename.preview')}>
            <table>
                <thead>
                    <tr>
                        <th>{tString('multiRename.oldName')}</th>
                        <th>{tString('multiRename.newName')}</th>
                        <th>{tString('multiRename.statusColumn')}</th>
                    </tr>
                </thead>
                <tbody>
                    {#each tool.rows.slice(0, SHOWN_ROWS) as row (row.row)}
                        <tr class:problem={row.status.type !== 'ready' && row.status.type !== 'unchanged'}>
                            <td class="name">{row.oldName}</td>
                            <td class="name" class:unchanged={row.status.type === 'unchanged'}>{row.newName}</td>
                            <td class="status">{statusText(row)}</td>
                        </tr>
                    {/each}
                </tbody>
            </table>
            {#if tool.rows.length > SHOWN_ROWS}
                <p class="more">{tString('multiRename.moreRows', { count: tool.rows.length - SHOWN_ROWS })}</p>
            {/if}
            {#if hiddenProblems > 0}
                <p class="more problem-note">{tString('multiRename.hiddenProblems', { count: hiddenProblems })}</p>
            {/if}
        </div>
    </div>

    {#snippet footerLeading()}
        <span class="counts" role="status">
            {tString('multiRename.counts', {
                ready: tool.counts.ready,
                unchanged: tool.counts.unchanged,
                problems: tool.counts.problems,
            })}
        </span>
    {/snippet}
    {#snippet footer()}
        <Button onclick={onClose}>{tString('multiRename.cancel')}</Button>
        <Button variant="primary" onclick={() => { void start() }} disabled={!canStart}>
            {tString('multiRename.rename', { count: tool.counts.ready })}
        </Button>
    {/snippet}
</ModalDialog>

<style>
    .sheet {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
        height: 100%;
        min-height: 0;
    }

    .masks,
    .row {
        display: flex;
        flex-wrap: wrap;
        gap: var(--spacing-md);
        align-items: flex-end;
    }

    .options {
        align-items: center;
    }

    .field {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
        min-width: 0;
    }

    .grow {
        flex: 1 1 220px;
    }

    .label {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .placeholders {
        display: flex;
        gap: var(--spacing-xs);
        flex-wrap: wrap;
    }

    .error {
        margin: 0;
        color: var(--color-error-text);
        font-size: var(--font-size-sm);
    }

    .preview {
        flex: 1 1 auto;
        min-height: 0;
        overflow: auto;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
    }

    table {
        width: 100%;
        border-collapse: collapse;
        font-size: var(--font-size-sm);
    }

    th {
        position: sticky;
        top: 0;
        text-align: left;
        background: var(--color-bg-secondary);
        padding: var(--spacing-xxs) var(--spacing-sm);
        font-weight: normal;
        color: var(--color-text-secondary);
    }

    td {
        padding: var(--spacing-xxs) var(--spacing-sm);
        border-top: 1px solid var(--color-border);
    }

    .name {
        font-family: var(--font-mono);
        white-space: pre;
    }

    .unchanged {
        color: var(--color-text-quiet);
    }

    .problem .status {
        color: var(--color-error-text);
    }

    .status {
        color: var(--color-text-secondary);
        white-space: nowrap;
    }

    .more,
    .counts {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .more {
        margin: var(--spacing-xs) var(--spacing-sm);
    }

    .problem-note {
        color: var(--color-error-text);
    }
</style>
