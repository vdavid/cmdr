<script lang="ts">
    /**
     * "Go to path" dialog (⌘G). A small modal with an auto-focused textbox, a
     * list of up to 10 recent paths (each with a digit chip, the middle-
     * truncated path, and a remove `[x]` button), a live inline hint (a warning
     * for the nearest-ancestor case, a neutral line for a server address), and
     * Cancel / "Go to path" buttons.
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
    import TextInput from '$lib/ui/TextInput.svelte'
    import Button from '$lib/ui/Button.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { useShortenMiddle } from '$lib/utils/shorten-middle-action'
    import { createDebounce, withTimeout } from '$lib/utils/timing'
    import { readClipboardText, resolveGoToPath } from '$lib/tauri-commands'
    import type { GoToPathResolution } from '$lib/ipc/bindings'
    import { getAppLogger } from '$lib/logging/logger'
    import { digitToRecentIndex, shouldPrefillClipboard } from './go-to-path'
    import { previewSchemeInput, readSchemeInput, type GoToPathOutcome } from './scheme-intercept'
    import {
        getRecentPathsList,
        loadRecentPaths,
        removeRecentPath as removeRecentPathFromState,
    } from './recent-paths-state.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        /** The focused pane's current path; relative input resolves against it. */
        baseDir: string
        /** Jump handler. Closes the dialog on a successful (non-invalid) jump. */
        onGo: (input: string) => Promise<GoToPathOutcome | undefined>
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
    /**
     * The preview line below the box, or `null` when there's none.
     *
     * ❗ Two tones, because two outcomes: `warning` is the nearest-ancestor case,
     * where the box does NOT name what the user will land on. `info` is a server
     * address, where it does. Painting the second one yellow tells a person the
     * good outcome is a problem.
     */
    let hint = $state<{ text: string; tone: 'warning' | 'info' } | null>(null)
    let isGoing = $state(false)

    const recents = $derived(getRecentPathsList().slice(0, 10))
    const canGo = $derived(inputValue.trim().length > 0)

    /** Digit shown on a recent row: index 0–8 → 1–9, index 9 → 0. */
    function digitFor(index: number): string {
        return index === 9 ? '0' : String(index + 1)
    }

    /**
     * Resolve the current box value for the live preview only. Sets the
     * nearest-ancestor warning, the neutral server-address line, or nothing at
     * all for directory/file/invalid. Wrapped in `withTimeout` so a hung mount
     * never blocks.
     */
    async function previewResolve(value: string): Promise<void> {
        const trimmed = value.trim()
        if (trimmed === '') {
            hint = null
            return
        }
        // ❗ A scheme input never reaches the local resolver, here or on the jump:
        // it would answer "this path doesn't exist" about an address that is
        // about to work.
        const intent = await withTimeout(readSchemeInput(trimmed), RESOLVE_TIMEOUT_MS, null)
        if (value !== inputValue) return
        if (intent) {
            hint = { text: previewSchemeInput(intent), tone: 'info' }
            return
        }

        const resolution = await withTimeout(resolveOrNull(trimmed), RESOLVE_TIMEOUT_MS, null)
        // A later keystroke may have changed the box while we awaited; only
        // apply if the value we resolved is still current.
        if (value !== inputValue) return
        hint =
            resolution?.kind === 'nearestAncestor'
                ? { text: tString('goToPath.dialog.ancestorHint', { dir: resolution.ancestorDir }), tone: 'warning' }
                : null
    }

    /** Reads the live box value. A function so the TS literal-narrowing across
     * `await` boundaries doesn't flatten the post-await re-checks to constants. */
    function boxIsEmpty(): boolean {
        return inputValue === ''
    }

    async function resolveOrNull(input: string): Promise<GoToPathResolution | null> {
        const result = await resolveGoToPath(input, baseDir)
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
        // Bare Enter only: ⌘Enter and friends aren't "go", and swallowing them here
        // would shadow whatever combo the user actually pressed.
        if (event.key === 'Enter' && !event.metaKey && !event.ctrlKey && !event.altKey && !event.shiftKey) {
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
                // A copied server address is exactly what someone opened this
                // box to paste, so it prefills without asking the local resolver
                // (which would call it unresolvable).
                if (await readSchemeInput(clip)) {
                    if (boxIsEmpty()) {
                        inputValue = clip
                        await tick()
                        inputRef?.select()
                    }
                    return
                }
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
    resizable="horizontal"
>
    {#snippet title()}{tString('goToPath.dialog.title')}{/snippet}

    <div class="dialog-body">
        <div class="input-group">
            <TextInput
                bind:inputElement={inputRef}
                bind:value={inputValue}
                mono
                warning={hint?.tone === 'warning'}
                ariaLabel={tString('goToPath.dialog.inputAriaLabel')}
                aria-describedby={hint ? 'go-to-path-hint' : undefined}
                spellcheck={false}
                autocomplete="off"
                autocapitalize="off"
                placeholder={tString('goToPath.dialog.inputPlaceholder')}
                onkeydown={handleInputKeydown}
                oninput={handleInput}
            />
            {#if hint}
                <p id="go-to-path-hint" class="hint" class:warning={hint.tone === 'warning'} role="status">
                    {hint.text}
                </p>
            {/if}
        </div>

        {#if recents.length > 0}
            <ul class="recents" aria-label={tString('goToPath.dialog.recentsAriaLabel')}>
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
                            aria-label={tString('goToPath.dialog.removeFromList')}
                            use:tooltip={tString('goToPath.dialog.removeFromList')}
                            onclick={(event) => void handleRemoveRecent(event, recent.id)}
                        >
                            <Icon name="x" size={14} />
                        </button>
                    </li>
                {/each}
            </ul>
        {/if}
    </div>

    {#snippet footer()}
        <Button variant="secondary" onclick={onCancel}>{tString('goToPath.dialog.cancel')}</Button>
        <Button variant="primary" onclick={() => void confirmGo()} disabled={!canGo || isGoing}
            >{tString('goToPath.dialog.confirm')}</Button
        >
    {/snippet}
</ModalDialog>

<style>
    .input-group {
        margin-bottom: var(--spacing-md);
    }

    .hint {
        margin: var(--spacing-sm) 0 0;
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        word-break: break-all;
    }

    .hint.warning {
        color: var(--color-warning);
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
</style>
