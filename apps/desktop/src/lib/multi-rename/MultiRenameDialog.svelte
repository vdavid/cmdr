<script lang="ts">
    /**
     * Multi-Rename Tool (⌃M), Total Commander's: a name mask and an extension
     * mask with placeholders, search & replace, a case step, Greek to Latin,
     * removing diacritics, Unicode normalization, a counter, presets, and a live
     * preview of every row. Start renames the rows that are ready as one operation
     * (the queue shows it), and Undo rename (⌘⌥Z) rolls the session's last one back.
     * Results (⌥⏎) opens the new names in the user's text editor to change by hand
     * (`results.svelte.ts`), and ⌥⇧↓ in a text field lists what it held in earlier
     * renames (`field-history-menu.svelte.ts`). The preview list is `PreviewList.svelte`.
     *
     * Keyboard-first: the name mask has focus on open, Tab walks the fields, the
     * preview follows every keystroke, Enter starts, Esc closes, F2 opens the Presets
     * menu, ⌘S saves the fields as a preset, and ⌘⌥ plus a letter flips an option
     * (the key chip beside a whole-name option, or a search chip's tooltip, says which).
     */
    import { onDestroy, onMount, type Snippet } from 'svelte'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Menu from '$lib/ui/Menu.svelte'
    import StatusBadge from '$lib/ui/StatusBadge.svelte'
    import Button from '$lib/ui/Button.svelte'
    import Chip from '$lib/ui/Chip.svelte'
    import Checkbox from '$lib/ui/Checkbox.svelte'
    import LinkButton from '$lib/ui/LinkButton.svelte'
    import ShortcutChip from '$lib/ui/ShortcutChip.svelte'
    import Select from '$lib/ui/Select.svelte'
    import TextInput from '$lib/ui/TextInput.svelte'
    import Trans from '$lib/intl/Trans.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import type { MessageKey } from '$lib/intl/keys.gen'
    import { claimKey } from '$lib/shortcuts/claim-key'
    import { eventMatchesCommand } from '$lib/shortcuts'
    import { rollbackOperation } from '$lib/tauri-commands'
    import { asRollbackRefusal } from '$lib/operation-log/rollback-refusal'
    import { rollbackRefusalNotice } from '$lib/operation-log/operation-log-labels'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { claimMenuCommand } from '$lib/commands/menu-claims'
    import { getBadgeStatus } from '$lib/feature-status'
    import { getAppLogger } from '$lib/logging/logger'
    import type { MultiRenameError, MultiRenameOpened, MultiRenameStarted } from '$lib/tauri-commands'
    import type { CaseChange, HistoryField, SpecError } from '$lib/ipc/bindings'
    import { createMultiRenameState } from './multi-rename-state.svelte'
    import MaskInput from './MaskInput.svelte'
    import { TOGGLE_COMMANDS, WHOLE_NAME_TOGGLES, optionKeyOf, type ToggleField, type WholeNameField } from './option-keys'
    import { presetKeyOf, type PresetsControlApi } from './preset-keys'
    import PresetsControl from './PresetsControl.svelte'
    import FieldHistoryHint from './FieldHistoryHint.svelte'
    import PreviewList from './PreviewList.svelte'
    import { insertAtCaret } from './spec'
    import { PLACEHOLDER_HELP, placeholderExamples } from './placeholder-help'
    import PlaceholderTip from './PlaceholderTip.svelte'
    import { renderExamples } from './rename-examples'
    import { SEARCH_OPTIONS, searchOptionExamples } from './search-option-help'
    import SearchOptionChips from './SearchOptionChips.svelte'
    import { getLastMultiRenameRun, setLastMultiRenameRun, type MultiRenameRun } from './last-run.svelte'
    import { createResults } from './results.svelte'
    import { createFieldHistoryMenu } from './field-history-menu.svelte'

    interface Props {
        session: MultiRenameOpened
        onApplied: (started: MultiRenameStarted) => void
        /** Undo rename handed the last run's reversal to the queue. */
        onUndoStarted: (run: MultiRenameRun) => void
        onClose: () => void
    }

    const { session, onApplied, onUndoStarted, onClose }: Props = $props()

    const log = getAppLogger('multiRename')

    // Alpha badge policy: the status comes from the repo-root feature-status.json.
    const badge = getBadgeStatus('multi-rename')

    // One sheet renames one session; a new session remounts it.
    const tool = createMultiRenameState(session.sessionId)
    const results = createResults(tool)
    const fieldHistory = createFieldHistoryMenu({
        inputOf: (field) =>
            ({ nameMask: nameMaskInput, extensionMask: extensionMaskInput, search: searchInput, replace: replaceInput })[
                field
            ],
        fill: fillFromHistory,
    })

    let nameMaskInput = $state<HTMLInputElement>()
    let extensionMaskInput = $state<HTMLInputElement>()
    let searchInput = $state<HTMLInputElement>()
    let replaceInput = $state<HTMLInputElement>()
    /** Holds the Letter case `Select`, so ⌘⌥U can open it through its trigger. */
    let caseField = $state<HTMLElement>()
    let presetsControl = $state<PresetsControlApi>()

    /** Each placeholder button's tooltip body, which the tooltip adopts on show. */
    const tipContent = $state<Partial<Record<string, HTMLDivElement>>>({})
    /** Every tooltip's examples, rendered by the engine on made-up files; `null` until they come. */
    let examples = $state.raw<ReadonlyMap<string, string> | null>(null)

    async function loadExamples(): Promise<void> {
        const asked = new Map([...placeholderExamples(PLACEHOLDER_HELP), ...searchOptionExamples(SEARCH_OPTIONS)])
        examples = await renderExamples(asked)
    }

    const WHOLE_NAME_LABELS: Readonly<Record<WholeNameField, MessageKey>> = {
        greekToLatin: 'multiRename.greekToLatin',
        removeDiacritics: 'multiRename.removeDiacritics',
        normalizeUnicode: 'multiRename.normalizeUnicode',
    }

    const caseItems = $derived([
        { value: 'unchanged', label: tString('multiRename.case.unchanged') },
        { value: 'lower', label: tString('multiRename.case.lower') },
        { value: 'upper', label: tString('multiRename.case.upper') },
        { value: 'firstUpper', label: tString('multiRename.case.firstUpper') },
        { value: 'words', label: tString('multiRename.case.words') },
    ])

    /** The session's last run, which Undo rename rolls back; `null` when there's none. */
    const lastRun = $derived(getLastMultiRenameRun())
    let undoing = $state(false)
    /** Why Undo rename didn't go, worded by the operation log's refusal notices. */
    let undoNotice = $state<MessageKey | null>(null)

    const canStart = $derived(tool.counts.ready > 0 && tool.error === null && !tool.pending && !tool.applying)
    /** Results writes what the preview shows, so it waits for the same settled preview Start does. */
    const canResults = $derived(tool.error === null && !tool.pending && !tool.applying)
    const shownError = $derived(tool.error ?? tool.applyError ?? results.error)
    /** The error line's words: the preview's, Start's, or Results' error, else why Undo rename didn't go. */
    const shownMessage = $derived(shownError ? errorText(shownError) : undoNotice ? tString(undoNotice) : '')

    onMount(() => {
        void tool.loadPresets()
        void loadExamples()
        void fieldHistory.load()
        nameMaskInput?.focus()
        // F2 is also File > Rename's accelerator. Where AppKit runs the menu item instead of
        // (or as well as) handing the webview the key, its fire comes here too.
        return claimMenuCommand('file.rename', () => {
            presetsControl?.pressOpenKey('menu')
        })
    })

    onDestroy(() => {
        tool.dispose()
        fieldHistory.destroy()
        // The next ⌃M opens where this one left off.
        tool.persist().catch((e: unknown) => {
            log.warn("couldn't remember the multi-rename settings: {reason}", { reason: String(e) })
        })
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

    /** A history pick: the field takes the value (the menu hands focus back to it). */
    function fillFromHistory(field: HistoryField, value: string): void {
        tool.update({ [field]: value })
    }

    function toggle(field: ToggleField): void {
        tool.update({ [field]: !tool.spec[field] })
    }

    /** Opens the Letter case menu as a click on its trigger would (`.select-trigger` is `Select`'s stable class). */
    function openCaseMenu(): void {
        const trigger = caseField?.querySelector<HTMLElement>('.select-trigger')
        trigger?.focus()
        trigger?.click()
    }

    async function start(): Promise<void> {
        if (!canStart) return
        const started = await tool.apply()
        if (started) {
            setLastMultiRenameRun({ operationId: started.operationId, renaming: started.renaming })
            onApplied(started)
        }
    }

    function openResults(): void {
        if (canResults) void results.start()
    }

    /** Hands the last run's reversal to the queue, as the operation log's Roll back does. */
    async function undoLastRun(): Promise<void> {
        const run = lastRun
        if (!run || undoing) return
        undoing = true
        undoNotice = null
        try {
            await rollbackOperation(run.operationId)
            setLastMultiRenameRun(null)
            onUndoStarted(run)
        } catch (e) {
            const refusal = asRollbackRefusal(e)
            undoNotice = rollbackRefusalNotice(refusal)
            if (refusal?.kind === 'alreadyRolledBack') setLastMultiRenameRun(null)
            if (!refusal) log.warn("couldn't undo the last multi-rename: {error}", { error: String(e) })
        } finally {
            undoing = false
        }
    }

    /** The sheet's registry keys, which work from anywhere in it, a text field included. Claims what it answers. */
    function answerSheetKey(e: KeyboardEvent): boolean {
        // F2 and ⌘S.
        const presetKey = presetKeyOf(e)
        if (presetKey !== null) {
            claimKey(e)
            if (presetKey === 'openMenu') presetsControl?.pressOpenKey('keyboard')
            else presetsControl?.openSave()
            return true
        }
        // The option keys: ⌘⌥ plus a letter flips a checkbox or opens Letter case.
        const optionKey = optionKeyOf(e)
        if (optionKey !== null) {
            claimKey(e)
            if (optionKey.kind === 'toggle') toggle(optionKey.field)
            else openCaseMenu()
            return true
        }
        // ⌥⇧↓ in a text field lists its history; elsewhere it's left alone.
        const historyField = fieldHistory.fieldOf(e.target)
        if (historyField !== null && !e.isComposing && eventMatchesCommand(e, 'multiRename.fieldHistory')) {
            claimKey(e)
            fieldHistory.open(historyField)
            return true
        }
        // ⌥⏎ opens Results; claimed always, so it never falls through to Enter's Rename.
        if (!e.isComposing && eventMatchesCommand(e, 'multiRename.results')) {
            claimKey(e)
            openResults()
            return true
        }
        // ⌘⌥Z rolls back the last run; claimed even with none, so ⌥ never types `Ω` into a field.
        if (!e.isComposing && eventMatchesCommand(e, 'multiRename.undoRename')) {
            claimKey(e)
            void undoLastRun()
            return true
        }
        return false
    }

    function handleKeydown(e: KeyboardEvent): void {
        if (answerSheetKey(e)) return
        // Enter in a text field starts, as TC's Start! does; a button or menu keeps its own Enter.
        if (e.key !== 'Enter' || e.isComposing || e.metaKey || e.ctrlKey || e.altKey || e.shiftKey) return
        if (!(e.target instanceof HTMLInputElement) || e.target.type === 'checkbox') return
        e.preventDefault()
        void start()
    }

    function specErrorText(error: SpecError): string {
        if (error.type === 'badRegex') return tString('multiRename.error.badRegex')
        return error.error.type === 'unclosed'
            ? tString('multiRename.error.unclosed')
            : tString('multiRename.error.unknown', { placeholder: error.error.placeholder })
    }

    function errorText(error: MultiRenameError): string {
        switch (error.type) {
            case 'spec':
                return specErrorText(error.error)
            case 'gone':
            case 'sessionClosed':
                return tString('multiRename.error.gone')
            case 'selectionChanged':
                return tString('multiRename.selectionChanged')
            case 'nothingToRename':
                return tString('multiRename.error.nothingToRename')
            case 'notConnected':
                return tString('multiRename.error.notConnected')
            case 'previewOutOfDate':
                return tString('multiRename.error.previewOutOfDate')
            case 'readOnly':
                return tString('multiRename.error.readOnly')
            case 'timedOut':
                return tString('multiRename.error.timedOut')
            case 'couldntWriteNames':
                return tString('multiRename.error.couldntWriteNames')
            case 'namesFileGone':
                return tString('multiRename.results.gone')
            case 'couldntStart':
            case 'internal':
                return tString('multiRename.error.couldntStart')
        }
    }
</script>

<!-- Back from the text editor: Results reads the names it wrote (a no-op with no file out). -->
<svelte:window onfocus={() => { void results.readBack() }} />

<ModalDialog
    titleId="multi-rename-title"
    dialogId="multi-rename"
    resizable
    containerStyle="width: min(1100px, 92vw); height: min(760px, 88vh)"
    fillBody
    onclose={onClose}
    onkeydown={handleKeydown}
>
    <!-- The title bar's `<h2>` is already the row (gap + badge alignment live there),
         so the words and the badge are its direct children. -->
    {#snippet title()}
        <span>{tString('multiRename.title')}</span>{#if badge}<StatusBadge status={badge} />{/if}
    {/snippet}

    <div class="sheet">
        <div class="controls">
            <!-- First: the masks and the placeholders they take. -->
            <div class="fields">
                <div class="masks">
                    <label class="field grow">
                        <span class="label">{tString('multiRename.nameMask')}</span>
                        <MaskInput
                            bind:inputElement={nameMaskInput}
                            value={tool.spec.nameMask}
                            onValueChange={(nameMask: string) => { tool.update({ nameMask }) }}
                            ariaLabel={tString('multiRename.nameMask')}
                            invalid={tool.error?.type === 'spec' && tool.error.error.type === 'nameMask'}
                        />
                        {#if fieldHistory.historyOf('nameMask').length > 0}
                            <FieldHistoryHint onOpen={() => { fieldHistory.open('nameMask') }} />
                        {/if}
                    </label>
                    <label class="field extension">
                        <span class="label">{tString('multiRename.extensionMask')}</span>
                        <MaskInput
                            bind:inputElement={extensionMaskInput}
                            value={tool.spec.extensionMask}
                            onValueChange={(extensionMask: string) => { tool.update({ extensionMask }) }}
                            ariaLabel={tString('multiRename.extensionMask')}
                            invalid={tool.error?.type === 'spec' && tool.error.error.type === 'extensionMask'}
                        />
                        {#if fieldHistory.historyOf('extensionMask').length > 0}
                            <FieldHistoryHint onOpen={() => { fieldHistory.open('extensionMask') }} />
                        {/if}
                    </label>
                </div>
                <div class="placeholders" role="group" aria-label={tString('multiRename.insertPlaceholder')}>
                    {#each PLACEHOLDER_HELP as help (help.placeholder)}
                        <Chip
                            variant="insert"
                            mono
                            label={help.placeholder}
                            tooltipContent={{ contentEl: tipContent[help.placeholder] }}
                            onActivate={() => { insertPlaceholder(help.placeholder) }}
                        />
                        <PlaceholderTip {help} rendered={examples} bind:contentEl={tipContent[help.placeholder]} />
                    {/each}
                </div>
            </div>

            <!-- Second: the search, the replacement, and the search's own options as chips
                 level with the fields. -->
            <div class="search">
                <label class="field grow">
                    <span class="label">{tString('multiRename.search')}</span>
                    <TextInput
                        bind:inputElement={searchInput}
                        value={tool.spec.search}
                        oninput={(e: Event) => { tool.update({ search: (e.currentTarget as HTMLInputElement).value }) }}
                        ariaLabel={tString('multiRename.search')}
                        invalid={tool.error?.type === 'spec' && tool.error.error.type === 'badRegex'}
                    />
                    {#if fieldHistory.historyOf('search').length > 0}
                        <FieldHistoryHint onOpen={() => { fieldHistory.open('search') }} />
                    {/if}
                </label>
                <label class="field grow">
                    <span class="label">{tString('multiRename.replace')}</span>
                    <TextInput
                        bind:inputElement={replaceInput}
                        value={tool.spec.replace}
                        oninput={(e: Event) => { tool.update({ replace: (e.currentTarget as HTMLInputElement).value }) }}
                        ariaLabel={tString('multiRename.replace')}
                    />
                    {#if fieldHistory.historyOf('replace').length > 0}
                        <FieldHistoryHint onOpen={() => { fieldHistory.open('replace') }} />
                    {/if}
                </label>
                <SearchOptionChips spec={tool.spec} onToggle={toggle} rendered={examples} />
            </div>

            <!-- Third, as the rename runs them after search & replace: what changes the whole
                 name (case, Greek to Latin, diacritics, then the Unicode form), each option with
                 its quiet ⌘⌥ key chip. -->
            <div class="options">
                <span class="option-row">
                    <span class="case-field" bind:this={caseField}>
                        <span class="label">{tString('multiRename.case')}</span>
                        <Select
                            items={caseItems}
                            value={tool.spec.case}
                            onChange={(v: string) => { tool.update({ case: v as CaseChange }) }}
                            ariaLabel={tString('multiRename.case')}
                        />
                    </span>
                    <span class="option-key" aria-hidden="true">
                        <ShortcutChip commandId="multiRename.letterCase" clickable={false} size="sm" />
                    </span>
                </span>
                {#each WHOLE_NAME_TOGGLES as field (field)}
                    <span class="option-row">
                        <!-- One flex item: `Checkbox` renders more than one element. -->
                        <span class="option-control">
                            <Checkbox
                                checked={tool.spec[field]}
                                onCheckedChange={(on: boolean) => { tool.update({ [field]: on }) }}
                            >
                                {tString(WHOLE_NAME_LABELS[field])}
                            </Checkbox>
                        </span>
                        <!-- The key, quiet: a hint for next time, never a control (it can't be rebound). -->
                        <span class="option-key" aria-hidden="true">
                            <ShortcutChip commandId={TOGGLE_COMMANDS[field]} clickable={false} size="sm" />
                        </span>
                    </span>
                {/each}
            </div>
        </div>

        <!-- Always there, one line tall, so a message coming or going never moves the preview. An
             error wins; with none, Results says what it's doing, quietly. -->
        <div class="message-line">
            <p
                class="error"
                role="alert"
                use:tooltip={shownMessage ? { text: shownMessage, overflowOnly: true } : undefined}
            >
                {shownMessage}
            </p>
            {#if !shownMessage && (results.open || tool.editedCount > 0)}
                <p class="results-notice" role="status">
                    <span class="results-words">
                        {tool.editedCount > 0
                            ? tString('multiRename.results.edited', { count: tool.editedCount })
                            : tString('multiRename.results.editing')}
                    </span>
                    {#if results.open}
                        <LinkButton onclick={() => { void results.readBack() }}>
                            {tString('multiRename.results.readNow')}
                        </LinkButton>
                    {/if}
                    {#if tool.editedCount > 0}
                        <LinkButton onclick={() => { void results.discard() }}>
                            {tString('multiRename.results.discard')}
                        </LinkButton>
                    {/if}
                </p>
            {/if}
        </div>

        <PreviewList rows={tool.source} />

        <!-- Inside the dialog, so it portals into the modal's layer (above the scrim, inside its focus trap). -->
        <Menu menu={fieldHistory.menu} ariaLabel={tString('multiRename.history')} minWidth={240} />
    </div>

    {#snippet footerLeading()}
        <div class="footer-leading">
            <PresetsControl bind:this={presetsControl} {tool} />
            <span class="counts" role="status">
                <Trans
                    key="multiRename.summary"
                    params={{ ready: tool.counts.ready, unchanged: tool.counts.unchanged, problems: tool.counts.problems }}
                    snippets={{ problemsToggle }}
                />
            </span>
        </div>
    {/snippet}
    {#snippet footer()}
        {#if lastRun}
            <!-- Quiet: a way back, not the sheet's next step. Only there while there's a run to undo. -->
            <span
                class="footer-link"
                use:tooltip={{ text: tString('multiRename.undoTooltip', { count: lastRun.renaming }) }}
            >
                <LinkButton disabled={undoing} onclick={() => { void undoLastRun() }}>
                    {tString('multiRename.undo')}
                </LinkButton>
                <span class="option-key" aria-hidden="true">
                    <ShortcutChip commandId="multiRename.undoRename" clickable={false} size="sm" />
                </span>
            </span>
        {/if}
        <!-- Quiet too: a side road to the names, never the sheet's next step. -->
        <span class="footer-link" use:tooltip={{ text: tString('multiRename.resultsTooltip') }}>
            <LinkButton disabled={!canResults} onclick={openResults}>
                {tString('multiRename.results')}
            </LinkButton>
            <span class="option-key" aria-hidden="true">
                <ShortcutChip commandId="multiRename.results" clickable={false} size="sm" />
            </span>
        </span>
        <Button onclick={onClose}>{tString('multiRename.cancel')}</Button>
        <Button variant="primary" onclick={() => { void start() }} disabled={!canStart}>
            {tString('multiRename.rename', { count: tool.counts.ready })}
        </Button>
    {/snippet}
</ModalDialog>

<!-- "N problems" in the summary: a toggle that lists the problem rows alone while there are any. -->
{#snippet problemsToggle(children: Snippet)}
    {#if tool.counts.problems > 0}
        <LinkButton aria-pressed={tool.problemsOnly} onclick={() => { tool.setProblemsOnly(!tool.problemsOnly) }}>
            {@render children()}
        </LinkButton>
    {:else}
        {@render children()}
    {/if}
{/snippet}

<style>
    .sheet {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
        height: 100%;
        min-height: 0;
    }

    /* Three rows across the full width, in the order a rename runs them: the masks, search &
       replace, then case and diacritics. A wider gap between them than inside them. */
    .controls {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-lg);
    }

    .fields {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
        min-width: 0;
    }

    .masks,
    .search {
        display: flex;
        gap: var(--spacing-md);
        align-items: flex-end;
    }

    .field {
        position: relative;
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
        min-width: 0;
    }

    .grow {
        flex: 1 1 0;
    }

    /* An extension mask is short: `[E]`, or a counter at most. */
    .extension {
        flex: 0 0 140px;
    }

    .label {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    /* A field's history hint shows only while the field is in use (`FieldHistoryHint.svelte`). */
    .field :global(.history-hint) {
        opacity: 0;
        transition: opacity var(--transition-fast);
    }

    .field:hover :global(.history-hint),
    .field:focus-within :global(.history-hint) {
        opacity: 1;
    }

    .placeholders {
        display: flex;
        gap: var(--spacing-xs);
        flex-wrap: wrap;
    }

    /* One row of whole-name options, each with its key chip beside it. */
    .options {
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        gap: var(--spacing-sm) var(--spacing-xl);
    }

    .option-control {
        display: flex;
    }

    .option-row {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
    }

    .case-field {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
    }

    /* The key hint stays quiet: tertiary text on no fill, so it doesn't shout. */
    .option-key {
        display: flex;
    }

    .option-key :global(.shortcut-chip) {
        color: var(--color-text-tertiary);
        background: transparent;
    }

    /* One reserved line under the search row: a long message ends in an ellipsis (the whole of it
       on hover) rather than growing the line. */
    .message-line {
        display: flex;
        gap: var(--spacing-sm);
        min-width: 0;
    }

    .error {
        flex: 0 1 auto;
        min-width: 0;
        margin: 0;
        min-height: calc(var(--font-size-sm) * var(--font-line-height-normal));
        overflow: hidden;
        white-space: nowrap;
        text-overflow: ellipsis;
        color: var(--color-error-text);
        font-size: var(--font-size-sm);
        line-height: var(--font-line-height-normal);
    }

    /* A status, not a warning: secondary words, its links inline, the line's one line kept. */
    .results-notice {
        display: flex;
        align-items: baseline;
        gap: var(--spacing-sm);
        min-width: 0;
        margin: 0;
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        white-space: nowrap;
    }

    .results-words {
        overflow: hidden;
        text-overflow: ellipsis;
    }

    .footer-link {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        margin-right: var(--spacing-sm);
    }

    .footer-leading {
        display: flex;
        align-items: center;
        gap: var(--spacing-md);
        min-width: 0;
    }

    .counts {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }
</style>
