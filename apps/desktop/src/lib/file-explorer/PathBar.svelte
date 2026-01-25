<script lang="ts">
    import { copyToClipboard } from '$lib/tauri-commands'

    interface Props {
        /** Full absolute path */
        path: string
        /** Volume base path (stripped from display) */
        volumePath: string
        /** Home directory path for ~ substitution */
        homeDir?: string
    }

    const { path, volumePath, homeDir }: Props = $props()

    // Display path: use ~ if path starts with homeDir, otherwise strip volumePath prefix
    const displayPath = $derived.by(() => {
        if (homeDir && path.startsWith(homeDir)) {
            // Show ~ for home directory
            const afterHome = path.slice(homeDir.length)
            return '~' + (afterHome || '/')
        }
        // Fall back to volume-relative path
        if (path.startsWith(volumePath)) {
            return path.slice(volumePath.length) || '/'
        }
        return path
    })

    // Full path for copying (always the real path)
    const copyPath = $derived(path)

    // State for copy feedback
    let showCopied = $state(false)
    let copyTimeout: ReturnType<typeof setTimeout> | undefined

    // State for drag detection
    let isDragging = $state(false)
    let startX = 0
    let startY = 0
    const dragThreshold = 3 // pixels

    function handleMouseDown(e: MouseEvent) {
        // Only handle left click
        if (e.button !== 0) return

        startX = e.clientX
        startY = e.clientY
        isDragging = false

        // Add temporary listeners for move and up
        document.addEventListener('mousemove', handleMouseMove)
        document.addEventListener('mouseup', handleMouseUp)
    }

    function handleMouseMove(e: MouseEvent) {
        const dx = Math.abs(e.clientX - startX)
        const dy = Math.abs(e.clientY - startY)

        if (dx > dragThreshold || dy > dragThreshold) {
            isDragging = true
        }
    }

    function handleMouseUp() {
        document.removeEventListener('mousemove', handleMouseMove)
        document.removeEventListener('mouseup', handleMouseUp)

        if (!isDragging) {
            // It was a click, copy to clipboard
            void copyToClipboard(copyPath)

            // Show feedback
            if (copyTimeout) clearTimeout(copyTimeout)
            showCopied = true
            copyTimeout = setTimeout(() => {
                showCopied = false
            }, 1500)
        }

        // Reset state after a short delay to allow selection to complete
        setTimeout(() => {
            isDragging = false
        }, 100)
    }
</script>

<div class="path-bar" class:is-selecting={isDragging} title="Click to copy, drag to select">
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <span class="path-text" class:selectable={isDragging} onmousedown={handleMouseDown}>{displayPath}</span>

    {#if showCopied}
        <span class="copied-feedback">Copied ✓</span>
    {/if}
</div>

<style>
    .path-bar {
        position: relative;
        display: flex;
        align-items: center;
        flex: 1;
        min-width: 0;
        overflow: hidden;
    }

    .path-text {
        font-family: var(--font-system), sans-serif;
        color: var(--color-text-secondary);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        flex: 1;
        min-width: 0;
        cursor: default;
        user-select: none;
        -webkit-user-select: none;
    }

    .path-text.selectable {
        user-select: text;
        -webkit-user-select: text;
        cursor: text;
        overflow-x: auto;
        text-overflow: clip;
    }

    /* When selecting, allow horizontal scroll for long paths */
    .path-bar.is-selecting {
        overflow-x: auto;
    }

    .copied-feedback {
        position: absolute;
        right: 0;
        top: 50%;
        transform: translateY(-50%);
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
        padding: 2px 6px;
        border-radius: 4px;
        font-size: 11px;
        font-weight: 500;
        animation: fade-out 1.5s ease-out forwards;
        pointer-events: none;
        box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
    }

    @keyframes fade-out {
        0% {
            opacity: 1;
        }
        70% {
            opacity: 1;
        }
        100% {
            opacity: 0;
        }
    }
</style>
