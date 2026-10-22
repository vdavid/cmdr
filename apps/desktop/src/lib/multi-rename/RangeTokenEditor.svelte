<script lang="ts">
    /**
     * The range token's editor (`[N2-5]`, `[E1]`, `[-3-]`), top to bottom: the token's field on the
     * list cursor's file in a text box to select characters in (TC's "select the characters to
     * insert"), how to count (each end from the start or the end, and the form), and what the token
     * takes from the files around the cursor. Every edit goes to `onChange`, which rewrites the
     * token in the mask. Which characters a range takes is always the engine's answer
     * (`rangeSampleSource`), never sliced here; this only turns a pick into a token.
     *
     * The box is a real text field with a caret and every native caret and selection key, but it
     * refuses edits (typing, paste, cut, drop, IME). It isn't `readonly`: WebKit draws no caret in
     * a read-only field.
     *
     * The box drives the token: a selection becomes the token (`textPick`), and a bare caret is the
     * whole field (`[N]`). A token that changes any other way (typed in the mask, a new cursor row,
     * the editor opening) sets the box's selection to the engine's bounds, or all the text for the
     * whole field; the box's own changes are never set back (`boxNeedsSync`), or every
     * caret move would turn back into a selection. A pointer selection is a fresh pick, each end
     * counting from the edge it's nearer; a key only adjusts it, keeping the token's own edges, so
     * a manual Start / End choice sticks until the next pointer pick. Only a selection that follows
     * the user's own pointer or key in the box counts: WebKit selects a field's whole text when Tab
     * lands in it, and that mustn't rewrite the token.
     *
     * Keys: Enter leaves through `onDone`, claimed in the capture phase so the sheet never starts a
     * rename from here. In the box, a bare ↑ leaves the same way and a bare ↓ moves on to the Start
     * toggles. Escape belongs to the `Popover` around this. Every other key is native.
     */
    import { tString } from '#lib/intl/messages.svelte.js'
    import TextInput from '#lib/ui/TextInput.svelte'
    import ToggleGroup, { type ToggleGroupOption } from '#lib/ui/ToggleGroup.svelte'
    import type { ToggleGroupAPI } from '#lib/ui/toggle-group-types.js'
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
    import {
        boxNeedsSync,
        boxOffsetsFor,
        isWholeField,
        rangeReading,
        selectionPieces,
        textPick,
        type BoxWrite,
    } from './range-editor'
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
    let picked = $state.raw<{ token: string; selection: Selection | null } | null>(null)
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
    /** The whole field has no part: the box selects all its text, and there are no edges to count. */
    const whole = $derived(isWholeField(value.range))
    const selection = $derived.by((): Selection | null => {
        if (whole) return null
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
    let startGroup: ToggleGroupAPI | undefined = $state()

    /**
     * The token the box (or a toggle, which keeps the box's selection) last wrote, and on which
     * text. While the token and the text still match it, the box's selection is the user's own and
     * stays as it is. Not reactive: only the token and the text changing re-run the sync.
     */
    let wrote: BoxWrite | null = null

    function commit(next: Selection | null, range: Range): void {
        const token: RangeToken = { field: value.field, range }
        const formatted = formatRangeToken(token)
        wrote = { token: formatted, text }
        picked = { token: formatted, selection: next }
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
    /** An IME composing in the box: its interim text is refused at the end, and its selection writes nothing. */
    let composing = false
    /** The box's selection right before an edit WebKit wouldn't let us cancel, to put back with the text. */
    let beforeEdit: { start: number; end: number } | null = null

    /** What the box selects for the token: its characters on this file, or all the text for the whole field. */
    const shownPart = $derived(whole ? 'whole' : selection)

    /** The box's selection set to the token's part (`shownPart`), or a caret at the end when it takes nothing. */
    function showSelection(): void {
        const box = boxEl
        if (!box) return
        selectingBy = null
        const { start, end } = boxOffsetsFor(text, shownPart)
        if (box.selectionStart !== start || box.selectionEnd !== end) box.setSelectionRange(start, end)
        noteBoxSelection()
    }

    // ===== The selection drawn while the box is unfocused =====
    // WebKit paints no selection in an unfocused field, yet that selection is the token: after Tab
    // moves on, and in a caret-opened editor whose box never takes focus, the mirror draws it.

    let boxFocused = $state(false)
    /** The box's selection and horizontal scroll, as last read, for the mirror. */
    let boxSelection = $state.raw({ start: 0, end: 0 })
    let boxScroll = $state(0)
    const mirror = $derived(selectionPieces(text, boxSelection.start, boxSelection.end))

    function noteBoxSelection(): void {
        const box = boxEl
        if (!box) return
        boxSelection = { start: box.selectionStart ?? 0, end: box.selectionEnd ?? 0 }
        boxScroll = box.scrollLeft
    }

    function handleBoxScroll(): void {
        boxScroll = boxEl?.scrollLeft ?? 0
    }

    // Follows a token from outside the box (the mask, a new cursor row, the engine's first answer),
    // never the box's own (rendered before effects run).
    $effect(() => {
        if (!boxNeedsSync({ token: tokenText, text, wrote })) return
        wrote = null
        showSelection()
    })

    function handleSelectionChange(): void {
        noteBoxSelection()
        const box = boxEl
        if (!box || selectingBy === null || composing || box.selectionStart === null || box.selectionEnd === null) return
        // A key adjusts a part's edges; from the whole field (no part yet) it's a fresh pick.
        const keep = selectingBy === 'key' && !whole ? { startFromEnd: anchors.startFromEnd, endFromEnd } : undefined
        const pick = textPick({ text, start: box.selectionStart, end: box.selectionEnd, form: anchors.form, keep })
        if (formatRangeToken({ field: value.field, range: pick.range }) === tokenText) {
            // The same token (a caret moving inside the whole field): the box already shows it.
            wrote = { token: tokenText, text }
            return
        }
        commit(pick.selection, pick.range)
    }

    function handleBoxPointerDown(): void {
        selectingBy = 'pointer'
    }

    /**
     * Bare ↑ / ↓ aren't caret keys here: ↑ leaves for the mask, ↓ goes on to the Start toggles (a
     * no-op for the whole field, whose toggles are off). With a modifier they stay native (⇧↑
     * extends the selection to the start, ⌘↓ moves to the end), like every other key.
     */
    function handleBoxKeydown(e: KeyboardEvent): void {
        const bare = !e.isComposing && !e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey
        if (bare && e.key === 'ArrowUp') {
            claimKey(e)
            onDone()
            return
        }
        if (bare && e.key === 'ArrowDown') {
            claimKey(e)
            startGroup?.focus()
            return
        }
        selectingBy = 'key'
    }

    function handleBoxFocus(): void {
        boxFocused = true
        // A pointer in the box selects on its own; Tab's select-all comes after this, so the token's
        // characters go back on top of it.
        if (selectingBy === null) setTimeout(showSelection, 0)
    }

    function handleBoxBlur(): void {
        selectingBy = null
        boxFocused = false
        noteBoxSelection()
    }

    // ===== The box refuses edits =====

    function refuse(e: Event): void {
        e.preventDefault()
    }

    function handleBeforeInput(e: InputEvent): void {
        const box = boxEl
        if (box && box.selectionStart !== null && box.selectionEnd !== null) {
            beforeEdit = { start: box.selectionStart, end: box.selectionEnd }
        }
        e.preventDefault()
    }

    /** Copies what cut would take, without taking it. */
    function handleCut(e: ClipboardEvent): void {
        const box = boxEl
        e.preventDefault()
        if (!box || box.selectionStart === null || box.selectionEnd === null) return
        e.clipboardData?.setData('text/plain', box.value.slice(box.selectionStart, box.selectionEnd))
    }

    /** An edit that got through anyway (an IME's interim text isn't cancelable in WebKit): put the field back. */
    function handleInput(): void {
        const box = boxEl
        if (!box || box.value === text || composing) return
        box.value = text
        const { start, end } = beforeEdit ?? boxOffsetsFor(text, shownPart)
        box.setSelectionRange(start, end)
        beforeEdit = null
        noteBoxSelection()
    }

    function handleCompositionStart(): void {
        composing = true
    }

    function handleCompositionEnd(): void {
        composing = false
        handleInput()
    }

    $effect(() => {
        const box = boxEl
        if (!box) return
        const listeners: [string, (e: never) => void][] = [
            ['pointerdown', handleBoxPointerDown],
            ['keydown', handleBoxKeydown],
            ['focus', handleBoxFocus],
            ['blur', handleBoxBlur],
            ['scroll', handleBoxScroll],
            ['select', handleSelectionChange],
            ['beforeinput', handleBeforeInput],
            ['cut', handleCut],
            ['paste', refuse],
            ['drop', refuse],
            ['input', handleInput],
            ['compositionstart', handleCompositionStart],
            ['compositionend', handleCompositionEnd],
        ]
        for (const [type, listener] of listeners) box.addEventListener(type, listener as EventListener)
        // WebKit tells a field's selection moving through the document's `selectionchange`.
        document.addEventListener('selectionchange', handleSelectionChange)
        return () => {
            for (const [type, listener] of listeners) box.removeEventListener(type, listener as EventListener)
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

    function countingNow() {
        return {
            start: anchors.startFromEnd ? 'end' : 'start',
            startOptions: edgeOptions(selection?.first),
            end: endFromEnd ? 'end' : 'start',
            endOptions: edgeOptions(selection?.last, anchors.startFromEnd),
            form: anchors.form,
            off: !selection,
        }
    }

    /**
     * What the toggle groups show, settled before each render. Ark's teardown reads a group's props
     * one last time, and a derived there would re-run the whole chain back to `value`, mid-change
     * when an error boundary is tearing the editor down: Svelte's `derived_references_self`. A
     * `$state` read returns what the group last showed.
     */
    let counting = $state.raw(countingNow())
    $effect.pre(() => {
        counting = countingNow()
    })
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

    function handleKeydown(e: KeyboardEvent): void {
        if (e.isComposing || e.metaKey || e.ctrlKey || e.altKey || e.shiftKey || e.key !== 'Enter') return
        claimKey(e)
        onDone()
    }

    $effect(() => {
        const root = rootEl
        if (!root) return
        // Capture: Enter is the editor's before a toggle group or the sheet sees it.
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
    <div class="pick">
        <label class="label" for={boxId}>{tString('multiRename.rangeEditor.selectCharacters')}</label>
        <div class="box-wrap">
            <!-- Not `readonly`, which draws no caret in WebKit: the box refuses edits itself. -->
            <TextInput
                id={boxId}
                aria-readonly="true"
                mono
                value={text}
                bind:inputElement={boxEl}
                placeholder={answer && length === 0 ? tString('multiRename.rangeEditor.emptyField') : undefined}
                aria-describedby={readingId}
                autocomplete="off"
                spellcheck="false"
            />
            {#if !boxFocused && mirror.selected}
                <!-- The box's selection, drawn while WebKit draws none (an unfocused field). VoiceOver
                     reads the field's own selection, so this stays out of the tree. -->
                <div class="box-mirror" aria-hidden="true">
                    <div class="box-mirror-clip">
                        <span class="box-mirror-text" style:transform="translateX({-boxScroll}px)"
                            >{mirror.before}<mark>{mirror.selected}</mark>{mirror.after}</span
                        >
                    </div>
                </div>
            {/if}
        </div>
    </div>

    <!-- Live, so a token changed from the toggles or the mask is read out in plain words. -->
    <p class="reading" id={readingId} aria-live="polite">
        <code class="token">[{tokenText}]</code>
        {reading}
        {#if answer && length > 0 && !whole && !selection}
            <span class="warning">{tString('multiRename.rangeEditor.takesNothing')}</span>
        {/if}
    </p>

    <!-- One grid, so the three groups share the widest one's natural width. -->
    <div class="counting">
        <div class="row">
            <span class="label" aria-hidden="true">{tString('multiRename.rangeEditor.start')}</span>
            <ToggleGroup
                bind:this={startGroup}
                semantics="toggles"
                value={counting.start}
                options={counting.startOptions}
                onChange={setStart}
                ariaLabel={tString('multiRename.rangeEditor.start')}
                disabled={counting.off}
            />
        </div>
        {#if counting.form === 'span'}
            <div class="row">
                <span class="label" aria-hidden="true">{tString('multiRename.rangeEditor.end')}</span>
                <ToggleGroup
                    semantics="toggles"
                    value={counting.end}
                    options={counting.endOptions}
                    onChange={setEnd}
                    ariaLabel={tString('multiRename.rangeEditor.end')}
                    disabled={counting.off}
                />
            </div>
        {/if}
        <div class="row">
            <span class="label" aria-hidden="true">{tString('multiRename.rangeEditor.writeAs')}</span>
            <ToggleGroup
                semantics="toggles"
                value={counting.form}
                options={formOptions}
                onChange={setForm}
                ariaLabel={tString('multiRename.rangeEditor.writeAs')}
                disabled={counting.off}
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

    .box-wrap {
        position: relative;
    }

    /* Laid over the house field's content box: the frame's border and padding
       (`app-field.css` § `.text-field`), then the control's own font and leading, so
       each character lands on the box's. Only the marked part paints; the rest stays
       transparent over the box's own text. */
    .box-mirror {
        display: flex;
        position: absolute;
        inset: 0;
        align-items: center;
        padding: var(--spacing-input);
        border: 1px solid transparent;
        pointer-events: none;
    }

    .box-mirror-clip {
        flex: 1;
        min-width: 0;
        overflow: hidden;
    }

    .box-mirror-text {
        display: inline-block;
        color: transparent;
        font-family: var(--font-mono);
        font-size: var(--font-size-input);
        line-height: var(--font-line-height-tight);
        white-space: pre;
    }

    .box-mirror-text mark {
        background: var(--color-text-selection-inactive);
        color: var(--color-text-primary);
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
