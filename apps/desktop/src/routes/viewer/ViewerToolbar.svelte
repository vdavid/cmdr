<!--
    Title-bar overlay toolbar for the viewer. Matches the main window's overlay
    style (see `tauri.conf.json` § titleBarStyle and `open-viewer.ts` mirror).
    Reserves 72 px on the left for the macOS traffic lights and lets the empty
    space remain draggable via `data-tauri-drag-region`. The pickers opt out of
    the drag region with their own click handlers so the user can interact with
    them. Indexing progress sits next to the pickers instead of stealing space
    in the status bar.

    Presentational only: every IPC call and page-state mutation lives in
    `+page.svelte`. This component renders state and reports interactions back
    through callback props.
-->

<script lang="ts">
    import { tooltip } from '$lib/tooltip/tooltip'
    import EncodingPicker from './EncodingPicker.svelte'
    import ViewModePicker from './ViewModePicker.svelte'
    import type { EncodingChoice, FileEncoding } from '$lib/ipc/bindings'

    interface Props {
        /** File name shown in the flexible middle of the bar. */
        fileName: string
        /** Current view mode (today only "text"). */
        viewMode: 'text'
        /** Currently active encoding. */
        currentEncoding: FileEncoding
        /** Encoding auto-detection picked at open time. Gets a "(Detected)" suffix. */
        detectedEncoding: FileEncoding
        /** Backend-authoritative dropdown options. */
        encodingChoices: EncodingChoice[]
        /** Whether the file is currently (re)indexing; disables the encoding picker and shows the indicator. */
        isIndexing: boolean
        /** Whether tail mode is on. */
        tailMode: boolean
        /** Called when the user picks a different view mode. */
        onViewModeChange: (mode: 'text') => void
        /** Called when the user picks a different encoding. */
        onEncodingChange: (encoding: FileEncoding) => void
        /** Called when the user toggles tail mode. */
        onToggleTail: () => void
    }

    const {
        fileName,
        viewMode,
        currentEncoding,
        detectedEncoding,
        encodingChoices,
        isIndexing,
        tailMode,
        onViewModeChange,
        onEncodingChange,
        onToggleTail,
    }: Props = $props()
</script>

<header class="viewer-toolbar" data-tauri-drag-region>
    <span class="viewer-toolbar-title" data-tauri-drag-region>{fileName}</span>
    <div class="viewer-toolbar-pickers">
        <ViewModePicker value={viewMode} onChange={onViewModeChange} />
        <EncodingPicker
            value={currentEncoding}
            detected={detectedEncoding}
            options={encodingChoices}
            disabled={isIndexing}
            onChange={onEncodingChange}
        />
        <button
            type="button"
            class="viewer-toolbar-toggle"
            class:active={tailMode}
            role="switch"
            aria-checked={tailMode}
            aria-label="Tail mode: follow file changes"
            onclick={onToggleTail}
            use:tooltip={{ text: 'Auto-follow file changes', shortcut: 'F' }}
        >
            Tail
        </button>
        {#if isIndexing}
            <span class="viewer-toolbar-indexing" role="status" aria-live="polite">Reindexing…</span>
        {/if}
    </div>
</header>

<style>
    .viewer-toolbar {
        /* The 72 px wide left gutter reserves space for the macOS traffic
           lights, which sit at trafficLightPosition { x: 9, y: 17 } per
           open-viewer.ts. Stylelint forbids raw px in `padding` shorthand,
           so the gutter goes on a pseudo-element instead. */
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        padding: var(--spacing-xs) var(--spacing-sm);
        background: var(--color-bg-secondary);
        border-bottom: 1px solid var(--color-border-strong);
        flex-shrink: 0;
        min-height: 38px;
    }

    .viewer-toolbar::before {
        content: '';
        display: block;
        width: 72px;
        flex-shrink: 0;
    }

    .viewer-toolbar-title {
        flex: 1;
        min-width: 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
        user-select: none;
    }

    .viewer-toolbar-pickers {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        flex-shrink: 0;
    }

    .viewer-toolbar-indexing {
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
        font-style: italic;
    }

    /* Tail toggle: same chrome as the search-bar toggles, sized for the toolbar. */
    .viewer-toolbar-toggle {
        background: var(--color-bg-tertiary);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-sm);
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        font-weight: 500;
        /* stylelint-disable-next-line declaration-property-value-disallowed-list -- compact toolbar button */
        padding: 2px 10px;
        line-height: 1.4;
        transition: all var(--transition-base);
    }

    .viewer-toolbar-toggle:hover {
        background: var(--color-bg-secondary);
    }

    .viewer-toolbar-toggle:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 1px;
    }

    .viewer-toolbar-toggle.active {
        background: var(--color-accent-subtle);
        border-color: var(--color-accent);
        color: var(--color-accent-text);
    }
</style>
