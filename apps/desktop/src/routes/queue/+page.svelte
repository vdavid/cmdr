<script lang="ts">
    import { onMount, onDestroy } from 'svelte'
    import { SvelteSet } from 'svelte/reactivity'
    import { getCurrentWindow } from '@tauri-apps/api/window'
    import { listen, type UnlistenFn } from '@tauri-apps/api/event'
    import { getSetting, onSpecificSettingChange } from '$lib/settings'
    import { initWindowSettings } from '$lib/settings/window-settings'
    import { setLocale } from '$lib/intl/messages.svelte'
    import { initAccentColor, cleanupAccentColor } from '$lib/accent-color'
    import { initReduceTransparency, cleanupReduceTransparency } from '$lib/reduce-transparency'
    import { initTextSize, cleanupTextSize } from '$lib/text-size.svelte'
    import { trackOwnRect } from '$lib/window-positioning'
    import { getAppLogger } from '$lib/logging/logger'
    import {
        cancelOperation,
        cancelOperations,
        cancelWriteOperation,
        dismissAllFailedOperations,
        dismissFailedOperation,
        pauseAll,
        pauseOperation,
        resumeAll,
        resumeOperation,
    } from '$lib/tauri-commands'
    import { tString } from '$lib/intl/messages.svelte'
    import Button from '$lib/ui/Button.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import QueueRow from '$lib/file-operations/queue/QueueRow.svelte'
    import { createOperationsStore, isHiddenSettledStatus } from '$lib/file-operations/queue/operations-store.svelte'

    const log = getAppLogger('queue')

    const store = createOperationsStore()

    let initialized = $state(false)
    let unlistenFocusSelf: UnlistenFn | undefined
    let unlistenRectTracking: (() => void) | undefined
    let unsubscribeLanguage: (() => void) | undefined

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

    // Selection that points at rows no longer present gets dropped so the count
    // and "Cancel selected" stay honest as ops finish.
    $effect(() => {
        const liveIds = new Set(rows.map((r) => r.snapshot.operationId))
        for (const id of selectedIds) {
            if (!liveIds.has(id)) selectedIds.delete(id)
        }
    })

    const selectedCount = $derived(selectedIds.size)

    function toggleSelect(operationId: string): void {
        if (selectedIds.has(operationId)) selectedIds.delete(operationId)
        else selectedIds.add(operationId)
    }

    async function pauseResumeRow(operationId: string, paused: boolean): Promise<void> {
        try {
            if (paused) await resumeOperation(operationId)
            else await pauseOperation(operationId)
        } catch (error) {
            log.warn('Failed to pause/resume operation {operationId}: {error}', { operationId, error: String(error) })
        }
    }

    async function cancelRow(operationId: string): Promise<void> {
        try {
            await cancelOperation(operationId)
        } catch (error) {
            log.warn('Failed to cancel operation {operationId}: {error}', { operationId, error: String(error) })
        }
    }

    /**
     * Cancel AND undo what the operation already wrote. This is the write-op
     * intent switch (`cancelWriteOperation(id, rollback = true)`), NOT the
     * manager-level `cancelOperation`: only the former can ask a running op to
     * delete its partial destination. The row offers it solely where the
     * backend's snapshot says the op is reversible.
     */
    async function rollbackRow(operationId: string): Promise<void> {
        try {
            await cancelWriteOperation(operationId, true)
        } catch (error) {
            log.warn('Failed to roll back operation {operationId}: {error}', { operationId, error: String(error) })
        }
    }

    async function cancelSelected(): Promise<void> {
        const ids = [...selectedIds]
        if (ids.length === 0) return
        try {
            await cancelOperations(ids)
            selectedIds.clear()
        } catch (error) {
            log.warn('Failed to cancel selected operations: {error}', { error: String(error) })
        }
    }

    /** Stop showing one retained failure. Explicit only: nothing else in the app
     *  drops a failed row, so a copy that died while the user was away is still
     *  here when they come back. */
    async function dismissRow(operationId: string): Promise<void> {
        try {
            await dismissFailedOperation(operationId)
        } catch (error) {
            log.warn('Failed to dismiss operation {operationId}: {error}', { operationId, error: String(error) })
        }
    }

    async function dismissAll(): Promise<void> {
        try {
            await dismissAllFailedOperations()
        } catch (error) {
            log.warn('Failed to dismiss all failed operations: {error}', { error: String(error) })
        }
    }

    async function handlePauseAll(): Promise<void> {
        try {
            await pauseAll()
        } catch (error) {
            log.warn('Failed to pause all operations: {error}', { error: String(error) })
        }
    }

    async function handleResumeAll(): Promise<void> {
        try {
            await resumeAll()
        } catch (error) {
            log.warn('Failed to resume all operations: {error}', { error: String(error) })
        }
    }

    /**
     * Keeps THIS window's UI language in sync. The queue window is its own webview
     * with its own i18n runtime, so the main window's applier doesn't reach it:
     * apply the persisted language at open and on any change. `'system'` maps to
     * the OS locale (`setLocale(null)`). Mirrors the Settings window.
     */
    function initLanguageSync(): void {
        const applyLanguage = (value: string) => {
            setLocale(value === 'system' ? null : value)
        }
        applyLanguage(getSetting('appearance.language'))
        unsubscribeLanguage = onSpecificSettingChange('appearance.language', (_id, value) => {
            applyLanguage(value)
        })
    }

    function handleKeydown(event: KeyboardEvent): void {
        if (event.key === 'Escape') {
            event.preventDefault()
            // Defer the close past the current event-loop tick so any in-flight IPC
            // ack settles before the webview is destroyed. `setTimeout(0)`, not rAF
            // (throttled when unfocused on macOS). Mirrors Settings / Shortcuts.
            const win = getCurrentWindow()
            setTimeout(() => {
                void win.close()
            }, 0)
        }
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
            initLanguageSync()
            await initAccentColor()
            await initReduceTransparency()
            await initTextSize()
            await store.init()
            initialized = true

            // Already-open window self-focuses on a re-open (cross-window setFocus()
            // doesn't reliably raise a window on macOS).
            unlistenFocusSelf = await listen('focus-self', () => {
                setTimeout(() => {
                    void getCurrentWindow().setFocus()
                }, 0)
            })

            // Remember position/size within the session so a reopen lands in place.
            unlistenRectTracking = await trackOwnRect('queue')
        } catch (error) {
            log.error('Failed to initialize operation-queue window: {error}', { error })
        }
    })

    onDestroy(() => {
        unlistenFocusSelf?.()
        unlistenRectTracking?.()
        unsubscribeLanguage?.()
        store.dispose()
        cleanupAccentColor()
        cleanupReduceTransparency()
        cleanupTextSize()
    })
</script>

<svelte:window on:keydown={handleKeydown} />

<main class="queue-window" tabindex="-1">
    <h1 class="sr-only">{tString('queue.windowTitle')}</h1>
    <!-- Drag strip under the overlay traffic lights, like Settings/Shortcuts. -->
    <div class="window-drag-region" data-tauri-drag-region aria-hidden="true"></div>

    <header class="queue-header">
        <div class="title-row">
            <span class="title">{tString('queue.heading')}</span>
        </div>
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
            <ul class="queue-list" aria-label={tString('queue.list.aria')}>
                {#each rows as row (row.snapshot.operationId)}
                    <QueueRow
                        {row}
                        selected={selectedIds.has(row.snapshot.operationId)}
                        onToggleSelect={() => { toggleSelect(row.snapshot.operationId); }}
                        onPauseResume={() =>
                            void pauseResumeRow(row.snapshot.operationId, row.snapshot.status === 'paused')}
                        onCancel={() => void cancelRow(row.snapshot.operationId)}
                        onRollback={() => void rollbackRow(row.snapshot.operationId)}
                        onDismiss={() => void dismissRow(row.snapshot.operationId)}
                    />
                {/each}
            </ul>
        {/if}
    {/if}
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

    /* Drag strip over the overlay title-bar row (where the traffic lights sit). */
    .window-drag-region {
        position: absolute;
        top: 0;
        left: 0;
        right: 0;
        height: var(--titlebar-height);
        z-index: var(--z-dropdown);
    }

    .queue-header {
        /* Start below the overlay title-bar so the title and toolbar clear the
           traffic lights and the drag strip above. */
        padding: var(--spacing-sm) var(--spacing-md) var(--spacing-sm);
        padding-top: calc(var(--titlebar-height) + var(--spacing-xs));
        display: flex;
        flex-direction: column;
        gap: var(--spacing-sm);
        border-bottom: 1px solid var(--color-border-glass);
    }

    .title-row {
        display: flex;
        align-items: baseline;
        justify-content: space-between;
        gap: var(--spacing-md);
        /* Leave room for the traffic lights on the left of the drag strip. */
        padding-left: var(--spacing-xl);
    }

    .title {
        font-size: var(--font-size-lg);
        font-weight: 600;
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
        list-style: none;
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
