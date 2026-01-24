<script lang="ts" module>
    function formatSize(bytes: number): string {
        if (bytes < 1024) return `${String(bytes)} B`
        if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
        if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
        return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`
    }
</script>

<script lang="ts">
    import { onMount, tick } from 'svelte'
    import { findMatches, nextMatchIndex, prevMatchIndex, type SearchMatch } from '$lib/file-viewer/viewer-search'

    let content = $state('')
    let lines = $state<string[]>([])
    let fileName = $state('')
    let lineCount = $state(0)
    let fileSize = $state(0)
    let error = $state('')
    let loading = $state(true)

    // Search state
    let searchVisible = $state(false)
    let searchQuery = $state('')
    let matches = $state<SearchMatch[]>([])
    let currentMatchIndex = $state(-1)
    let searchInputRef: HTMLInputElement | undefined = $state()

    // Viewer container ref for scrolling
    let viewerRef: HTMLDivElement | undefined = $state()

    // Compute line number gutter width based on total lines
    const gutterWidth = $derived(String(lineCount).length)

    // Update matches when query or content changes
    $effect(() => {
        if (searchQuery && lines.length > 0) {
            matches = findMatches(lines, searchQuery)
            currentMatchIndex = matches.length > 0 ? 0 : -1
        } else {
            matches = []
            currentMatchIndex = -1
        }
    })

    // Scroll to current match when it changes
    $effect(() => {
        if (currentMatchIndex >= 0 && matches[currentMatchIndex]) {
            scrollToMatch(matches[currentMatchIndex])
        }
    })

    function scrollToMatch(match: SearchMatch) {
        if (!viewerRef) return
        const lineEl = viewerRef.querySelector(`[data-line="${String(match.line)}"]`)
        if (lineEl) {
            lineEl.scrollIntoView({ block: 'center', behavior: 'smooth' })
        }
    }

    function openSearch() {
        searchVisible = true
        void tick().then(() => {
            searchInputRef?.focus()
            searchInputRef?.select()
        })
    }

    function closeSearch() {
        searchVisible = false
        searchQuery = ''
        matches = []
        currentMatchIndex = -1
    }

    function findNext() {
        if (matches.length === 0) return
        currentMatchIndex = nextMatchIndex(currentMatchIndex, matches.length)
    }

    function findPrev() {
        if (matches.length === 0) return
        currentMatchIndex = prevMatchIndex(currentMatchIndex, matches.length)
    }

    async function closeWindow() {
        try {
            const { getCurrentWindow } = await import('@tauri-apps/api/window')
            await getCurrentWindow().close()
        } catch {
            // Not in Tauri environment
        }
    }

    function handleKeyDown(e: KeyboardEvent) {
        // Cmd+F or Ctrl+F: open search
        if ((e.metaKey || e.ctrlKey) && e.key === 'f') {
            e.preventDefault()
            openSearch()
            return
        }

        // Escape: close search first, then close window
        if (e.key === 'Escape') {
            e.preventDefault()
            if (searchVisible) {
                closeSearch()
            } else {
                void closeWindow()
            }
            return
        }

        // Enter in search: find next (Shift+Enter: find previous)
        if (e.key === 'Enter' && searchVisible) {
            e.preventDefault()
            if (e.shiftKey) {
                findPrev()
            } else {
                findNext()
            }
        }
    }

    /** Renders a line with search highlights. Returns an array of segments. */
    function getHighlightedSegments(lineIdx: number, lineText: string) {
        const lineMatches = matches.filter((m) => m.line === lineIdx)
        if (lineMatches.length === 0) {
            return [{ text: lineText, highlight: false, active: false }]
        }

        const segments: Array<{ text: string; highlight: boolean; active: boolean }> = []
        let pos = 0
        for (const m of lineMatches) {
            if (m.start > pos) {
                segments.push({ text: lineText.slice(pos, m.start), highlight: false, active: false })
            }
            const isActive = matches.indexOf(m) === currentMatchIndex
            segments.push({ text: lineText.slice(m.start, m.start + m.length), highlight: true, active: isActive })
            pos = m.start + m.length
        }
        if (pos < lineText.length) {
            segments.push({ text: lineText.slice(pos), highlight: false, active: false })
        }
        return segments
    }

    onMount(async () => {
        // Hide the loading screen
        const loadingScreen = document.getElementById('loading-screen')
        if (loadingScreen) {
            loadingScreen.style.display = 'none'
        }

        // Get file path from URL query parameter
        const params = new URLSearchParams(window.location.search)
        const filePath = params.get('path')

        if (!filePath) {
            error = 'No file path specified'
            loading = false
            return
        }

        try {
            const { readFileContent } = await import('$lib/tauri-commands')
            const result = await readFileContent(filePath)
            content = result.content
            lines = content.split('\n')
            fileName = result.fileName
            lineCount = result.lineCount
            fileSize = result.size

            // Set window title
            try {
                const { getCurrentWindow } = await import('@tauri-apps/api/window')
                await getCurrentWindow().setTitle(`${result.fileName} — Viewer`)
            } catch {
                // Not in Tauri environment
            }
        } catch (e) {
            error = typeof e === 'string' ? e : 'Failed to read file'
        } finally {
            loading = false
        }
    })
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="viewer-container" onkeydown={handleKeyDown} tabindex={-1} bind:this={viewerRef}>
    {#if searchVisible}
        <div class="search-bar" role="search">
            <input
                bind:this={searchInputRef}
                bind:value={searchQuery}
                type="text"
                placeholder="Find in file..."
                aria-label="Search text"
                class="search-input"
            />
            <span class="match-count" aria-live="polite">
                {#if matches.length > 0}
                    {currentMatchIndex + 1} of {matches.length}
                {:else if searchQuery}
                    No matches
                {/if}
            </span>
            <button
                onclick={findPrev}
                disabled={matches.length === 0}
                aria-label="Previous match"
                title="Previous match (Shift+Enter)">&#x25B2;</button
            >
            <button
                onclick={findNext}
                disabled={matches.length === 0}
                aria-label="Next match"
                title="Next match (Enter)">&#x25BC;</button
            >
            <button onclick={closeSearch} aria-label="Close search" title="Close (Escape)">&#x2715;</button>
        </div>
    {/if}

    {#if loading}
        <div class="status-message">Loading...</div>
    {:else if error}
        <div class="status-message error">{error}</div>
    {:else}
        <div class="file-content" role="document" aria-label="File content: {fileName}">
            {#each lines as line, idx (idx)}
                <div class="line" data-line={idx}>
                    <span class="line-number" style="width: {gutterWidth}ch" aria-hidden="true">{idx + 1}</span>
                    <span class="line-text"
                        >{#each getHighlightedSegments(idx, line) as seg, segIdx (segIdx)}{#if seg.highlight}<mark
                                    class:active={seg.active}>{seg.text}</mark
                                >{:else}{seg.text}{/if}{/each}</span
                    >
                </div>
            {/each}
        </div>
    {/if}

    <div class="status-bar" aria-label="File information">
        <span>{fileName}</span>
        <span>{lineCount} {lineCount === 1 ? 'line' : 'lines'}</span>
        <span>{formatSize(fileSize)}</span>
        <span class="shortcut-hint">Ctrl+F search &middot; Esc close</span>
    </div>
</div>

<style>
    .viewer-container {
        display: flex;
        flex-direction: column;
        height: 100vh;
        font-family: var(--font-system);
        background: var(--color-bg-primary);
        color: var(--color-text-primary);
        outline: none;
    }

    .search-bar {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        padding: var(--spacing-xs) var(--spacing-sm);
        background: var(--color-bg-secondary);
        border-bottom: 1px solid var(--color-border-primary);
        flex-shrink: 0;
    }

    .search-input {
        flex: 1;
        max-width: 300px;
        padding: 3px 8px;
        border: 1px solid var(--color-border-primary);
        border-radius: 4px;
        background: var(--color-bg-primary);
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        font-family: var(--font-system);
    }

    .search-input:focus {
        border-color: var(--color-accent);
        outline: none;
    }

    .match-count {
        font-size: 11px;
        color: var(--color-text-secondary);
        min-width: 70px;
    }

    .search-bar button {
        background: var(--color-bg-tertiary);
        border: 1px solid var(--color-border-secondary);
        border-radius: 3px;
        color: var(--color-text-primary);
        font-size: 11px;
        padding: 2px 6px;
        cursor: pointer;
        line-height: 1;
    }

    .search-bar button:hover:not(:disabled) {
        background: var(--color-button-hover);
    }

    .search-bar button:disabled {
        opacity: 0.4;
        cursor: default;
    }

    .file-content {
        flex: 1;
        overflow: auto;
        font-family: 'SF Mono', Menlo, Monaco, Consolas, monospace;
        font-size: 12px;
        line-height: 1.5;
        padding: var(--spacing-xs) 0;
        user-select: text;
        -webkit-user-select: text;
        cursor: text;
    }

    .line {
        display: flex;
        padding: 0 var(--spacing-sm);
        min-height: 18px;
    }

    .line:hover {
        background: var(--color-bg-hover);
    }

    .line-number {
        display: inline-block;
        text-align: right;
        color: var(--color-text-muted);
        padding-right: var(--spacing-sm);
        margin-right: var(--spacing-sm);
        border-right: 1px solid var(--color-border-secondary);
        flex-shrink: 0;
        user-select: none;
        -webkit-user-select: none;
    }

    .line-text {
        white-space: pre;
        word-break: break-all;
        flex: 1;
        min-width: 0;
    }

    mark {
        background: #fff3a8;
        color: #000;
        border-radius: 2px;
        padding: 0 1px;
    }

    mark.active {
        background: #ff9632;
        color: #fff;
    }

    @media (prefers-color-scheme: dark) {
        mark {
            background: #665d20;
            color: #fff;
        }
        mark.active {
            background: #cc6600;
            color: #fff;
        }
    }

    .status-bar {
        display: flex;
        align-items: center;
        gap: var(--spacing-md);
        padding: var(--spacing-xs) var(--spacing-sm);
        background: var(--color-bg-secondary);
        border-top: 1px solid var(--color-border-primary);
        font-size: 11px;
        color: var(--color-text-secondary);
        flex-shrink: 0;
    }

    .shortcut-hint {
        margin-left: auto;
        color: var(--color-text-muted);
    }

    .status-message {
        display: flex;
        align-items: center;
        justify-content: center;
        flex: 1;
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
    }

    .status-message.error {
        color: var(--color-error);
    }
</style>
