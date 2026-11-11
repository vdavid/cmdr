<script lang="ts">
    /**
     * The switch for the Finder extension: "Open in Cmdr" at the top level of Finder's
     * right-click menu. The same item is always available through the macOS service, so
     * this row only moves it up, at the cost of a small process that runs alongside Finder.
     *
     * ❗ **Not a registry setting**, for the reason `RevealHandlerCard` isn't: macOS holds
     * the on/off and the person can change it in System Settings, so the card reads through
     * on every mount and renders what the write left behind. `src-tauri/src/reveal/finder_extension.rs`.
     */
    import { onMount } from 'svelte'
    import SectionCard from '#lib/ui/SectionCard.svelte'
    import Switch from '#lib/ui/Switch.svelte'
    import { tString } from '#lib/intl/messages.svelte.js'
    import { tooltip } from '#lib/tooltip/tooltip.js'
    import { hasCapability } from '#lib/platform/index.js'
    import { getFinderExtensionStatus, setFinderExtensionEnabled } from '#lib/tauri-commands/index.js'
    import { createShouldShow } from '#lib/settings/settings-search.js'
    import type { MessageKey } from '#lib/intl/keys.gen.js'
    import type { FinderExtensionStatus, RevealHandlerBlocker } from '#lib/ipc/bindings.js'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    const SWITCH_ID = 'finder-extension-switch'

    /** `null` until the OS has answered, and for good where this OS has no Finder extension. */
    let status = $state<FinderExtensionStatus | null>(null)
    let applying = $state(false)

    onMount(() => {
        if (!hasCapability('finderExtension')) return
        void (async () => {
            status = await getFinderExtensionStatus()
        })()
    })

    /** Writable, because Ark flips it on click and a refusal has to re-derive over that. */
    let switchOn = $derived(status?.enabled === true)

    const shouldShow = $derived(createShouldShow(searchQuery))
    const visible = $derived(status !== null && shouldShow('row:behavior.finderExtension'))
    const blockedBy = $derived(status?.blockedBy ?? null)
    const label = $derived(tString('settings.finderExtension.label'))

    /** Keyed on the typed variant, so a new one is a compile error here. */
    const BLOCKER_MESSAGES: Record<RevealHandlerBlocker, MessageKey> = {
        notProductionBuild: 'settings.finderExtension.notProductionBuild',
        notInApplications: 'settings.finderExtension.notInApplications',
    }
    const blockedReason = $derived(blockedBy === null ? null : tString(BLOCKER_MESSAGES[blockedBy]))

    async function handleChange(next: boolean): Promise<void> {
        if (applying) return
        applying = true
        try {
            status = await setFinderExtensionEnabled(next)
        } finally {
            applying = false
        }
    }
</script>

{#if visible}
    <SectionCard label={tString('settings.finderExtension.card')}>
        <!-- On the whole row: a disabled control takes no pointer events. The same sentence is
             in the row as `sr-only` text, so the reason isn't hover-only. -->
        <div class="row" use:tooltip={blockedReason ?? ''}>
            <div class="header">
                <label class="label" for={SWITCH_ID}>{label}</label>
                <Switch
                    id={SWITCH_ID}
                    bind:checked={switchOn}
                    disabled={applying || blockedBy !== null}
                    ariaLabel={label}
                    onCheckedChange={(next: boolean) => void handleChange(next)}
                    data-test="finder-extension-switch"
                />
            </div>
            <p class="description">{tString('settings.finderExtension.description')}</p>
            {#if blockedReason}
                <p class="sr-only" data-test="finder-extension-blocked">{blockedReason}</p>
            {/if}
        </div>
    </SectionCard>
{/if}

<style>
    /* Mirrors `RevealHandlerCard`'s row, which mirrors `SettingRow`'s frame. */
    .row {
        padding: var(--spacing-sm) 0;
    }

    .header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--spacing-md);
    }

    .label {
        font-weight: 500;
        color: var(--color-text-primary);
    }

    .description {
        margin: var(--spacing-xs) 0 0;
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
    }
</style>
