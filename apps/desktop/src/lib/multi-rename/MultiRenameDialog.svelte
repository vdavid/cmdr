<script lang="ts">
    /**
     * Multi-Rename Tool (⌃M), Total Commander's: a name mask and an extension
     * mask with placeholders, search & replace, a case step, removing diacritics,
     * a counter, presets, and a live preview of every row. Start renames the
     * rows that are ready as one operation (the queue shows it; Undo reverses it).
     * The preview is the house `ColumnList` over a windowed source: the rows come a
     * page at a time, and it draws only the ones in view.
     *
     * Keyboard-first: the name mask has focus on open, Tab walks the fields, the
     * preview follows every keystroke, Enter starts, Esc closes, F2 opens the Presets
     * menu, ⌘S saves the fields as a preset, and ⌘⌥ plus a letter flips an option
     * (the key chip beside a whole-name option, or a search chip's tooltip, says which).
     */
    import { onDestroy, onMount, type Snippet } from 'svelte'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import StatusBadge from '$lib/ui/StatusBadge.svelte'
    import StatusGlyph from '$lib/ui/StatusGlyph.svelte'
    import Button from '$lib/ui/Button.svelte'
    import Checkbox from '$lib/ui/Checkbox.svelte'
    import LinkButton from '$lib/ui/LinkButton.svelte'
    import ShortcutChip from '$lib/ui/ShortcutChip.svelte'
    import ColumnList from '$lib/ui/ColumnList.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import { columnListProps, type ColumnListCellContext, type ColumnListColumn } from '$lib/ui/column-list-types'
    import { getCachedIcon, iconCacheVersion } from '$lib/icon-cache'
    import { useShortenMiddle } from '$lib/utils/shorten-middle-action'
    import Select from '$lib/ui/Select.svelte'
    import TextInput from '$lib/ui/TextInput.svelte'
    import Trans from '$lib/intl/Trans.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { claimKey } from '$lib/shortcuts/claim-key'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { claimMenuCommand } from '$lib/commands/menu-claims'
    import { getBadgeStatus } from '$lib/feature-status'
    import type { MultiRenameError, MultiRenameOpened, MultiRenameStarted, PreviewRow } from '$lib/tauri-commands'
    import type { CaseChange } from '$lib/ipc/bindings'
    import { createMultiRenameState } from './multi-rename-state.svelte'
    import MaskInput from './MaskInput.svelte'
    import { TOGGLE_COMMANDS, optionKeyOf, type ToggleField } from './option-keys'
    import { presetKeyOf, type PresetsControlApi } from './preset-keys'
    import PresetsControl from './PresetsControl.svelte'
    import { rowStatusView, type StatusMessage } from './row-status'
    import { insertAtCaret } from './spec'
    import { PLACEHOLDER_HELP, placeholderExamples } from './placeholder-help'
    import PlaceholderTip from './PlaceholderTip.svelte'
    import { renderExamples } from './rename-examples'
    import { SEARCH_OPTIONS, searchOptionExamples } from './search-option-help'
    import SearchOptionChips from './SearchOptionChips.svelte'

    interface Props {
        session: MultiRenameOpened
        onApplied: (started: MultiRenameStarted) => void
        onClose: () => void
    }

    const { session, onApplied, onClose }: Props = $props()

    // Alpha badge policy: the status comes from the repo-root feature-status.json.
    const badge = getBadgeStatus('multi-rename')

    // One sheet renames one session; a new session remounts it.
    const tool = createMultiRenameState(session.sessionId)

    let nameMaskInput = $state<HTMLInputElement>()
    /** Holds the Letter case `Select`, so ⌘⌥U can open it through its trigger. */
    let caseField = $state<HTMLElement>()
    let presetsControl = $state<PresetsControlApi>()

    /** The icon track, the same as Search's results (`query-ui/result-column-widths.ts`). */
    const ICON_TRACK_PX = 24
    /** The arrow and status glyph tracks: one small glyph each. */
    const GLYPH_TRACK_PX = 16
    /** Floor on each name track: a narrow sheet still shows a few characters of both. */
    const NAME_MIN_PX = 80

    // Read so a late icon re-renders its rows.
    const iconVersion = $derived($iconCacheVersion)

    /** Each placeholder button's tooltip body, which the tooltip adopts on show. */
    const tipContent = $state<Partial<Record<string, HTMLDivElement>>>({})
    /** Every tooltip's examples, rendered by the engine on made-up files; `null` until they come. */
    let examples = $state.raw<ReadonlyMap<string, string> | null>(null)

    async function loadExamples(): Promise<void> {
        const asked = new Map([...placeholderExamples(PLACEHOLDER_HELP), ...searchOptionExamples(SEARCH_OPTIONS)])
        examples = await renderExamples(asked)
    }

    const caseItems = $derived([
        { value: 'unchanged', label: tString('multiRename.case.unchanged') },
        { value: 'lower', label: tString('multiRename.case.lower') },
        { value: 'upper', label: tString('multiRename.case.upper') },
        { value: 'firstUpper', label: tString('multiRename.case.firstUpper') },
        { value: 'words', label: tString('multiRename.case.words') },
    ])

    const canStart = $derived(tool.counts.ready > 0 && tool.error === null && !tool.pending && !tool.applying)
    const shownError = $derived(tool.error ?? tool.applyError)

    // Every track is fixed or shared: a windowed source can't be measured, so the two names
    // split what the glyphs leave.
    const columns = $derived<ColumnListColumn<PreviewRow>[]>([
        { id: 'icon', label: '', width: { kind: 'fixed', px: ICON_TRACK_PX }, clip: false, cell: iconCell },
        {
            id: 'old-name',
            label: tString('multiRename.oldName'),
            width: { kind: 'share', minPx: NAME_MIN_PX },
            class: 'preview-old-name',
            cell: oldNameCell,
        },
        { id: 'arrow', label: '', width: { kind: 'fixed', px: GLYPH_TRACK_PX }, tone: 'tertiary', cell: arrowCell },
        {
            id: 'new-name',
            label: tString('multiRename.newName'),
            width: { kind: 'share', minPx: NAME_MIN_PX },
            emphasis: true,
            class: 'preview-new-name',
            cell: newNameCell,
        },
        {
            id: 'status',
            label: tString('multiRename.statusColumn'),
            labelHidden: true,
            width: { kind: 'fixed', px: GLYPH_TRACK_PX },
            clip: false,
            class: 'preview-status',
            cell: statusCell,
        },
    ])

    onMount(() => {
        void tool.loadPresets()
        void loadExamples()
        nameMaskInput?.focus()
        // F2 is also File > Rename's accelerator. Where AppKit runs the menu item instead of
        // (or as well as) handing the webview the key, its fire comes here too.
        return claimMenuCommand('file.rename', () => {
            presetsControl?.pressOpenKey('menu')
        })
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
        if (started) onApplied(started)
    }

    function handleKeydown(e: KeyboardEvent): void {
        // F2 and ⌘S work from anywhere in the sheet, a text field included.
        const presetKey = presetKeyOf(e)
        if (presetKey !== null) {
            claimKey(e)
            if (presetKey === 'openMenu') presetsControl?.pressOpenKey('keyboard')
            else presetsControl?.openSave()
            return
        }
        // So do the option keys: ⌘⌥ plus a letter flips a checkbox or opens Letter case.
        const optionKey = optionKeyOf(e)
        if (optionKey !== null) {
            claimKey(e)
            if (optionKey.kind === 'toggle') toggle(optionKey.field)
            else openCaseMenu()
            return
        }
        // Enter in a text field starts, as TC's Start! does; a button or menu keeps its own Enter.
        if (e.key !== 'Enter' || e.isComposing || e.metaKey || e.ctrlKey || e.altKey || e.shiftKey) return
        if (!(e.target instanceof HTMLInputElement) || e.target.type === 'checkbox') return
        e.preventDefault()
        void start()
    }

    function words(message: StatusMessage): string {
        return tString(message.key, message.params)
    }

    function iconUrl(iconId: string | null): string | undefined {
        // eslint-disable-next-line @typescript-eslint/no-unused-expressions -- reactive read: a late icon re-renders the row.
        iconVersion
        return iconId === null ? undefined : getCachedIcon(iconId)
    }

    function errorText(error: MultiRenameError): string {
        switch (error.type) {
            case 'spec':
                if (error.error.type === 'badRegex') return tString('multiRename.error.badRegex')
                return error.error.error.type === 'unclosed'
                    ? tString('multiRename.error.unclosed')
                    : tString('multiRename.error.unknown', { placeholder: error.error.error.placeholder })
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
            case 'couldntStart':
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
    <!-- The title bar's `<h2>` is already the row (gap + badge alignment live there),
         so the words and the badge are its direct children. -->
    {#snippet title()}
        <span>{tString('multiRename.title')}</span>{#if badge}<StatusBadge status={badge} />{/if}
    {/snippet}

    <div class="sheet">
        <div class="controls">
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
                    </label>
                    <label class="field extension">
                        <span class="label">{tString('multiRename.extensionMask')}</span>
                        <MaskInput
                            value={tool.spec.extensionMask}
                            onValueChange={(extensionMask: string) => { tool.update({ extensionMask }) }}
                            ariaLabel={tString('multiRename.extensionMask')}
                            invalid={tool.error?.type === 'spec' && tool.error.error.type === 'extensionMask'}
                        />
                    </label>
                </div>
                <div class="placeholders" role="group" aria-label={tString('multiRename.insertPlaceholder')}>
                    {#each PLACEHOLDER_HELP as help (help.placeholder)}
                        <Button
                            size="mini"
                            tooltipContent={{ contentEl: tipContent[help.placeholder] }}
                            onclick={() => { insertPlaceholder(help.placeholder) }}
                        >
                            {help.placeholder}
                        </Button>
                        <PlaceholderTip {help} rendered={examples} bind:contentEl={tipContent[help.placeholder]} />
                    {/each}
                </div>
            </div>

            <!-- What changes the whole name, beside the masks: option and key, the keys on one
                 right edge, each a quiet ⌘⌥ chip. -->
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
                <span class="option-row">
                    <!-- One grid cell: `Checkbox` renders more than one element. -->
                    <span class="option-control">
                        <Checkbox
                            checked={tool.spec.removeDiacritics}
                            onCheckedChange={(on: boolean) => { tool.update({ removeDiacritics: on }) }}
                        >
                            {tString('multiRename.removeDiacritics')}
                        </Checkbox>
                    </span>
                    <!-- The key, quiet: a hint for next time, never a control (it can't be rebound). -->
                    <span class="option-key" aria-hidden="true">
                        <ShortcutChip commandId={TOGGLE_COMMANDS.removeDiacritics} clickable={false} size="sm" />
                    </span>
                </span>
            </div>

            <!-- The full width under both: the search, the replacement, and the search's own
                 options as chips level with the fields. -->
            <div class="search">
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
                <SearchOptionChips spec={tool.spec} onToggle={toggle} rendered={examples} />
            </div>
        </div>

        <!-- Always there, one line tall, so a message coming or going never moves the preview. -->
        <p
            class="error"
            role="alert"
            use:tooltip={shownError ? { text: errorText(shownError), overflowOnly: true } : undefined}
        >
            {shownError ? errorText(shownError) : ''}
        </p>

        <div class="preview">
            <ColumnList
                {...columnListProps({
                    columns,
                    rows: tool.source,
                    semantics: 'table',
                    ariaLabel: tString('multiRename.preview'),
                    headerClass: 'preview-header',
                    rowClass: 'preview-row',
                })}
            />
        </div>
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

{#snippet iconCell({ row }: ColumnListCellContext<PreviewRow>)}
    <span class="icon-box">
        {#if iconUrl(row.iconId)}
            <img class="icon-img" src={iconUrl(row.iconId)} alt="" width="16" height="16" />
        {:else}
            <Icon name={row.isDirectory ? 'folder' : 'file'} size={16} aria-hidden="true" />
        {/if}
    </span>
{/snippet}

<!-- Mid-truncating names, as in Search's results: the extension stays in view, and the full
     name is on hover when it was cut. -->
{#snippet oldNameCell({ row }: ColumnListCellContext<PreviewRow>)}
    <span
        class="name-text"
        use:useShortenMiddle={{ text: row.oldName, preferBreakAt: '.', startRatio: 0.7, tooltipWhenTruncated: true }}
    ></span>
{/snippet}

{#snippet arrowCell()}
    <span class="arrow" aria-hidden="true"><Icon name="arrow-right" size={12} /></span>
{/snippet}

{#snippet newNameCell({ row }: ColumnListCellContext<PreviewRow>)}
    <span
        class="name-text"
        class:unchanged={row.status.type === 'unchanged'}
        use:useShortenMiddle={{ text: row.newName, preferBreakAt: '.', startRatio: 0.7, tooltipWhenTruncated: true }}
    ></span>
{/snippet}

<!-- A problem shows its glyph, named by its label and explained by its tooltip; a ready or
     unchanged row stays quiet and says what it is to screen readers alone. -->
{#snippet statusCell({ row }: ColumnListCellContext<PreviewRow>)}
    {@const view = rowStatusView(row.status)}
    {#if view.glyph}
        <span class="problem-glyph">
            <StatusGlyph
                name={view.glyph}
                label={words(view.label)}
                tooltip={view.reason ? words(view.reason) : undefined}
            />
        </span>
    {:else}
        <span class="sr-only">{words(view.label)}</span>
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

    /* The masks and placeholders on the left, the whole-name options beside them, and the
       search row across both. */
    .controls {
        display: grid;
        grid-template-columns: minmax(0, 1fr) auto;
        gap: var(--spacing-md) var(--spacing-lg);
    }

    .search {
        grid-column: 1 / -1;
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

    .placeholders {
        display: flex;
        gap: var(--spacing-xs);
        flex-wrap: wrap;
    }

    /* Two columns, option and key, so the keys line up on one right edge; the two rows share
       the masks' and placeholders' height, so the column sits level with them. */
    .options {
        display: grid;
        grid-template-columns: auto auto;
        align-content: space-evenly;
        gap: var(--spacing-sm) var(--spacing-lg);
        padding-left: var(--spacing-lg);
        border-left: 1px solid var(--color-border);
    }

    .option-control {
        display: flex;
    }

    /* One row: the option and its key share the column tracks and center on each other. */
    .option-row {
        grid-column: 1 / -1;
        display: grid;
        grid-template-columns: subgrid;
        align-items: center;
    }

    .case-field {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
    }

    /* The key hint stays quiet: tertiary text on no fill, so it doesn't shout. */
    .option-key {
        display: flex;
        justify-content: flex-end;
    }

    .option-key :global(.shortcut-chip) {
        color: var(--color-text-tertiary);
        background: transparent;
    }

    /* One reserved line under the search row: a long message ends in an ellipsis (the whole of it
       on hover) rather than growing the line. */
    .error {
        margin: 0;
        min-height: calc(var(--font-size-sm) * var(--font-line-height-normal));
        overflow: hidden;
        white-space: nowrap;
        text-overflow: ellipsis;
        color: var(--color-error-text);
        font-size: var(--font-size-sm);
        line-height: var(--font-line-height-normal);
    }

    /* The well around the list: one element owns the border and the rounded corners, and
       hands the list the sheet's spare height. */
    .preview {
        display: flex;
        flex-direction: column;
        flex: 1 1 auto;
        min-height: 0;
        overflow: hidden;
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
    }

    /* Cell contents. The cells themselves (font, tone, the track widths) are `ColumnList`'s. */
    .icon-box {
        display: flex;
        align-items: center;
        justify-content: center;
        width: 16px;
        color: var(--color-text-secondary);
    }

    .icon-img {
        width: 16px;
        height: 16px;
        object-fit: contain;
    }

    /* A block, so `useShortenMiddle` reads the track's width rather than its own text's. */
    .name-text {
        display: block;
        overflow: hidden;
        white-space: nowrap;
    }

    .name-text.unchanged {
        color: var(--color-text-quiet);
        font-weight: normal;
    }

    .arrow {
        display: flex;
        align-items: center;
    }

    .problem-glyph {
        display: flex;
        align-items: center;
        color: var(--color-error-text);
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
