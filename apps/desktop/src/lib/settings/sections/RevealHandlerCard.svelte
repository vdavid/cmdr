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
     * - **It isn't in the settings search index**, so any search query hides it
     *   (`shouldShow` answers `false` for an id it's never seen, which is exactly
     *   the behavior wanted). A hit that scrolled to a row this machine doesn't
     *   render would be worse than no hit.
     * - **It renders nothing on `unavailable`**, which means a debug, worktree, or
     *   E2E build that must never write the key, plus every non-macOS platform.
     *   Real users on macOS never see that state, so a disabled row explaining it
     *   would be copy shipped only to us.
     *
     * When another app holds the key, the switch reads off and a line names the
     * holder ("Currently: Path Finder", falling back to the raw bundle id when
     * that app isn't installed any more). Switching on takes the key over: one
     * explicit click, never silently.
     */
    import { onMount } from 'svelte'
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import Switch from '$lib/ui/Switch.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { isMacOS } from '$lib/shortcuts/key-capture'
    import { getRevealHandlerState, setRevealHandlerEnabled } from '$lib/tauri-commands'
    import type { RevealHandlerState } from '$lib/ipc/bindings'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    /** The switch's own element id, so the visible label points at the control. */
    const SWITCH_ID = 'reveal-handler-switch'

    /** `null` until the OS has answered, so no switch flashes the wrong way first. */
    let handlerState = $state<RevealHandlerState | null>(null)
    /** True while a take-over or hand-back is in flight; the switch stays put meanwhile. */
    let applying = $state(false)

    onMount(() => {
        // Non-macOS has no mechanism at all, so skip the round-trip: the wrapper
        // would answer `unavailable` and the card would hide either way.
        if (!isMacOS()) return
        void (async () => {
            handlerState = await getRevealHandlerState()
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
    let switchOn = $derived(handlerState?.kind === 'registered')

    const otherHolder = $derived(
        handlerState?.kind === 'heldByOtherApp' ? (handlerState.displayName ?? handlerState.bundleId) : null,
    )
    const visible = $derived(
        !searchQuery.trim() && handlerState !== null && handlerState.kind !== 'unavailable',
    )

    // `$derived` so a live language switch re-renders the label and the accessible
    // name together; `t()` reads the locale-version rune.
    const label = $derived(tString('settings.revealHandler.label'))

    async function handleChange(next: boolean): Promise<void> {
        if (applying) return
        applying = true
        try {
            // The command answers with the state the OS was LEFT in, not the one
            // asked for: another app can take the key between the read and the
            // click, and the row has to render the truth.
            handlerState = await setRevealHandlerEnabled(next)
        } finally {
            applying = false
        }
    }
</script>

{#if visible}
    <SectionCard label={tString('settings.navigationAndFileOps.card.showInFinder')}>
        <div class="reveal-row">
            <div class="reveal-header">
                <label class="reveal-label" for={SWITCH_ID}>{label}</label>
                <Switch
                    id={SWITCH_ID}
                    bind:checked={switchOn}
                    disabled={applying}
                    ariaLabel={label}
                    onCheckedChange={(next: boolean) => void handleChange(next)}
                    data-test="reveal-handler-switch"
                />
            </div>
            <p class="reveal-description">{tString('settings.revealHandler.description')}</p>
            {#if otherHolder}
                <p class="reveal-holder">
                    {tString('settings.revealHandler.heldByOtherApp', { app: otherHolder })}
                </p>
            {/if}
        </div>
    </SectionCard>
{/if}

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
    .reveal-holder {
        margin: var(--spacing-xs) 0 0;
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
    }
</style>
