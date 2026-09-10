<script lang="ts">
    /**
     * The "Show in Finder" card in `Behavior > Navigation & file ops`: one switch
     * deciding whether another app's reveal lands in a Cmdr pane.
     *
     * ❗ **Not a registry setting.** There's no `settings.json` key and no
     * `SettingsValues` entry: the answer is the macOS `NSFileViewer` preference,
     * which anyone can change from outside Cmdr, so the card reads through to the
     * OS every time it mounts and re-reads whatever the write left behind. A
     * cached flag would show a switch that disagrees with the Mac it sits on.
     * Mechanism and its decisions: `src-tauri/src/reveal/DETAILS.md`.
     *
     * Three consequences of that shape, all deliberate:
     *
     * - **It can't use `SettingRow`**, whose `id` is a `SettingId` and whose reset
     *   pip, modified dot, and change subscription are all registry reads. The row
     *   below is the same shape, hand-built.
     * - **It gates itself on search**, as the `SearchableRow` in
     *   `RevealHandlerCard.rows.ts`: it owns its card frame, so no section-level
     *   `anyVisible(...)` can do that for it. The row joins the index on macOS
     *   only, matching where the card renders.
     * - **It renders nothing only where there's no mechanism**: off macOS, or when
     *   the command doesn't answer (the wrapper's `null`). A build that may not
     *   write the key still shows the card, disabled, with the backend's reason.
     *
     * When another app holds the key, the switch reads off and a line names the
     * holder ("Currently: Path Finder", falling back to the raw bundle id when
     * that app isn't installed any more). Switching on takes the key over: one
     * explicit click, never silently.
     *
     * ❗ **The switch is disabled when `blockedBy` is set**: on a debug, worktree,
     * or E2E build, and for a copy of Cmdr outside an Applications folder. The
     * Applications gate never blocks switching OFF: `blockedBy` comes back `null`
     * while Cmdr holds the key, so a copy that was registered and then moved can
     * always hand it back. A fully dead switch there would strand someone
     * registered, which is the dangling key the block exists to prevent. (A
     * non-production build never holds the key, so it has nothing to hand back.)
     */
    import { onMount, type Snippet } from 'svelte'
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import Switch from '$lib/ui/Switch.svelte'
    import Trans from '$lib/intl/Trans.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { isMacOS } from '$lib/shortcuts/key-capture'
    import { getRevealHandlerState, setRevealHandlerEnabled } from '$lib/tauri-commands'
    import { REVEAL_HANDLER_ANCHOR_ID } from '$lib/reveal/reveal-settings-link'
    import { createShouldShow } from '$lib/settings/settings-search'
    import type { MessageKey } from '$lib/intl/keys.gen'
    import type { RevealHandlerBlocker, RevealHandlerStatus } from '$lib/ipc/bindings'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    /** The switch's own element id, so the visible label points at the control. */
    const SWITCH_ID = 'reveal-handler-switch'

    /**
     * `null` until the OS has answered, so no switch flashes the wrong way first,
     * and for good where there's nothing to ask (off macOS).
     */
    let handlerStatus = $state<RevealHandlerStatus | null>(null)
    /** True while a take-over or hand-back is in flight; the switch stays put meanwhile. */
    let applying = $state(false)

    onMount(() => {
        // Non-macOS has no mechanism at all, so skip the round-trip: the wrapper
        // would answer `null` and the card would hide either way.
        if (!isMacOS()) return
        void (async () => {
            handlerStatus = await getRevealHandlerState()
        })()
    })

    /**
     * The switch's own on/off. A WRITABLE `$derived`, and `bind:checked` below,
     * because both halves are load-bearing: Ark flips the switch locally on
     * click (so it needs somewhere to write), and a refused take-over comes back
     * on the SAME `false` the row started from (so the answer has to re-derive
     * and discard that write). ❌ Not a plain prop: the switch would sit there
     * reading "on" while the key belongs to someone else.
     */
    let switchOn = $derived(handlerStatus?.state.kind === 'registered')

    const otherHolder = $derived.by(() => {
        const state = handlerStatus?.state
        if (state?.kind !== 'heldByOtherApp') return null
        return state.displayName ?? state.bundleId
    })
    const shouldShow = $derived(createShouldShow(searchQuery))
    const visible = $derived(handlerStatus !== null && shouldShow('row:behavior.revealHandler'))

    /** Why the switch won't take a yes, or `null` while it will. */
    const blockedBy = $derived(handlerStatus?.blockedBy ?? null)

    // `$derived` so a live language switch re-renders the label and the accessible
    // name together; `t()` reads the locale-version rune.
    const label = $derived(tString('settings.revealHandler.label'))

    /**
     * The reason, worded. Keyed on the typed variant, ❌ never on a message the backend
     * sent (`cmdr/no-error-string-match`); a `Record` over the union means a new variant
     * is a compile error here rather than a silently unexplained disabled switch.
     */
    const BLOCKER_MESSAGES: Record<RevealHandlerBlocker, MessageKey> = {
        notProductionBuild: 'settings.revealHandler.notProductionBuild',
        notInApplications: 'settings.revealHandler.notInApplications',
    }

    const blockedReason = $derived(blockedBy === null ? null : tString(BLOCKER_MESSAGES[blockedBy]))

    async function handleChange(next: boolean): Promise<void> {
        if (applying) return
        applying = true
        try {
            // The command answers with the state the OS was LEFT in, not the one
            // asked for: another app can take the key between the read and the
            // click, and the row has to render the truth. It refuses outright when
            // `blockedBy` is set, and the answer says so.
            handlerStatus = await setRevealHandlerEnabled(next)
        } finally {
            applying = false
        }
    }
</script>

{#if visible}
    <!-- The id is the deep-link target for the first-reveal notice; an OS-backed row has
         no `SettingId` for `settingAnchorId` to derive one from. -->
    <SectionCard
        id={REVEAL_HANDLER_ANCHOR_ID}
        label={tString('settings.navigationAndFileOps.card.showInFinder')}
    >
        <!-- The tooltip sits on the whole row, not the switch: a disabled control takes no
             pointer events, so a tooltip on it would never show. The same sentence is in the
             row as `sr-only` text, so the reason isn't hover-only. -->
        <div class="reveal-row" use:tooltip={blockedReason ?? ''}>
            <div class="reveal-header">
                <label class="reveal-label" for={SWITCH_ID}>{label}</label>
                <Switch
                    id={SWITCH_ID}
                    bind:checked={switchOn}
                    disabled={applying || blockedBy !== null}
                    ariaLabel={label}
                    onCheckedChange={(next: boolean) => void handleChange(next)}
                    data-test="reveal-handler-switch"
                />
            </div>
            <p class="reveal-description">{tString('settings.revealHandler.description')}</p>
            {#if blockedReason}
                <p class="sr-only" data-test="reveal-handler-blocked">{blockedReason}</p>
            {/if}
            {#if switchOn}
                <!-- Only once it's ON. Before that it's friction on a decision nobody has
                     made yet; after it, it's the one thing that matters, because nothing of
                     ours runs at uninstall to clear the key. -->
                <p class="reveal-warning" data-test="reveal-handler-warning">
                    <Trans key="settings.revealHandler.uninstallWarning" snippets={{ lead }} />
                </p>
            {/if}
            {#if otherHolder}
                <p class="reveal-holder">
                    {tString('settings.revealHandler.heldByOtherApp', { app: otherHolder })}
                </p>
            {/if}
        </div>
    </SectionCard>
{/if}

{#snippet lead(children: Snippet)}
    <strong>{@render children()}</strong>
{/snippet}

<style>
    /* Mirrors `SettingRow`'s frame. The row can't use that component (its `id` is a
       `SettingId`), and there's no border-bottom here because the card holds one row. */
    .reveal-row {
        padding: var(--spacing-sm) 0;
    }

    .reveal-header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--spacing-md);
    }

    .reveal-label {
        font-weight: 500;
        color: var(--color-text-primary);
    }

    .reveal-description,
    .reveal-holder,
    .reveal-warning {
        margin: var(--spacing-xs) 0 0;
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
    }

    /* The uninstall warning is the one line here that costs something to miss, so it takes
       the warning tint the rest of the app uses for "read this before you act". */
    .reveal-warning {
        color: var(--color-warning-text);
    }
</style>
