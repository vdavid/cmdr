<script lang="ts">
    import { commands } from '$lib/ipc/bindings'
    import { onMount } from 'svelte'
    import SettingsSection from '../components/SettingsSection.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import Button from '$lib/ui/Button.svelte'
    import { getSettingDefinition } from '$lib/settings'
    import { createShouldShow } from '$lib/settings/settings-search'
    import Size from '$lib/ui/Size.svelte'
    import { getAppLogger } from '$lib/logging/logger'
    import {
        getGlobalRevealEnabled,
        getGlobalRevealBinding,
        setGlobalRevealEnabled,
        setGlobalRevealBinding,
        GLOBAL_REVEAL_ENABLED_KEY,
    } from '$lib/downloads/global-shortcut-setting'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    const log = getAppLogger('settings')

    const shouldShow = $derived(createShouldShow(searchQuery))

    const enabledDef = getSettingDefinition('indexing.enabled') ?? { label: '', description: '' }
    const globalShortcutDef = getSettingDefinition(GLOBAL_REVEAL_ENABLED_KEY) ?? {
        label: '',
        description: '',
    }

    // Inline status indicator for the global-shortcut row. Updated via the
    // backend's `set_global_reveal_shortcut` ack on every flip plus a mount-time
    // poll. M7 will polish this; M6 just exposes the truth.
    let shortcutEnabled = $state(true)
    let shortcutBinding = $state('\u{2303}\u{2325}\u{2318}J')
    let shortcutStatusText = $state('Registered')
    let fdaPending = $state(false)

    async function refreshShortcutStatus() {
        try {
            shortcutEnabled = getGlobalRevealEnabled()
            shortcutBinding = getGlobalRevealBinding()
        } catch (err) {
            log.warn('Failed to read global shortcut settings: {err}', { err: String(err) })
        }
        // Belt-and-braces re-check of the FDA gate so opening Settings recovers
        // from a stale focus-event read (e.g. user granted FDA, came straight here).
        const statusResult = await commands.downloadsWatcherStatus()
        if (statusResult.status === 'ok') {
            fdaPending = statusResult.data.fdaPending
        }
        await commands.recheckDownloadsWatcherGate()

        if (fdaPending) {
            shortcutStatusText = 'Cmdr needs Full Disk Access'
            return
        }
        // Ask the backend to apply the current setting and report the resulting status.
        const result = await commands.setGlobalRevealShortcut(shortcutEnabled, shortcutBinding)
        if (result.status === 'ok') {
            const s = result.data.status
            shortcutStatusText =
                s === 'registered'
                    ? 'Registered'
                    : s === 'conflict'
                      ? "Couldn't register: in use by another app"
                      : 'Not registered'
        } else {
            shortcutStatusText = "Couldn't register"
        }
    }

    async function handleShortcutEnabledChange(next: boolean) {
        setGlobalRevealEnabled(next)
        shortcutEnabled = next
        await refreshShortcutStatus()
    }

    async function handleBindingChange(next: string) {
        if (!next || next === shortcutBinding) return
        setGlobalRevealBinding(next)
        shortcutBinding = next
        await refreshShortcutStatus()
    }

    let dbFileSize = $state<number | null>(null)
    let clearing = $state(false)
    let clearError = $state<string | null>(null)
    let refreshTimer: ReturnType<typeof setInterval> | undefined

    async function refreshDbSize() {
        const res = await commands.getIndexStatus()
        if (res.status === 'ok') {
            dbFileSize = res.data.dbFileSize
        } else {
            dbFileSize = null
        }
    }

    async function handleClearIndex() {
        clearing = true
        clearError = null
        try {
            const res = await commands.clearDriveIndex()
            if (res.status === 'error') throw new Error(res.error)
            dbFileSize = null
            log.info('Drive index cleared from settings')
        } catch (error: unknown) {
            const msg = error instanceof Error ? error.message : String(error)
            clearError = msg
            log.error('Failed to clear drive index: {error}', { error: msg })
        } finally {
            clearing = false
        }
    }

    onMount(() => {
        void refreshDbSize()
        void refreshShortcutStatus()
        // Refresh DB size every 2 seconds while visible
        refreshTimer = setInterval(() => void refreshDbSize(), 2000)

        return () => {
            clearInterval(refreshTimer)
        }
    })
</script>

<SettingsSection title="Drive indexing">
    {#if shouldShow('indexing.enabled')}
        <SettingRow id="indexing.enabled" label={enabledDef.label} description={enabledDef.description} {searchQuery}>
            <SettingSwitch id="indexing.enabled" />
        </SettingRow>
    {/if}

    <div class="index-info">
        <div class="index-row">
            <span class="info-label">Index size</span>
            <div class="index-controls">
                {#if dbFileSize != null || clearing}
                    <Button
                        variant="secondary"
                        size="mini"
                        onclick={handleClearIndex}
                        disabled={clearing}
                    >
                        {clearing ? 'Clearing...' : 'Clear index'}
                    </Button>
                {/if}
                <span class="info-value">
                    {#if dbFileSize != null}
                        <Size bytes={dbFileSize} />
                    {:else}
                        No index
                    {/if}
                </span>
            </div>
        </div>

        <p class="clear-description">Deletes the index database. A fresh scan starts next time indexing is enabled.</p>

        {#if clearError}
            <div class="clear-error">{clearError}</div>
        {/if}
    </div>

    <!--
        M6 stub: global reveal-latest-download shortcut row. M7 will rename this
        section to "File system watching" and split into sub-groups; for now the
        controls live inline so settings persistence works end-to-end.
        Using a constrained text input for the binding because the existing
        shortcut recorder is designed for in-app combos and may misbehave with
        system-modifier capture (plan risk 3).
    -->
    {#if shouldShow(GLOBAL_REVEAL_ENABLED_KEY)}
        <SettingRow
            id={GLOBAL_REVEAL_ENABLED_KEY}
            label={globalShortcutDef.label}
            description={globalShortcutDef.description}
            {searchQuery}
        >
            <div class="shortcut-row">
                <label class="shortcut-label">
                    <input
                        type="checkbox"
                        checked={shortcutEnabled}
                        onchange={(e) => void handleShortcutEnabledChange((e.currentTarget as HTMLInputElement).checked)}
                        disabled={fdaPending}
                    />
                    <span>Enabled</span>
                </label>
                <label class="shortcut-label">
                    <span>Combo</span>
                    <input
                        class="shortcut-binding"
                        type="text"
                        value={shortcutBinding}
                        onchange={(e) => void handleBindingChange((e.currentTarget as HTMLInputElement).value)}
                        disabled={fdaPending || !shortcutEnabled}
                        spellcheck="false"
                    />
                </label>
                <span
                    class="shortcut-status"
                    class:warn={shortcutStatusText.toLowerCase().includes("couldn't")}>{shortcutStatusText}</span>
            </div>
        </SettingRow>
    {/if}
</SettingsSection>

<style>
    .index-info {
        padding: var(--spacing-sm) 0;
        border-bottom: 1px solid var(--color-border-subtle);
    }

    .index-row {
        display: grid;
        grid-template-columns: 1fr 1fr;
        align-items: center;
        gap: var(--spacing-md);
    }

    .info-label {
        font-weight: 500;
        color: var(--color-text-primary);
    }

    .index-controls {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--spacing-sm);
    }

    .info-value {
        color: var(--color-text-secondary);
        font-family: var(--font-mono);
        font-size: var(--font-size-sm);
    }

    .clear-description {
        margin: var(--spacing-xs) 0 0;
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        line-height: 1.4;
    }

    .clear-error {
        margin-top: var(--spacing-xs);
        padding: var(--spacing-xs) var(--spacing-sm);
        border-radius: var(--radius-sm);
        background: color-mix(in srgb, var(--color-error) 10%, transparent);
        color: var(--color-error-text);
        font-size: var(--font-size-sm);
    }

    .shortcut-row {
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        gap: var(--spacing-md);
        font-size: var(--font-size-sm);
    }

    .shortcut-label {
        display: inline-flex;
        align-items: center;
        gap: var(--spacing-xs);
        color: var(--color-text-secondary);
    }

    .shortcut-binding {
        font-family: var(--font-mono);
        padding: var(--spacing-xxs) var(--spacing-sm);
        border: 1px solid var(--color-border-subtle);
        border-radius: var(--radius-sm);
        background: var(--color-bg-tertiary);
        color: var(--color-text-primary);
        min-width: 80px;
    }

    .shortcut-status {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-xs);
    }

    .shortcut-status.warn {
        color: var(--color-warning-text);
    }
</style>
