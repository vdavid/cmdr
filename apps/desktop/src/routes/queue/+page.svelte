<script lang="ts">
    import { onMount, onDestroy, tick } from 'svelte'
    import { SvelteSet } from 'svelte/reactivity'
    import { closeSelfWindow } from '#lib/child-window-close.js'
    import { showSelfWindow } from '#lib/child-window-show.js'
    import type { UnlistenFn } from '@tauri-apps/api/event'
    import { initWindowSettings, initWindowLanguageSync } from '#lib/settings/window-settings.js'
    import { initAccentColor, cleanupAccentColor } from '#lib/accent-color.js'
    import { initGlassMaterial, cleanupGlassMaterial } from '#lib/glass-material.js'
    import { initDisplayAccessibility, cleanupDisplayAccessibility } from '#lib/display-accessibility.js'
    import { initTextSize, cleanupTextSize } from '#lib/text-size.svelte.js'
    import { trackOwnRect } from '#lib/window-positioning.js'
    import { getAppLogger } from '#lib/logging/logger.js'
    import {
        cancelOperations,
        dismissAllFailedOperations,
        dismissFailedOperation,
        moveQueuedOperation,
        onQueueRowContextAction,
        onQueueSelectAll,
        pauseAll,
        placeQueuedOperation,
        resumeAll,
        showQueueRowContextMenu,
        type QueueMoveDirection,
        type QueueMoveOutcome,
        type QueueRowMenuAction,
    } from '#lib/tauri-commands/index.js'
    import { boundShortcuts } from '#lib/shortcuts/index.js'
    import { tString } from '#lib/intl/messages.svelte.js'
    import Button from '#lib/ui/Button.svelte'
    import Icon from '#lib/ui/Icon.svelte'
    import QueueRow from '#lib/file-operations/queue/QueueRow.svelte'
    import { createQueueCursor } from '#lib/file-operations/queue/queue-cursor.svelte.js'
    import {
        queueKeyAction,
        type QueueKeyAction,
        type QueueRowAction,
        type QueueRowApi,
    } from '#lib/file-operations/queue/queue-keys.js'
    import {
        createOperationsStore,
        isHiddenSettledStatus,
        isTerminalStatus,
    } from '#lib/file-operations/queue/operations-store.svelte.js'
    import { treesCollapsible } from '#lib/file-operations/queue/row-tree.js'
    import { createQueueDrag } from '#lib/file-operations/queue/queue-drag.svelte.js'
    import { measureRowTops, slideRowsFrom } from '#lib/file-operations/queue/queue-flip.js'
    import {
        queuedNeighbors,
        queueRowMenuEntries,
        queueRowMenuKeyAction,
        queueRowMenuOffers,
        type QueueRowMenuFacts,
        type QueueRowMenuSource,
    } from '#lib/file-operations/queue/queue-row-menu.js'
    import {
        initOperationSessions,
        destroyOperationSessions,
    } from '#lib/file-operations/operation-session/window-operation-sessions.svelte.js'

    const log = getAppLogger('queue')

    const store = createOperationsStore()

    let initialized = $state(false)
    let unlistenRectTracking: (() => void) | undefined
    let unsubscribeLanguage: (() => void) | undefined
    let unlistenSelectAll: UnlistenFn | undefined
    let unlistenRowMenu: UnlistenFn | undefined

    /** Ids the user has checked for "Cancel selected". A `SvelteSet` so toggling
     *  is O(1) AND reactive on in-place mutation (add/delete), per the project's
     *  selection pattern. */
    const selectedIds = new SvelteSet<string>()

    /** Live work, plus every retained failure. A `done` or `cancelled` op can
     *  linger in the snapshot for a tick before the backend prunes it, and it
     *  has nothing left to say, so it stays hidden; a failure stays visible
     *  until someone dismisses it. */
    const rows = $derived(store.operations.filter((row) => !isHiddenSettledStatus(row.snapshot.status)))
    const isEmpty = $derived(rows.length === 0)
    /** Two or more rows show their live tree, so each gets a collapse toggle. */
    const treeCollapsible = $derived(treesCollapsible(rows))

    // Selection that no longer points at something cancelable gets dropped so the
    // count and "Cancel selected" stay honest. Membership alone isn't the test: a
    // failure STAYS in `rows` and drops its checkbox, so a tick the user made
    // while it was running would have no way back out, and the toolbar would
    // offer to cancel an operation that has already stopped.
    $effect(() => {
        const cancelableIds = new Set(
            rows.filter((r) => !isTerminalStatus(r.snapshot.status)).map((r) => r.snapshot.operationId),
        )
        for (const id of selectedIds) {
            if (!cancelableIds.has(id)) selectedIds.delete(id)
        }
    })

    const selectedCount = $derived(selectedIds.size)

    // ── Keyboard (DETAILS § Keyboard) ───────────────────────────────────────
    // The list is an ARIA grid with ONE Tab stop: the cursor row. The arrows move
    // the cursor, ← / → reach the focused row's buttons, and the row's own keys
    // (Enter, ⌫) do what its buttons do. The cursor follows an operation id, so
    // a reorder or a finished operation never slides another one under it.

    const rowIds = $derived(rows.map((r) => r.snapshot.operationId))

    const cursor = createQueueCursor({
        ids: () => rowIds,
        selection: selectedIds,
        isSelectable: (id) => {
            const row = rows.find((r) => r.snapshot.operationId === id)
            return row !== undefined && !isTerminalStatus(row.snapshot.status)
        },
    })

    /**
     * Each row's keyboard half and menu facts (`QueueRow`'s exports), by operation id. Raw state:
     * read imperatively, never rendered, and a proxy would wrap the component exports.
     */
    const rowApis: Record<string, (QueueRowApi & QueueRowMenuSource) | undefined> = $state.raw({})

    let gridEl: HTMLDivElement | undefined = $state()

    /** The grid held focus last, so a row that leaves (it finished, or a reorder
     *  moved its element) hands focus to the cursor's new row instead of dropping
     *  it on the page. Cleared only when focus genuinely moves elsewhere. */
    let gridOwnsFocus = $state(false)

    /** The live region's text: where a reordered operation landed. */
    let announcement = $state('')

    // The cursor follows its operation, or takes the row that slid into its place.
    // `reconcile` reads the ids, which is what re-runs this on every change.
    $effect(() => {
        cursor.reconcile()
    })

    // Keep focus on the cursor row while the grid owns it: a row that vanished, or
    // a keyed `{#each}` moving the focused element, leaves focus on <body>.
    // Reading `rowIds` here is what re-runs it after a reorder, when the cursor id stays put.
    $effect(() => {
        const id = cursor.cursorId
        if (!gridOwnsFocus || id === null || !gridEl || !rowIds.includes(id)) return
        const active = document.activeElement
        if (active === null || active === document.body || !gridEl.contains(active)) rowApis[id]?.focusRow()
    })

    /** The row Tab lands on: the cursor row, or the first row before there is one. */
    const tabStopId = $derived(
        cursor.cursorId !== null && rowIds.includes(cursor.cursorId) ? cursor.cursorId : (rowIds[0] ?? null),
    )

    function handleGridFocusOut(event: FocusEvent): void {
        const next = event.relatedTarget
        // `null` is focus going nowhere: a removed element, or the window losing
        // focus. Either way the grid still owns it when it comes back.
        if (next instanceof Node && !gridEl?.contains(next)) gridOwnsFocus = false
    }

    function focusCursorRow(): void {
        const id = cursor.cursorId
        if (id !== null) rowApis[id]?.focusRow()
    }

    async function moveCursorRow(direction: QueueMoveDirection): Promise<void> {
        const id = cursor.cursorId
        if (id === null) return
        try {
            await announceMove(await moveQueuedOperation(id, direction), direction)
        } catch (error) {
            log.warn('Failed to move operation {operationId}: {error}', { operationId: id, error })
        }
    }

    /** Says where a moved operation landed. A drop where it started says nothing. */
    async function announceMove(outcome: QueueMoveOutcome, direction: QueueMoveDirection): Promise<void> {
        if (outcome.kind === 'unchanged') return
        // Announced fresh each time: a live region repeats nothing it already says.
        announcement = ''
        await tick()
        switch (outcome.kind) {
            case 'moved':
                announcement = tString('queue.reorder.moved', {
                    direction,
                    position: outcome.position,
                    waiting: outcome.waiting,
                })
                return
            case 'at_edge':
                announcement = tString('queue.reorder.atEdge', { direction })
                return
            case 'not_queued':
                announcement = tString('queue.reorder.notQueued')
        }
    }

    // ── Dragging to reorder (DETAILS § "Dragging to reorder") ───────────────
    // The mouse half of ⌥↑ / ⌥↓: a waiting row's grip drags it among the waiting
    // rows, and the drop puts it at a place in line in one backend call.

    const waitingIds = $derived(rows.filter((r) => r.snapshot.status === 'queued').map((r) => r.snapshot.operationId))
    /** A lone waiting row has nobody to trade with, so it gets no grip, and with no
     *  grips the column goes too. */
    const reorderable = $derived(waitingIds.length >= 2)

    function rowElement(id: string): HTMLElement | undefined {
        return gridEl?.querySelector<HTMLElement>(`[data-operation-id="${CSS.escape(id)}"]`) ?? undefined
    }

    const drag = createQueueDrag({
        waitingIds: () => waitingIds,
        rowElement,
        scroller: () => gridEl,
        onLift: (id) => {
            if (cursor.cursorId !== id) cursor.place(id)
            rowApis[id]?.focusRow()
        },
        onDrop: async (id, position) => {
            const from = waitingIds.indexOf(id) + 1
            try {
                const outcome = await placeQueuedOperation(id, position)
                await announceMove(outcome, position < from ? 'earlier' : 'later')
                return outcome.kind === 'moved'
            } catch (error) {
                log.warn('Failed to place operation {operationId}: {error}', { operationId: id, error })
                return false
            }
        },
    })

    /** The gap the dragged row would land in, drawn on the waiting row that borders it. */
    function dropCueFor(id: string): 'above' | 'below' | null {
        const slot = drag.dropSlot
        if (slot === null) return null
        const index = waitingIds.indexOf(id)
        if (index === slot) return 'above'
        if (slot === waitingIds.length && index === waitingIds.length - 1) return 'below'
        return null
    }

    // FLIP: when the line's order changes (a drop, ⌥↑ / ⌥↓, a row that finished),
    // measure every row where it's drawn BEFORE the DOM moves, then slide each
    // from there into its new slot. A dropped row is measured where it was dropped.
    let flipOrder = ''
    $effect.pre(() => {
        const order = rowIds.join('\n')
        if (order === flipOrder) return
        const firstRender = flipOrder === ''
        flipOrder = order
        if (firstRender || !gridEl) return
        const list = gridEl
        const before = measureRowTops(list)
        drag.listChanging()
        void tick().then(() => {
            slideRowsFrom(list, before)
        })
    })

    /** Runs a key the grid answers. `false` leaves the key to whoever else wants it. */
    function runGridAction(action: QueueKeyAction): boolean {
        switch (action.kind) {
            case 'cursor':
                cursor.move(action.to)
                focusCursorRow()
                return true
            case 'extend':
                cursor.extend(action.to)
                focusCursorRow()
                return true
            case 'toggleSelected':
                cursor.toggle()
                return true
            case 'selectAll':
                cursor.selectAll()
                return true
            case 'move':
                void moveCursorRow(action.direction)
                return true
            case 'show':
            case 'pauseOrResume':
            case 'cancelOrDismiss':
            case 'cancel':
            case 'rollback':
            case 'control':
                return runRowAction(action)
            case 'close':
                return false
        }
    }

    /** The keys that act on the cursor row's own operation, through the row. */
    function runRowAction(action: QueueRowAction): boolean {
        const id = cursor.cursorId
        const api = id === null ? undefined : rowApis[id]
        if (!api) return false
        switch (action.kind) {
            case 'show':
                return api.show()
            case 'pauseOrResume':
                return api.pauseOrResume()
            case 'cancelOrDismiss':
                return api.cancelOrDismiss()
            case 'cancel':
                return api.cancel()
            case 'rollback':
                return api.rollback()
            case 'control':
                return api.focusControl(action.step)
        }
    }

    // ── The row's right-click menu (DETAILS § "The row's right-click menu") ──
    // A native menu whose every pick runs the keyboard action of the same item, on
    // the row it opened over, so the menu and the keys can't drift.

    /** The row the open (or last) menu belongs to. The pick arrives as an event
     *  after the menu closed, so this is how it finds its row again. */
    let menuRowId: string | null = null

    /** Everything the menu asks about row `id`, or `null` once the row is gone. The
     *  row answers for itself; the selection and the line's order are the page's. */
    function rowMenuFacts(id: string): QueueRowMenuFacts | null {
        const api = rowApis[id]
        const row = rows.find((r) => r.snapshot.operationId === id)
        if (!api || !row) return null
        const line = rows.map((r) => ({ id: r.snapshot.operationId, status: r.snapshot.status }))
        return {
            ...api.menuFacts(),
            selectable: !isTerminalStatus(row.snapshot.status),
            selected: selectedIds.has(id),
            ...queuedNeighbors(line, id),
        }
    }

    /** A right-click puts the cursor on the row, as in any list, then opens its menu. */
    async function openRowMenu(id: string): Promise<void> {
        if (cursor.cursorId !== id) cursor.place(id)
        rowApis[id]?.focusRow()
        const facts = rowMenuFacts(id)
        if (facts === null) return
        menuRowId = id
        try {
            await showQueueRowContextMenu(queueRowMenuEntries(facts), boundShortcuts())
        } catch (error) {
            log.warn('Failed to open the menu of operation {operationId}: {error}', {
                operationId: id,
                error,
            })
        }
    }

    /** Runs a menu pick, if the row still offers it live: it may have finished, or
     *  been paused from the toolbar, while the menu was up. */
    function runRowMenuPick(action: QueueRowMenuAction): void {
        const id = menuRowId
        menuRowId = null
        if (id === null) return
        const facts = rowMenuFacts(id)
        if (facts === null || !queueRowMenuOffers(facts, action)) return
        if (cursor.cursorId !== id) cursor.place(id)
        runGridAction(queueRowMenuKeyAction(action))
    }

    /** Whether the grid answers a key pressed on `target`. Inside the grid, yes;
     *  on the bare page too (nothing focused yet, or focus dropped after a click),
     *  so ↓ works the moment the window opens. ❌ Never inside a dialog (the
     *  rollback question has its own keys), on the toolbar, or inside a row's
     *  live tree: that's its own widget, where ⌫ cancelling the whole copy would
     *  be a trap. Escape there hands focus back to the row (`QueueRow`). */
    function gridHears(target: EventTarget | null): boolean {
        if (!(target instanceof Element)) return true
        if (target.closest('[role="dialog"], [role="alertdialog"], [data-queue-tree]')) return false
        if (gridEl?.contains(target)) return true
        return target === document.body || target.classList.contains('queue-window')
    }

    /** Enter and Space press a focused BUTTON natively; ⌫ and the letters belong
     *  to the row wherever focus is in it. */
    function isRowOnlyKey(action: QueueKeyAction, event: KeyboardEvent): boolean {
        if (!(event.target instanceof HTMLButtonElement)) return false
        return action.kind === 'toggleSelected' || (action.kind === 'show' && event.key === 'Enter')
    }

    function toggleSelect(operationId: string): void {
        if (selectedIds.has(operationId)) selectedIds.delete(operationId)
        else selectedIds.add(operationId)
    }

    // ❌ No per-row pause/resume/cancel/rollback here. A row commands its own
    // operation through the session it's already bound to, so the same press
    // means the same thing from every surface and no two of them can send it
    // twice. This page keeps only what ISN'T a command on one operation: the
    // fleet actions below and dismissing a retained failure.

    async function cancelSelected(): Promise<void> {
        const ids = [...selectedIds]
        if (ids.length === 0) return
        try {
            await cancelOperations(ids)
            selectedIds.clear()
        } catch (error) {
            log.warn('Failed to cancel selected operations: {error}', { error })
        }
    }

    /** Stop showing one retained failure. Explicit only: nothing else in the app
     *  drops a failed row, so a copy that died while the user was away is still
     *  here when they come back. */
    async function dismissRow(operationId: string): Promise<void> {
        try {
            await dismissFailedOperation(operationId)
        } catch (error) {
            log.warn('Failed to dismiss operation {operationId}: {error}', { operationId, error })
        }
    }

    async function dismissAll(): Promise<void> {
        try {
            await dismissAllFailedOperations()
        } catch (error) {
            log.warn('Failed to dismiss all failed operations: {error}', { error })
        }
    }

    async function handlePauseAll(): Promise<void> {
        try {
            await pauseAll()
        } catch (error) {
            log.warn('Failed to pause all operations: {error}', { error })
        }
    }

    async function handleResumeAll(): Promise<void> {
        try {
            await resumeAll()
        } catch (error) {
            log.warn('Failed to resume all operations: {error}', { error })
        }
    }

    function closeWindow(): void {
        void closeSelfWindow()
    }

    function handleKeydown(event: KeyboardEvent): void {
        if (event.defaultPrevented) return
        const action = queueKeyAction(event)
        if (action === null) return
        if (action.kind === 'close') {
            event.preventDefault()
            // Mid-drag, Escape puts the row back instead, as in any list.
            if (!drag.cancel()) closeWindow()
            return
        }
        if (!gridHears(event.target) || isRowOnlyKey(action, event)) return
        if (runGridAction(action)) event.preventDefault()
    }

    onMount(async () => {
        const loadingScreen = document.getElementById('loading-screen')
        if (loadingScreen) loadingScreen.style.display = 'none'

        try {
            // Seeds the store AND the reactive layer that `<Size>` and friends read.
            // `window-settings.ts` knows this window has no store capability (see
            // `src-tauri/capabilities/CLAUDE.md` § queue — no persistence in v1), so it
            // takes the restricted path: the backend snapshot plus cross-window change
            // events, mirroring the viewer, and non-throwing so a perm failure can't
            // leave the body unrendered.
            await initWindowSettings()
            unsubscribeLanguage = initWindowLanguageSync({ title: () => tString('queue.windowTitle') })
            await initAccentColor()
            await initGlassMaterial()
            await initDisplayAccessibility()
            await initTextSize()
            await store.init()
            // This window's session registry. Subscribed here, before any row
            // can ask for a session: the fan-out's listeners are async, and
            // events that arrive while they're being set up would be lost.
            await initOperationSessions()
            initialized = true

            // The opener creates this window hidden; it looks right now, so show it
            // (`#lib/child-window-show`). A no-op on an E2E window, which is created visible.
            await tick()
            showSelfWindow()

            // Remember position/size within the session so a reopen lands in place.
            unlistenRectTracking = await trackOwnRect('queue')

            // ⌘A reaches this window as a menu event, never as a keydown (the
            // accelerator fires first). The keydown road above stays for a menu
            // that doesn't claim it (the dev browser).
            unlistenSelectAll = await onQueueSelectAll(() => {
                cursor.selectAll()
            })
            unlistenRowMenu = await onQueueRowContextAction(runRowMenuPick)
        } catch (error) {
            log.error('Failed to initialize operation-queue window: {error}', { error })
        }
    })

    onDestroy(() => {
        drag.cancel()
        unlistenRectTracking?.()
        unlistenSelectAll?.()
        unlistenRowMenu?.()
        unsubscribeLanguage?.()
        store.dispose()
        destroyOperationSessions()
        cleanupAccentColor()
        cleanupGlassMaterial()
        cleanupDisplayAccessibility()
        cleanupTextSize()
    })
</script>

<svelte:window onkeydown={handleKeydown} />

<main class="queue-window" class:is-reordering={drag.active} tabindex="-1">
    <h1 class="sr-only">{tString('queue.windowTitle')}</h1>
    <!-- The title sits on the traffic lights' line, centered like a native window
         title (the main window's title bar does the same), and the whole strip
         drags the window. The text carries the attribute too: Tauri reads it off
         the element under the cursor. -->
    <div class="title-bar" data-tauri-drag-region>
        <span class="title" data-tauri-drag-region aria-hidden="true">{tString('queue.heading')}</span>
    </div>

    <header class="queue-header">
        <div class="toolbar" role="toolbar" aria-label={tString('queue.heading')}>
            <Button variant="secondary" size="mini" onclick={handlePauseAll} disabled={!store.hasRunning}>
                <span class="btn-inner"><Icon name="pause" size={13} />{tString('queue.toolbar.pauseAll')}</span>
            </Button>
            <Button variant="secondary" size="mini" onclick={handleResumeAll} disabled={!store.hasPaused}>
                <span class="btn-inner"><Icon name="play" size={13} />{tString('queue.toolbar.resumeAll')}</span>
            </Button>
            <!-- One failure has its own Dismiss on the row; the toolbar button
                 earns its place only once there's a pile to clear. -->
            {#if store.failureCount > 1}
                <Button variant="secondary" size="mini" onclick={() => void dismissAll()}>
                    <span class="btn-inner"><Icon name="x" size={13} />{tString('queue.toolbar.dismissAll')}</span>
                </Button>
            {/if}
            <span class="toolbar-spacer"></span>
            {#if selectedCount > 0}
                <span class="selected-count">{tString('queue.toolbar.selectedCount', { count: selectedCount })}</span>
            {/if}
            <Button variant="secondary" size="mini" onclick={cancelSelected} disabled={selectedCount === 0}>
                <span class="btn-inner"><Icon name="x" size={13} />{tString('queue.toolbar.cancelSelected')}</span>
            </Button>
        </div>
    </header>

    {#if initialized}
        {#if isEmpty}
            <div class="empty-state">
                <span class="empty-icon" aria-hidden="true"><Icon name="hourglass" size={28} /></span>
                <p class="empty-title">{tString('queue.empty.title')}</p>
                <p class="empty-body">{tString('queue.empty.body')}</p>
            </div>
        {:else}
            <div
                bind:this={gridEl}
                class="queue-list queue-grid"
                role="grid"
                aria-label={tString('queue.list.aria')}
                aria-multiselectable="true"
                onfocusin={() => {
                    gridOwnsFocus = true
                }}
                onfocusout={handleGridFocusOut}
            >
                {#each rows as row (row.snapshot.operationId)}
                    {@const id = row.snapshot.operationId}
                    <QueueRow
                        bind:this={rowApis[id]}
                        {row}
                        selected={selectedIds.has(id)}
                        isCursor={cursor.cursorId === id}
                        tabbable={tabStopId === id}
                        {treeCollapsible}
                        onFocusRow={() => {
                            if (cursor.cursorId !== id) cursor.place(id)
                        }}
                        onToggleSelect={() => {
                            toggleSelect(id)
                        }}
                        onContextMenu={() => void openRowMenu(id)}
                        onDismiss={() => void dismissRow(id)}
                        gripColumn={reorderable}
                        draggable={reorderable && row.snapshot.status === 'queued'}
                        onGripPointerDown={(event: PointerEvent) => {
                            drag.start(id, event)
                        }}
                        dragOffset={drag.draggingId === id ? drag.offsetY : null}
                        lifted={drag.active && drag.draggingId === id}
                        dropCue={dropCueFor(id)}
                    />
                {/each}
            </div>
        {/if}
    {/if}
    <!-- Where a reordered operation landed. The list re-renders silently, so
         this is the only way a screen reader hears that ⌥↑ / ⌥↓ did anything. -->
    <div class="sr-only" role="status" aria-live="polite">{announcement}</div>
</main>

<style>
    .queue-window {
        width: 100%;
        height: 100vh;
        background: var(--color-bg-glass);
        color: var(--color-text-primary);
        font-family: var(--font-system) sans-serif;
        font-size: var(--font-size-sm);
        overflow: hidden;
        display: flex;
        flex-direction: column;
        position: relative;
    }

    /* While a row is held, the closed hand wins over every hover cursor it passes, and
       nothing it crosses starts a text selection. */
    .queue-window.is-reordering,
    .queue-window.is-reordering :global(*) {
        cursor: grabbing;
        user-select: none;
        -webkit-user-select: none;
    }

    /* The overlay title bar's own line, shared with the traffic lights. */
    .title-bar {
        height: var(--titlebar-height);
        display: flex;
        align-items: center;
        justify-content: center;
        padding-top: var(--spacing-xxs);
        flex-shrink: 0;
    }

    .title {
        /* Capped like the main window's title, so a large text size can't
           outgrow the fixed-height bar. */
        font-size: min(var(--font-size-sm), 18px);
        font-weight: 500;
        color: var(--color-text-secondary);
        user-select: none;
    }

    .queue-header {
        padding: 0 var(--spacing-md) var(--spacing-sm);
        border-bottom: 1px solid var(--color-border-glass);
    }

    .toolbar {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
    }

    .toolbar-spacer {
        flex: 1;
    }

    .selected-count {
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
        white-space: nowrap;
    }

    .btn-inner {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
    }

    .queue-list {
        flex: 1;
        overflow-y: auto;
        padding: var(--spacing-sm);
        margin: 0;
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
        scrollbar-gutter: stable;
    }

    .empty-state {
        flex: 1;
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        gap: var(--spacing-xs);
        padding: var(--spacing-xl);
        text-align: center;
    }

    .empty-icon {
        color: var(--color-text-tertiary);
        margin-bottom: var(--spacing-xs);
    }

    .empty-title {
        font-size: var(--font-size-md);
        font-weight: 600;
        color: var(--color-text-primary);
        margin: 0;
    }

    .empty-body {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        max-width: 320px;
        margin: 0;
    }
</style>
