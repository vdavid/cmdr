<script lang="ts">
    /**
     * The range token's editor (`[N2-5]`, `[E1]`, `[-3-]`), top to bottom: the token's field on the
     * list cursor's file in a read-only text box to select characters in (TC's "select the
     * characters to insert"), how to count (each end from the start or the end, and the form), and
     * what the token takes from the files around the cursor. Every edit goes to `onChange`, which
     * rewrites the token in the mask. Which characters a range takes is always the engine's answer
     * (`rangeSampleSource`), never sliced here; this only turns a pick into a token.
     *
     * The box and the token follow each other: a selection the user makes becomes the token
     * (`textPick`), and a token that changes any other way (typed in the mask, Start / End / Write
     * as, a new cursor row) sets the box's selection to the engine's bounds. A bare caret leaves the
     * token. A pointer selection is a fresh pick, each end counting from the edge it's nearer; a key
     * only adjusts it, keeping the token's own edges, so a manual Start / End choice sticks until
     * the next pointer pick. Only a selection that follows the user's own pointer or key in the box
     * counts: WebKit selects a field's whole text when Tab lands in it, and that mustn't rewrite the
     * token to `[N]`.
     *
     * Keys: ↑ / ↓ walk the rows (↑ from the box, and Enter anywhere, leave through `onDone`). In the
     * single-line box plain ↑ / ↓ would only collapse the selection to an end, which leaves the token
     * anyway, so nothing is lost; ⇧↑ / ⇧↓ stay the field's own. ↑ / ↓ / Enter are claimed in the
     * capture phase, so the toggle groups and the sheet never see them. Escape belongs to the
     * `Popover` around this.
     */
    import { tString } from '#lib/intl/messages.svelte.js'
    import TextInput from '#lib/ui/TextInput.svelte'
    import ToggleGroup, { type ToggleGroupOption } from '#lib/ui/ToggleGroup.svelte'
    import { claimKey } from '#lib/shortcuts/claim-key.js'
    import type { RangeSample } from '#lib/tauri-commands/index.js'
    import type { TokenEditorProps } from './mask-token-kinds'
    import {
        anchorsOf,
        autoAnchors,
        formatRangeToken,
        rangeForSelection,
        type Anchors,
        type Range,
        type RangeForm,
        type RangeToken,
        type Selection,
    } from './range-token'
    import { rangeReading, textOffsetsOf, textPick } from './range-editor'
    import { rangeSampleSource } from './range-sample-source'

    const { value, onChange, onDone }: TokenEditorProps<RangeToken> = $props()

    const source = rangeSampleSource()

    const tokenText = $derived(formatRangeToken(value))
    const anchors = $derived(anchorsOf(value.range))

    /** The engine's latest answer: for which token, which row is the cursor's, and the samples. */
    let answer = $state.raw<{ token: string; cursor: number; samples: RangeSample[] } | null>(null)
    /**
     * The pick the last edit wrote, standing in until the engine answers for its token, so the box
     * keeps the user's selection rather than jumping back to the answer before.
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
    const text = $derived(cursorSample?.text ?? '')
    const length = $derived(Array.from(text).length)
    const selection = $derived.by((): Selection | null => {
        if (picked?.token === tokenText) return picked.selection
        const taken = cursorSample?.taken
        return taken ? { first: taken.first, last: taken.last, length } : null
    })
    /** The end counts from the end with a start that does: that's how the grammar reads `[N-8-5]`. */
    const endFromEnd = $derived(anchors.startFromEnd || anchors.endFromEnd)

    const uid = $props.id()
    const boxId = `range-editor-box-${uid}`
    const readingId = `range-editor-reading-${uid}`
    const samplesId = `range-editor-samples-${uid}`

    let rootEl: HTMLDivElement | undefined = $state()
    let boxEl: HTMLInputElement | undefined = $state()

    function commit(next: Selection, range: Range): void {
        const token: RangeToken = { field: value.field, range }
        picked = { token: formatRangeToken(token), selection: next }
        onChange(token)
    }

    function write(next: Selection, nextAnchors: Anchors, form: RangeForm): void {
        commit(next, rangeForSelection({ selection: next, anchors: nextAnchors, form }))
    }

    // ===== The box =====

    /**
     * What last moved the box's selection since focus last left it: the user's pointer (a fresh
     * pick), a key (an adjustment, keeping the token's own edges), or neither (Tab's select-all,
     * or this editor setting it), which writes nothing.
     */
    let selectingBy: 'pointer' | 'key' | null = null

    /** The box's selection set to the token's characters on this file, or a caret when it takes none. */
    function showSelection(): void {
        const box = boxEl
        if (!box) return
        const { start, end } = selection ? textOffsetsOf(text, selection) : { start: 0, end: 0 }
        if (box.selectionStart === start && box.selectionEnd === end) return
        box.setSelectionRange(start, end)
    }

    // Follows the token, the cursor's file, and the box's text (rendered before effects run).
    $effect(showSelection)

    function handleSelectionChange(): void {
        const box = boxEl
        if (!box || selectingBy === null || box.selectionStart === null || box.selectionEnd === null) return
        const keep = selectingBy === 'key' ? { startFromEnd: anchors.startFromEnd, endFromEnd } : undefined
        const pick = textPick({ text, start: box.selectionStart, end: box.selectionEnd, form: anchors.form, keep })
        // A caret leaves the token; the same characters again (the token's own, set back) change nothing.
        if (!pick || (selection && pick.selection.first === selection.first && pick.selection.last === selection.last)) return
        commit(pick.selection, pick.range)
    }

    function handleBoxPointerDown(): void {
        selectingBy = 'pointer'
    }

    function handleBoxKeydown(): void {
        selectingBy = 'key'
    }

    function handleBoxFocus(): void {
        // A pointer in the box selects on its own; Tab's select-all comes after this, so the token's
        // characters go back on top of it.
        if (selectingBy === null) setTimeout(showSelection, 0)
    }

    function handleBoxBlur(): void {
        selectingBy = null
    }

    $effect(() => {
        const box = boxEl
        if (!box) return
        box.addEventListener('pointerdown', handleBoxPointerDown)
        box.addEventListener('keydown', handleBoxKeydown)
        box.addEventListener('focus', handleBoxFocus)
        box.addEventListener('blur', handleBoxBlur)
        box.addEventListener('select', handleSelectionChange)
        // WebKit tells a field's selection moving through the document's `selectionchange`.
        document.addEventListener('selectionchange', handleSelectionChange)
        return () => {
            box.removeEventListener('pointerdown', handleBoxPointerDown)
            box.removeEventListener('keydown', handleBoxKeydown)
            box.removeEventListener('focus', handleBoxFocus)
            box.removeEventListener('blur', handleBoxBlur)
            box.removeEventListener('select', handleSelectionChange)
            document.removeEventListener('selectionchange', handleSelectionChange)
        }
    })

    // ===== How to count =====

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

    /** The element ↑ / ↓ puts focus on in `row`: the box, or the row's picked option. */
    function focusTargetIn(row: Element): HTMLElement | null {
        return (
            row.querySelector<HTMLElement>('input') ??
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
        // Capture: ↑ / ↓ / Enter are the editor's before the box, a toggle group, or the sheet sees them.
        root.addEventListener('keydown', handleKeydown, true)
        return () => {
            root.removeEventListener('keydown', handleKeydown, true)
        }
    })

    function pieces(sample: RangeSample): { before: string; taken: string; after: string } {
        const chars = Array.from(sample.text)
        if (!sample.taken) return { before: sample.text, taken: '', after: '' }
        const { first, last } = sample.taken
        return {
            before: chars.slice(0, first).join(''),
            taken: chars.slice(first, last + 1).join(''),
            after: chars.slice(last + 1).join(''),
        }
    }
</script>

<div class="range-editor" bind:this={rootEl}>
    <div class="pick" data-walk>
        <label class="label" for={boxId}>{tString('multiRename.rangeEditor.selectCharacters')}</label>
        <TextInput
            id={boxId}
            readonly
            mono
            value={text}
            bind:inputElement={boxEl}
            placeholder={answer && length === 0 ? tString('multiRename.rangeEditor.emptyField') : undefined}
            aria-describedby={readingId}
            autocomplete="off"
            spellcheck="false"
        />
    </div>

    <!-- Live, so a token changed from the toggles or the mask is read out in plain words. -->
    <p class="reading" id={readingId} aria-live="polite">
        <code class="token">[{tokenText}]</code>
        {reading}
        {#if answer && length > 0 && !selection}
            <span class="warning">{tString('multiRename.rangeEditor.takesNothing')}</span>
        {/if}
    </p>

    <!-- One grid, so the three groups share the widest one's natural width. -->
    <div class="counting">
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
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
        width: 420px;
        max-width: 100%;
    }

    .pick {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
    }

    .label {
        margin: 0;
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        font-weight: normal;
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

    /* The groups' column is as wide as the widest group's labels, and every group fills it, so the
       three line up as one column whatever a translation does to a label. */
    .counting {
        display: grid;
        grid-template-columns: max-content max-content;
        justify-content: start;
        align-items: center;
        gap: var(--spacing-xs) var(--spacing-sm);
    }

    .counting .row {
        display: contents;
    }

    .counting :global(.tg-root) {
        display: flex;
        width: 100%;
    }

    /* Each segment grows from its label's width, so a long label never clips. */
    .counting :global(.tg-root .tg-item) {
        flex: 1 1 auto;
        justify-content: center;
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
