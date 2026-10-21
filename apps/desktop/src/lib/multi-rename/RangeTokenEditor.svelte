<script lang="ts">
    /**
     * The range token's editor (`[N2-5]`, `[E1]`, `[-3-]`), top to bottom: what to take (the
     * field, and its characters on the list cursor's file as a ribbon to drag across), how to
     * count (each end from the start or the end, and the form), and what the token takes from
     * the files around the cursor. Every edit goes to `onChange`, which rewrites the token in the
     * mask. Which characters a range takes is always the engine's answer (`rangeSampleSource`),
     * never sliced here; this only turns a pick into a token (`range-token.ts`).
     *
     * A drag (or a ⇧-click, which extends it) counts each end from the edge it's nearer
     * (`autoAnchors`); keys and the Start / End rows keep the token's own choice, so a manual
     * choice sticks until the next drag.
     *
     * Keys: ↑ / ↓ walk the rows (↑ from the top row, and Enter anywhere, leave through `onDone`);
     * on the ribbon ← / → move the selection, ⇧ its end, ⌥ its start. ↑ / ↓ / Enter are claimed
     * in the capture phase, so the toggle groups and the sheet never see them. Escape belongs to
     * the `Popover` around this.
     */
    import { tick } from 'svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import type { MessageKey } from '#lib/intl/keys.gen.js'
    import ToggleGroup, { type ToggleGroupOption } from '#lib/ui/ToggleGroup.svelte'
    import { claimKey } from '#lib/shortcuts/claim-key.js'
    import type { RangeSample } from '#lib/tauri-commands/index.js'
    import type { TokenEditorProps } from './mask-token-kinds'
    import {
        RANGE_FIELDS,
        anchorsOf,
        autoAnchors,
        formatRangeToken,
        rangeForSelection,
        type Anchors,
        type RangeField,
        type RangeForm,
        type RangeToken,
        type Selection,
    } from './range-token'
    import { extendAnchor, moveSelection, rangeReading, ribbonMoveOf, spanBetween } from './range-editor'
    import { rangeSampleSource } from './range-sample-source'

    const { value, onChange, onDone }: TokenEditorProps<RangeToken> = $props()

    const source = rangeSampleSource()

    /** The handles' width, so the end one sits inside its cell. Matches `.edge`'s CSS. */
    const EDGE_WIDTH_PX = 3

    const FIELD_LABELS: Record<RangeField, MessageKey> = {
        name: 'multiRename.rangeEditor.field.name',
        extension: 'multiRename.rangeEditor.field.extension',
        parent: 'multiRename.rangeEditor.field.parent',
        grandparent: 'multiRename.rangeEditor.field.grandparent',
        fullName: 'multiRename.rangeEditor.field.fullName',
    }

    const tokenText = $derived(formatRangeToken(value))
    const anchors = $derived(anchorsOf(value.range))

    /** The engine's latest answer: for which token, which row is the cursor's, and the samples. */
    let answer = $state.raw<{ token: string; cursor: number; samples: RangeSample[] } | null>(null)
    /**
     * The pick the last edit wrote, standing in until the engine answers for its token, so a key
     * pressed again right away moves on from it rather than from the answer before.
     */
    let picked = $state.raw<{ token: string; selection: Selection } | null>(null)
    let asked = 0

    $effect(() => {
        // Asks again for a new token, and for every new preview (a new cursor row, or a moved one).
        const rows = source?.sampleRows
        const token = tokenText
        if (!source || !rows) return
        const id = ++asked
        void source.rangeSamples(token, rows.rows).then((samples) => {
            // A newer question overtook this one, or the engine couldn't say: keep what's shown.
            if (id !== asked || samples === null) return
            answer = { token, cursor: rows.cursor, samples }
            if (picked?.token === token) picked = null
        })
    })

    const cursorSample = $derived(answer?.samples.find((sample) => sample.row === answer?.cursor))
    const chars = $derived(cursorSample ? Array.from(cursorSample.text) : [])
    const length = $derived(chars.length)
    const selection = $derived.by((): Selection | null => {
        if (picked?.token === tokenText) return picked.selection
        const taken = cursorSample?.taken
        return taken ? { first: taken.first, last: taken.last, length } : null
    })
    /** The end counts from the end with a start that does: that's how the grammar reads `[N-8-5]`. */
    const endFromEnd = $derived(anchors.startFromEnd || anchors.endFromEnd)

    const uid = $props.id()
    const readingId = `range-editor-reading-${uid}`
    const keysId = `range-editor-keys-${uid}`
    const samplesId = `range-editor-samples-${uid}`

    let rootEl: HTMLDivElement | undefined = $state()
    let ribbonEl: HTMLDivElement | undefined = $state()
    let startEl: HTMLSpanElement | undefined = $state()
    let endEl: HTMLSpanElement | undefined = $state()

    function write(next: Selection, nextAnchors: Anchors, form: RangeForm): void {
        const token: RangeToken = { field: value.field, range: rangeForSelection({ selection: next, anchors: nextAnchors, form }) }
        picked = { token: formatRangeToken(token), selection: next }
        onChange(token)
    }

    /** A fresh pick (a drag or a click): each end counts from the edge it's nearer. */
    function pickFresh(next: Selection): void {
        write(next, autoAnchors(next), anchors.form)
    }

    // ===== The ribbon =====

    let dragAnchor: number | null = null
    let dragAt: number | null = null

    function cellAt(e: PointerEvent): number | null {
        // Under pointer capture the event targets the ribbon itself, so find the cell by position.
        const direct = e.target instanceof Element ? e.target.closest<HTMLElement>('[data-index]') : null
        const cell = direct ?? document.elementFromPoint(e.clientX, e.clientY)?.closest<HTMLElement>('[data-index]')
        const index = cell?.dataset.index
        return index === undefined ? null : Number(index)
    }

    function handlePointerDown(e: PointerEvent): void {
        if (e.button !== 0 || length === 0) return
        const index = cellAt(e)
        if (index === null) return
        e.preventDefault()
        dragAnchor = e.shiftKey && selection ? extendAnchor(selection, index) : index
        dragAt = index
        pickFresh(spanBetween(dragAnchor, index, length))
        if (ribbonEl && 'setPointerCapture' in ribbonEl) ribbonEl.setPointerCapture(e.pointerId)
        void tick().then(() => startEl?.focus())
    }

    function handlePointerMove(e: PointerEvent): void {
        if (dragAnchor === null) return
        const index = cellAt(e)
        if (index === null || index === dragAt) return
        dragAt = index
        pickFresh(spanBetween(dragAnchor, index, length))
    }

    function endDrag(): void {
        dragAnchor = null
        dragAt = null
    }

    function handleRibbonKey(e: KeyboardEvent): void {
        const move = e.isComposing ? null : ribbonMoveOf(e)
        if (move === null) return
        claimKey(e)
        if (length === 0) return
        if (!selection) {
            pickFresh(spanBetween(0, 0, length))
            return
        }
        const next = moveSelection(selection, move)
        if (next.first === selection.first && next.last === selection.last) return
        // Keys keep the token's own choice of edges.
        write(next, { startFromEnd: anchors.startFromEnd, endFromEnd }, anchors.form)
        // The handle that moved takes focus, so a screen reader reads its new place.
        if (move.edge === 'end') endEl?.focus()
        else if (move.edge === 'start') startEl?.focus()
    }

    // The ribbon is a labelled group, so its pointer and key handlers go on as listeners; the
    // handles carry the keys too, as sliders.
    $effect(() => {
        const ribbon = ribbonEl
        if (!ribbon) return
        ribbon.addEventListener('pointerdown', handlePointerDown)
        ribbon.addEventListener('pointermove', handlePointerMove)
        ribbon.addEventListener('pointerup', endDrag)
        ribbon.addEventListener('pointercancel', endDrag)
        ribbon.addEventListener('keydown', handleRibbonKey)
        return () => {
            ribbon.removeEventListener('pointerdown', handlePointerDown)
            ribbon.removeEventListener('pointermove', handlePointerMove)
            ribbon.removeEventListener('pointerup', endDrag)
            ribbon.removeEventListener('pointercancel', endDrag)
            ribbon.removeEventListener('keydown', handleRibbonKey)
        }
    })

    // The first answer lands after the popover focused the empty ribbon: hand focus to the start handle.
    $effect(() => {
        if (selection && startEl && document.activeElement === ribbonEl) startEl.focus()
    })

    function charName(index: number): string {
        const char = chars[index] ?? ''
        return char === ' ' ? tString('multiRename.rangeEditor.space') : char
    }

    function edgeText(index: number, fromEnd: boolean): string {
        return fromEnd
            ? tString('multiRename.rangeEditor.edgeFromEnd', { at: length - index, character: charName(index) })
            : tString('multiRename.rangeEditor.edgeFromStart', { at: index + 1, character: charName(index) })
    }

    // ===== How to count =====

    const fieldOptions = $derived<ToggleGroupOption[]>(
        RANGE_FIELDS.map((field) => ({ value: field, label: tString(FIELD_LABELS[field]) })),
    )

    /**
     * Start / End's two options, each showing the real position of the edge at `index` on the cursor's
     * file. `lockStart` turns "from the start" off: with the start counted from the end, the end can't
     * count from the start, since the grammar has no way to say it.
     */
    function edgeOptions(index: number | undefined, lockStart = false): ToggleGroupOption[] {
        const fromStart =
            index === undefined
                ? tString('multiRename.rangeEditor.fromStart')
                : tString('multiRename.rangeEditor.positionFromStart', { at: index + 1 })
        const fromEnd =
            index === undefined
                ? tString('multiRename.rangeEditor.fromEnd')
                : tString('multiRename.rangeEditor.positionFromEnd', { at: length - index })
        const lock = lockStart ? { disabled: true, tooltip: tString('multiRename.rangeEditor.endFollowsStart') } : {}
        return [
            { value: 'start', label: fromStart, ...lock },
            { value: 'end', label: fromEnd },
        ]
    }

    const startOptions = $derived(edgeOptions(selection?.first))
    const endOptions = $derived(edgeOptions(selection?.last, anchors.startFromEnd))
    const formOptions = $derived<ToggleGroupOption[]>([
        { value: 'span', label: tString('multiRename.rangeEditor.form.span') },
        { value: 'length', label: tString('multiRename.rangeEditor.form.length') },
    ])

    function setField(field: string): void {
        picked = null
        onChange({ field: field as RangeField, range: value.range })
    }

    function setStart(edge: string): void {
        if (!selection) return
        const startFromEnd = edge === 'end'
        write(selection, { startFromEnd, endFromEnd: startFromEnd || endFromEnd }, anchors.form)
    }

    function setEnd(edge: string): void {
        if (!selection) return
        write(selection, { startFromEnd: anchors.startFromEnd, endFromEnd: edge === 'end' }, anchors.form)
    }

    function setForm(form: string): void {
        if (!selection) return
        // Back from a length, which says nothing about the end, the end counts from its nearer edge.
        const fromLength = anchors.form === 'length' && form === 'span'
        const nextEnd = fromLength ? anchors.startFromEnd || autoAnchors(selection).endFromEnd : endFromEnd
        write(selection, { startFromEnd: anchors.startFromEnd, endFromEnd: nextEnd }, form as RangeForm)
    }

    const reading = $derived.by(() => {
        const { key, params } = rangeReading(value.range)
        return tString(key, params)
    })

    // ===== Keys =====

    /** The element ↑ / ↓ puts focus on in `row`: its handle, its focusable ribbon, or its picked option. */
    function focusTargetIn(row: Element): HTMLElement | null {
        return (
            row.querySelector<HTMLElement>('[role="slider"]') ??
            row.querySelector<HTMLElement>('[tabindex="0"]') ??
            row.querySelector<HTMLElement>('[data-state="on"]:not([disabled])') ??
            row.querySelector<HTMLElement>('button:not([disabled])')
        )
    }

    /** ↑ / ↓ from the row holding the event's target: the next row's control, or out through `onDone` from the top row. */
    function walk(e: KeyboardEvent, root: HTMLElement, step: 1 | -1): void {
        const rows = [...root.querySelectorAll('[data-walk]')].filter((row) => focusTargetIn(row) !== null)
        const at = rows.findIndex((row) => e.target instanceof Node && row.contains(e.target))
        if (at === -1) return
        claimKey(e)
        if (step === -1 && at === 0) {
            onDone()
            return
        }
        const next = rows.at(at + step)
        if (next) focusTargetIn(next)?.focus()
    }

    function handleKeydown(e: KeyboardEvent): void {
        if (e.isComposing || e.metaKey || e.ctrlKey || e.altKey || e.shiftKey || !rootEl) return
        if (e.key === 'Enter') {
            claimKey(e)
            onDone()
        } else if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
            walk(e, rootEl, e.key === 'ArrowUp' ? -1 : 1)
        }
    }

    $effect(() => {
        const root = rootEl
        if (!root) return
        // Capture: ↑ / ↓ / Enter are the editor's before a toggle group or the sheet sees them.
        root.addEventListener('keydown', handleKeydown, true)
        return () => {
            root.removeEventListener('keydown', handleKeydown, true)
        }
    })

    function pieces(sample: RangeSample): { before: string; taken: string; after: string } {
        const text = Array.from(sample.text)
        if (!sample.taken) return { before: sample.text, taken: '', after: '' }
        const { first, last } = sample.taken
        return {
            before: text.slice(0, first).join(''),
            taken: text.slice(first, last + 1).join(''),
            after: text.slice(last + 1).join(''),
        }
    }
</script>

<div class="range-editor" bind:this={rootEl}>
    <div class="row" data-walk>
        <span class="label" aria-hidden="true">{tString('multiRename.rangeEditor.takeFrom')}</span>
        <ToggleGroup
            semantics="toggles"
            value={value.field}
            options={fieldOptions}
            onChange={setField}
            ariaLabel={tString('multiRename.rangeEditor.takeFrom')}
        />
    </div>

    <div class="ribbon-row" data-walk>
        {#if answer && length === 0}
            <p class="note">{tString('multiRename.rangeEditor.emptyField')}</p>
        {:else}
            <!-- The handles carry the keys while there's a selection; before one, the ribbon does. -->
            <div
                class="ribbon"
                bind:this={ribbonEl}
                role="group"
                aria-label={tString('multiRename.rangeEditor.characters')}
                aria-describedby="{readingId} {keysId}"
                tabindex={selection ? -1 : 0}
                data-initial-focus={selection ? undefined : ''}
            >
                <div class="numbers" aria-hidden="true">
                    {#each chars as _, i (i)}
                        <span
                            class="number"
                            class:on={selection !== null &&
                                ((i === selection.first && !anchors.startFromEnd) ||
                                    (i === selection.last && anchors.form === 'span' && !endFromEnd))}
                            >{i + 1}</span
                        >
                    {/each}
                </div>
                <div class="cells">
                    {#each chars as char, i (i)}
                        <span
                            class="cell"
                            class:space={char === ' '}
                            class:taken={selection !== null && i >= selection.first && i <= selection.last}
                            data-index={i}
                            aria-hidden="true">{char === ' ' ? '·' : char}</span
                        >
                    {/each}
                    {#if selection}
                        <span
                            bind:this={startEl}
                            class="edge"
                            style:left="calc({selection.first} * var(--spacing-range-cell))"
                            role="slider"
                            tabindex="0"
                            data-initial-focus
                            aria-label={tString('multiRename.rangeEditor.start')}
                            aria-describedby="{readingId} {keysId}"
                            aria-valuemin={1}
                            aria-valuemax={length}
                            aria-valuenow={selection.first + 1}
                            aria-valuetext={edgeText(selection.first, anchors.startFromEnd)}
                        ></span>
                        <span
                            bind:this={endEl}
                            class="edge"
                            style:left="calc({selection.last + 1} * var(--spacing-range-cell) - {EDGE_WIDTH_PX}px)"
                            role="slider"
                            tabindex="0"
                            aria-label={tString('multiRename.rangeEditor.end')}
                            aria-describedby="{readingId} {keysId}"
                            aria-valuemin={1}
                            aria-valuemax={length}
                            aria-valuenow={selection.last + 1}
                            aria-valuetext={edgeText(selection.last, anchors.form === 'span' && endFromEnd)}
                        ></span>
                    {/if}
                </div>
                <div class="numbers" aria-hidden="true">
                    {#each chars as _, i (i)}
                        <span
                            class="number"
                            class:on={selection !== null &&
                                ((i === selection.first && anchors.startFromEnd) ||
                                    (i === selection.last && anchors.form === 'span' && endFromEnd))}
                            >{i - length}</span
                        >
                    {/each}
                </div>
            </div>
        {/if}
        <p class="keys" id={keysId}>{tString('multiRename.rangeEditor.keys')}</p>
    </div>

    <p class="reading" id={readingId}>
        <code class="token">[{tokenText}]</code>
        {reading}
        {#if answer && length > 0 && !selection}
            <span class="warning">{tString('multiRename.rangeEditor.takesNothing')}</span>
        {/if}
    </p>

    <div class="row" data-walk>
        <span class="label" aria-hidden="true">{tString('multiRename.rangeEditor.start')}</span>
        <ToggleGroup
            semantics="toggles"
            value={anchors.startFromEnd ? 'end' : 'start'}
            options={startOptions}
            onChange={setStart}
            ariaLabel={tString('multiRename.rangeEditor.start')}
            disabled={!selection}
        />
    </div>
    {#if anchors.form === 'span'}
        <div class="row" data-walk>
            <span class="label" aria-hidden="true">{tString('multiRename.rangeEditor.end')}</span>
            <ToggleGroup
                semantics="toggles"
                value={endFromEnd ? 'end' : 'start'}
                options={endOptions}
                onChange={setEnd}
                ariaLabel={tString('multiRename.rangeEditor.end')}
                disabled={!selection}
            />
        </div>
    {/if}
    <div class="row" data-walk>
        <span class="label" aria-hidden="true">{tString('multiRename.rangeEditor.writeAs')}</span>
        <ToggleGroup
            semantics="toggles"
            value={anchors.form}
            options={formOptions}
            onChange={setForm}
            ariaLabel={tString('multiRename.rangeEditor.writeAs')}
            disabled={!selection}
        />
    </div>

    {#if answer && answer.samples.length > 0}
        <section class="samples" aria-labelledby={samplesId}>
            <h3 class="label" id={samplesId}>{tString('multiRename.rangeEditor.onYourFiles')}</h3>
            <ul>
                {#each answer.samples as sample (sample.row)}
                    {@const part = pieces(sample)}
                    <li class="sample" class:current={sample.row === answer.cursor} aria-current={sample.row === answer.cursor ? 'true' : undefined}>
                        <span class="source">{part.before}<mark>{part.taken}</mark>{part.after}</span>
                        <span class="arrow" aria-hidden="true">→</span>
                        <span class="sr-only">{tString('multiRename.rangeEditor.gives')}</span>
                        {#if sample.taken}
                            <span class="result">{part.taken}</span>
                        {:else}
                            <span class="result nothing">{tString('multiRename.rangeEditor.nothing')}</span>
                        {/if}
                    </li>
                {/each}
            </ul>
        </section>
    {/if}
</div>

<style>
    .range-editor {
        --spacing-range-cell: 22px;

        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
        width: 520px;
        max-width: 100%;
    }

    .row {
        display: grid;
        grid-template-columns: 72px minmax(0, 1fr);
        align-items: center;
        gap: var(--spacing-sm);
    }

    .label {
        margin: 0;
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        font-weight: normal;
    }

    .ribbon-row {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
        min-width: 0;
    }

    .ribbon {
        display: flex;
        flex-direction: column;
        align-self: flex-start;
        max-width: 100%;
        min-height: var(--spacing-xl);
        padding: var(--spacing-xxs);
        overflow-x: auto;
        border-radius: var(--radius-sm);
        user-select: none;
        touch-action: none;
    }

    .ribbon:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: -2px;
    }

    .numbers,
    .cells {
        display: flex;
    }

    .cells {
        position: relative;
    }

    .number {
        flex: none;
        width: var(--spacing-range-cell);
        color: var(--color-text-tertiary);
        font-family: var(--font-mono);
        font-size: var(--font-size-xs);
        text-align: center;
        font-variant-numeric: tabular-nums;
    }

    .number.on {
        color: var(--color-text-primary);
        font-weight: 600;
    }

    .cell {
        flex: none;
        display: grid;
        place-items: center;
        width: var(--spacing-range-cell);
        height: 28px;
        border-block: 1px solid var(--color-border);
        background: var(--color-bg-primary);
        color: var(--color-text-primary);
        font-family: var(--font-mono);
        font-size: var(--font-size-md);
        cursor: text;
    }

    .cell:first-child {
        border-left: 1px solid var(--color-border);
        border-radius: var(--radius-sm) 0 0 var(--radius-sm);
    }

    .cell:nth-last-child(1 of .cell) {
        border-right: 1px solid var(--color-border);
        border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
    }

    .cell.space {
        color: var(--color-text-tertiary);
    }

    .cell.taken {
        background: var(--color-accent-selection);
    }

    /* The two handles: a bar on each edge of the selection, in the mark color that reads on any
       tinted surface. */
    .edge {
        position: absolute;
        top: 0;
        bottom: 0;
        width: 3px;
        border-radius: var(--radius-xs);
        background: var(--color-accent-pop);
        cursor: ew-resize;
    }

    .edge:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 1px;
    }

    .keys,
    .note {
        margin: 0;
        color: var(--color-text-tertiary);
        font-size: var(--font-size-xs);
    }

    .reading {
        margin: 0;
        font-size: var(--font-size-sm);
    }

    .token {
        margin-right: var(--spacing-xs);
        color: var(--color-text-secondary);
        font-family: var(--font-mono);
    }

    .warning {
        display: block;
        color: var(--color-warning-text);
    }

    .samples {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
    }

    .samples ul {
        margin: 0;
        padding: 0;
        list-style: none;
    }

    .sample {
        display: grid;
        grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
        align-items: baseline;
        gap: var(--spacing-sm);
        padding: var(--spacing-xxs) var(--spacing-xs);
        border-radius: var(--radius-sm);
        font-family: var(--font-mono);
        font-size: var(--font-size-sm);
    }

    .sample.current {
        background: var(--color-bg-tertiary);
    }

    .source {
        overflow-wrap: anywhere;
        color: var(--color-text-secondary);
    }

    .source mark {
        border-radius: var(--radius-xs);
        background: var(--color-accent-selection);
        color: var(--color-text-primary);
    }

    .arrow {
        color: var(--color-text-tertiary);
    }

    .result {
        overflow-wrap: anywhere;
    }

    .result.nothing {
        color: var(--color-warning-text);
        font-style: italic;
    }
</style>
