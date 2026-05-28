<script lang="ts">
    /**
     * `File system watching` is the umbrella section for both the file-system
     * indexer and the downloads watcher. Both depend on the same FDA gate, so
     * we group them and surface a single hint when the gate is closed.
     *
     * The section renders three sub-groups inside `SectionCard`s:
     *
     *   1. **Drive indexing** — the existing `indexing.enabled` toggle plus
     *      the clear-index action. Stays interactive even when the FDA gate
     *      is closed (indexing operates on whatever paths it has access to;
     *      the gate is for the downloads watcher).
     *   2. **Downloads notifications** — the 4-option ToggleGroup driving
     *      `behavior.fileSystemWatching.downloadsNotifications`. Greyed out
     *      when the FDA gate is closed. Carries a stable anchor id so the
     *      M5 "Stop showing these" toast button can deep-link here.
     *   3. **Reveal latest download** — the global hotkey on/off toggle,
     *      the binding picker (v1 text input; recorder follow-up tracked in
     *      `docs/specs/downloads-watcher-plan.md`), and the registration-
     *      status indicator. Greyed out when the FDA gate is closed.
     *
     * Sub-groups 2 and 3 share ONE FDA hint, not one per sub-group, per the
     * plan's "Locked copy" decision.
     */
    import { commands } from '$lib/ipc/bindings'
    import { onMount } from 'svelte'
    import SettingsSection from '../components/SettingsSection.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import SettingToggleGroup from '../components/SettingToggleGroup.svelte'
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import Button from '$lib/ui/Button.svelte'
    import LinkButton from '$lib/ui/LinkButton.svelte'
    import { getSettingDefinition } from '$lib/settings'
    import { createShouldShow } from '$lib/settings/settings-search'
    import Size from '$lib/ui/Size.svelte'
    import { getAppLogger } from '$lib/logging/logger'
    import { openPrivacySettings } from '$lib/tauri-commands'
    import {
        getGlobalRevealEnabled,
        getGlobalRevealBinding,
        setGlobalRevealEnabled,
        setGlobalRevealBinding,
        GLOBAL_REVEAL_ENABLED_KEY,
    } from '$lib/downloads/global-shortcut-setting'
    import {
        DOWNLOADS_NOTIFICATIONS_SETTING_KEY,
        DOWNLOADS_NOTIFICATIONS_ANCHOR_ID,
    } from '$lib/downloads/notifications-mode'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    const log = getAppLogger('settings')

    const shouldShow = $derived(createShouldShow(searchQuery))

    const enabledDef = getSettingDefinition('indexing.enabled') ?? { label: '', description: '' }
    const notificationsDef = getSettingDefinition(DOWNLOADS_NOTIFICATIONS_SETTING_KEY) ?? {
        label: '',
        description: '',
    }
    const globalShortcutDef = getSettingDefinition(GLOBAL_REVEAL_ENABLED_KEY) ?? {
        label: '',
        description: '',
    }

    // Inline status indicator for the global-shortcut row. Updated via the
    // backend's `set_global_reveal_shortcut` ack on every flip plus a mount-time
    // refresh.
    let shortcutEnabled = $state(true)
    let shortcutBinding = $state('\u{2303}\u{2325}\u{2318}J')
    let shortcutStatusText = $state('Registered')
    let fdaPending = $state(false)
    /** Watcher dormant. FDA-closed is the common cause; either way we surface the same hint. */
    let watcherRunning = $state(true)

    /** Sub-groups 2 + 3 grey out when the FDA gate is closed or the watcher is dormant. */
    const downloadsGated = $derived(fdaPending || !watcherRunning)

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
            watcherRunning = statusResult.data.running
        }
        const recheck = await commands.recheckDownloadsWatcherGate()
        if (recheck.status === 'error') {
            log.warn('recheckDownloadsWatcherGate failed: {message}', {
                message: recheck.error.message,
            })
        }

        if (fdaPending) {
            shortcutStatusText = 'Cmdr needs Full Disk Access'
            return
        }
        // Ask the backend to apply the current setting and report the resulting status.
        const result = await commands.setGlobalRevealShortcut(shortcutEnabled, shortcutBinding)
        if (result.status === 'ok') {
            shortcutStatusText = result.data.status === 'registered' ? 'Registered' : 'Not registered'
        } else {
            // Two error kinds: `invalidBinding` (typo in the combo) and
            // `pluginError` (conflict with another app, allocation, OS IO,
            // …). Render the underlying message tail when present so the
            // user sees something actionable.
            if (result.error.kind === 'invalidBinding') {
                shortcutStatusText = `Couldn't register: invalid combo`
            } else {
                shortcutStatusText = `Couldn't register: ${result.error.message}`
            }
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

    async function handleOpenSystemSettings() {
        try {
            await openPrivacySettings()
        } catch (err) {
            log.warn('Failed to open System Settings: {err}', { err: String(err) })
        }
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

<SettingsSection title="File system watching">
    <SectionCard label="Drive indexing">
        {#if shouldShow('indexing.enabled')}
            <SettingRow
                id="indexing.enabled"
                label={enabledDef.label}
                description={enabledDef.description}
                {searchQuery}
            >
                <SettingSwitch id="indexing.enabled" />
            </SettingRow>
        {/if}

        <div class="index-info">
            <div class="index-row">
                <span class="info-label">Index size</span>
                <div class="index-controls">
                    {#if dbFileSize != null || clearing}
                        <Button variant="secondary" size="mini" onclick={handleClearIndex} disabled={clearing}>
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

            <p class="clear-description">
                Deletes the index database. A fresh scan starts next time indexing is enabled.
            </p>

            {#if clearError}
                <div class="clear-error">{clearError}</div>
            {/if}
        </div>
    </SectionCard>

    {#if downloadsGated}
        <p class="fda-hint">
            Cmdr needs Full Disk Access to watch your Downloads folder.
            <LinkButton onclick={handleOpenSystemSettings}>Open System Settings</LinkButton>
        </p>
    {/if}

    <div id={DOWNLOADS_NOTIFICATIONS_ANCHOR_ID} data-gated={downloadsGated ? 'true' : 'false'}>
        <SectionCard label="Downloads notifications">
            {#if shouldShow(DOWNLOADS_NOTIFICATIONS_SETTING_KEY)}
                <SettingRow
                    id={DOWNLOADS_NOTIFICATIONS_SETTING_KEY}
                    label={notificationsDef.label}
                    description={notificationsDef.description}
                    {searchQuery}
                >
                    <SettingToggleGroup id={DOWNLOADS_NOTIFICATIONS_SETTING_KEY} disabled={downloadsGated} />
                </SettingRow>
            {/if}
        </SectionCard>
    </div>

    <div data-gated={downloadsGated ? 'true' : 'false'}>
        <SectionCard label="Reveal latest download">
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
                                data-test="global-shortcut-enabled"
                                type="checkbox"
                                checked={shortcutEnabled}
                                onchange={(e) =>
                                    void handleShortcutEnabledChange(
                                        (e.currentTarget).checked,
                                    )}
                                disabled={downloadsGated}
                            />
                            <span>Global shortcut</span>
                        </label>
                        <label class="shortcut-label">
                            <span>Combo</span>
                            <input
                                class="shortcut-binding"
                                type="text"
                                value={shortcutBinding}
                                onchange={(e) =>
                                    void handleBindingChange((e.currentTarget).value)}
                                disabled={downloadsGated || !shortcutEnabled}
                                spellcheck="false"
                            />
                        </label>
                        <span
                            class="shortcut-status"
                            class:warn={shortcutStatusText.toLowerCase().includes("couldn't")}
                            >{shortcutStatusText}</span
                        >
                    </div>
                </SettingRow>
            {/if}
        </SectionCard>
    </div>
</SettingsSection>

<style>
    .index-info {
        padding: var(--spacing-sm) 0;
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

    .fda-hint {
        margin: 0 0 var(--spacing-md);
        padding: var(--spacing-sm) var(--spacing-md);
        border-radius: var(--radius-md);
        background: var(--color-bg-tertiary);
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        line-height: 1.4;
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        gap: var(--spacing-sm);
    }

    /* The two gated sub-groups dim their content and fade interaction affordances.
       Inner controls own their own `disabled` state (the toggle group, checkbox,
       and text input all pass `downloadsGated` through), so the wrapper only
       owns the visual cue. */
    [data-gated='true'] :global(.section-card) {
        opacity: 0.5;
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
