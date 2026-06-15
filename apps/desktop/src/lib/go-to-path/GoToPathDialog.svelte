<script lang="ts">
    /**
     * "Go to path" dialog (⌘G). A small modal with an auto-focused textbox, a
     * list of up to 10 recent paths (each with a digit chip, the middle-
     * truncated path, and a remove `[x]` button), a live inline warning for the
     * nearest-ancestor case, and Cancel / "Go to path" buttons.
     *
     * Muscle-memory flows we optimize:
     * - ⌘G → Enter on a clipboard path (prefilled when the clipboard resolves
     *   to something on disk).
     * - ⌘G → digit (1–9, 0) jumps to a recent — but ONLY while the box is empty
     *   (no valid path starts with a digit, so this is unambiguous).
     *
     * Smart-backend, thin-frontend: the backend's `resolve_go_to_path` owns all
     * path reasoning. This dialog only presents state and dispatches.
     */
    import { onMount, onDestroy, tick } from 'svelte'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Button from '$lib/ui/Button.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { useShortenMiddle } from '$lib/utils/shorten-middle-action'
    import { createDebounce, withTimeout } from '$lib/utils/timing'
    import { readClipboardText } from '$lib/tauri-commands'
    import { commands, type GoToPathResolution } from '$lib/ipc/bindings'
    import { getAppLogger } from '$lib/logging/logger'
    import { digitToRecentIndex, shouldPrefillClipboard } from './go-to-path'
    import {
        getRecentPathsList,
        loadRecentPaths,
        removeRecentPath as removeRecentPathFromState,
    } from './recent-paths-state.svelte'
    import Icon from '$lib/ui/Icon.svelte'

    interface Props {
        /** The focused pane's current path; relative input resolves against it. */
        baseDir: string
        /** Jump handler. Closes the dialog on a successful (non-invalid) jump. */
        onGo: (input: string) => Promise<GoToPathResolution | undefined>
        onCancel: () => void
    }

    const { baseDir, onGo, onCancel }: Props = $props()

    const log = getAppLogger('go-to-path')

    /** Debounce for the live preview resolve. Long enough not to thrash a hung mount. */
    const RESOLVE_DEBOUNCE_MS = 200
    /** Hard cap so a slow mount can never freeze typing (per the plan). */
    const RESOLVE_TIMEOUT_MS = 2000

    let inputValue = $state('')
    let inputRef: HTMLInputElement | undefined = $state()
    /** Nearest-ancestor preview line below the box, or `''` when there's none. */
    let ancestorHint = $state('')
    let isGoing = $state(false)

    const recents = $derived(getRecentPathsList().slice(0, 10))
    const canGo = $derived(inputValue.trim().length > 0)

    /** Digit shown on a recent row: index 0–8 → 1–9, index 9 → 0. */
    function digitFor(index: number): string {
        return index === 9 ? '0' : String(index + 1)
    }

    /**
     * Resolve the current box value for the live preview only. Returns the
     * nearest-ancestor hint string, or `''` for directory/file/invalid (no
     * warning shown). Wrapped in `withTimeout` so a hung mount never blocks.
     */
    async function previewResolve(value: string): Promise<void> {
        const trimmed = value.trim()
        if (trimmed === '') {
            ancestorHint = ''
            return
        }
        const resolution = await withTimeout(resolveOrNull(trimmed), RESOLVE_TIMEOUT_MS, null)
        // A later keystroke may have changed the box while we awaited; only
        // apply if the value we resolved is still current.
        if (value !== inputValue) return
        ancestorHint =
            resolution?.kind === 'nearestAncestor'
                ? `This path doesn't exist. The closest place to go is ${resolution.ancestorDir}.`
                : ''
    }

    /** Reads the live box value. A function so the TS literal-narrowing across
     * `await` boundaries doesn't flatten the post-await re-checks to constants. */
    function boxIsEmpty(): boolean {
        return inputValue === ''
    }

    async function resolveOrNull(input: string): Promise<GoToPathResolution | null> {
        const result = await commands.resolveGoToPath(input, baseDir)
        return result.status === 'ok' ? result.data : null
    }

    const debouncedPreview = createDebounce(() => {
        void previewResolve(inputValue)
    }, RESOLVE_DEBOUNCE_MS)

    function handleInput() {
        debouncedPreview.call()
    }

    async function confirmGo(): Promise<void> {
        const trimmed = inputValue.trim()
        if (!trimmed || isGoing) return
        isGoing = true
        try {
            const resolution = await onGo(trimmed)
            // `invalid` keeps the dialog open (the user should fix their input);
            // every other outcome jumped, so close.
            if (resolution && resolution.kind !== 'invalid') {
                onCancel()
            }
        } finally {
            isGoing = false
        }
    }

    async function jumpToRecent(path: string): Promise<void> {
        if (isGoing) return
        isGoing = true
        try {
            await onGo(path)
            onCancel()
        } finally {
            isGoing = false
        }
    }

    async function handleRemoveRecent(event: MouseEvent, id: string): Promise<void> {
        // Don't let the click bubble to the row (which would jump).
        event.stopPropagation()
        await removeRecentPathFromState(id)
        // Keep keyboard focus inside the dialog after the row disappears.
        inputRef?.focus()
    }

    function handleInputKeydown(event: KeyboardEvent): void {
        if (event.key === 'Enter') {
            event.preventDefault()
            event.stopPropagation()
            void confirmGo()
            return
        }
        // Digit jump: only while the box is empty. The empty-box guard is
        // unambiguous because no valid path starts with a digit (paths start
        // with `/`, `~`, or `.`), so once any character is in the box, digits
        // are ordinary input.
        const modifierHeld = event.metaKey || event.ctrlKey || event.altKey
        const index = digitToRecentIndex(inputValue, event.key, recents.length, modifierHeld)
        if (index !== null) {
            event.preventDefault()
            event.stopPropagation()
            void jumpToRecent(recents[index].path)
        }
    }

    onMount(async () => {
        await tick()
        inputRef?.focus()

        // Load recents and try a clipboard prefill in parallel.
        void loadRecentPaths()

        try {
            const clip = (await readClipboardText())?.trim() ?? ''
            // Only prefill if the box is still empty (the user may have typed
            // already, here or after the resolve await) and the clipboard
            // resolves to something on disk. `boxIsEmpty()` reads the live
            // value each time so the post-await re-check is honoured (a literal
            // `inputValue === ''` would be narrowed to "always true" by TS).
            if (clip && boxIsEmpty()) {
                const resolution = await resolveOrNull(clip)
                if (resolution && shouldPrefillClipboard(resolution) && boxIsEmpty()) {
                    inputValue = clip
                    await tick()
                    inputRef?.select()
                }
            }
        } catch (error) {
            log.debug('Clipboard prefill skipped: {error}', { error })
        }
    })

    onDestroy(() => {
        debouncedPreview.cancel()
    })
</script>

<ModalDialog
    titleId="go-to-path-title"
    dialogId="go-to-path"
    onclose={onCancel}
    containerStyle="width: 440px"
>
    {#snippet title()}Go to path{/snippet}

    <div class="dialog-body">
        <div class="input-group">
            <input
                bind:this={inputRef}
                bind:value={inputValue}
                type="text"
                class="path-input"
                class:has-warning={!!ancestorHint}
                aria-label="Path to go to"
                aria-describedby={ancestorHint ? 'go-to-path-warning' : undefined}
                spellcheck="false"
                autocomplete="off"
                autocapitalize="off"
                placeholder="Type or paste a path, e.g. ~/Documents"
                onkeydown={handleInputKeydown}
                oninput={handleInput}
            />
            {#if ancestorHint}
                <p id="go-to-path-warning" class="warning" role="status">{ancestorHint}</p>
            {/if}
        </div>

        {#if recents.length > 0}
            <ul class="recents" aria-label="Recent paths">
                {#each recents as recent, index (recent.id)}
                    <li class="recent-row">
                        <!-- Row body is out of the tab order on purpose: the digit
                             keys (1-9, 0) are the keyboard path to jumping a recent,
                             so tabbing through every row body would be redundant. The
                             `[x]` remove button keeps its natural tab order so keyboard
                             users can remove a recent (digits can't express that). -->
                        <button
                            type="button"
                            class="recent-main"
                            onclick={() => void jumpToRecent(recent.path)}
                            tabindex="-1"
                        >
                            <span class="digit-chip" aria-hidden="true">{digitFor(index)}</span>
                            <span
                                class="recent-path"
                                use:useShortenMiddle={{
                                    text: recent.path,
                                    preferBreakAt: '/',
                                    tooltipWhenTruncated: true,
                                }}
                            ></span>
                        </button>
                        <button
                            type="button"
                            class="remove-button"
                            aria-label="Remove from list"
                            use:tooltip={'Remove from list'}
                            onclick={(event) => void handleRemoveRecent(event, recent.id)}
                        >
                            <Icon name="x" size={14} />
                        </button>
                    </li>
                {/each}
            </ul>
        {/if}

        <div class="button-row">
            <Button variant="secondary" onclick={onCancel}>Cancel</Button>
            <Button variant="primary" onclick={() => void confirmGo()} disabled={!canGo || isGoing}>Go to path</Button>
        </div>
    </div>
</ModalDialog>

<style>
    .dialog-body {
        padding: 0 var(--spacing-xl) var(--spacing-xl);
    }

    .input-group {
        margin-bottom: var(--spacing-md);
    }

    .path-input {
        width: 100%;
        padding: var(--spacing-md) var(--spacing-md);
        font-size: var(--font-size-md);
        font-family: var(--font-mono);
        background: var(--color-bg-primary);
        border: 2px solid var(--color-accent);
        border-radius: var(--radius-md);
        color: var(--color-text-primary);
        box-sizing: border-box;
    }

    .path-input.has-warning {
        border-color: var(--color-warning);
    }

    .path-input::placeholder {
        color: var(--color-text-tertiary);
        font-family: var(--font-system) sans-serif;
    }

    .path-input:focus {
        outline: none;
        box-shadow: var(--shadow-focus);
    }

    .warning {
        margin: var(--spacing-sm) 0 0;
        font-size: var(--font-size-sm);
        color: var(--color-warning);
        line-height: 1.4;
        word-break: break-all;
    }

    .recents {
        list-style: none;
        margin: 0 0 var(--spacing-lg);
        padding: 0;
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xxs);
        max-height: 280px;
        overflow-y: auto;
    }

    .recent-row {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        border-radius: var(--radius-sm);
    }

    .recent-row:hover {
        background: var(--color-bg-secondary);
    }

    .recent-main {
        flex: 1 1 auto;
        min-width: 0;
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        padding: var(--spacing-xs) var(--spacing-sm);
        background: none;
        border: none;
        text-align: left;
    }

    .digit-chip {
        flex: 0 0 auto;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 1.4em;
        height: 1.4em;
        font-size: var(--font-size-xs);
        font-family: var(--font-mono);
        color: var(--color-text-secondary);
        background: var(--color-bg-tertiary);
        border-radius: var(--radius-xs);
    }

    .recent-path {
        flex: 1 1 auto;
        min-width: 0;
        font-size: var(--font-size-sm);
        font-family: var(--font-mono);
        color: var(--color-text-primary);
    }

    .remove-button {
        flex: 0 0 auto;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 1.6em;
        height: 1.6em;
        padding: 0;
        margin-right: var(--spacing-xs);
        font-size: var(--font-size-md);
        color: var(--color-text-tertiary);
        background: none;
        border: none;
        border-radius: var(--radius-xs);
    }

    .remove-button:hover {
        color: var(--color-text-primary);
        background: var(--color-bg-tertiary);
    }

    .remove-button:focus-visible {
        outline: none;
        box-shadow: var(--shadow-focus);
    }

    .button-row {
        display: flex;
        gap: var(--spacing-md);
        justify-content: flex-end;
    }
</style>
