<script lang="ts">
    /**
     * `Behavior > Notifications` covers the two notification-shaped watchers: the
     * downloads watcher and the low-disk-space warning. The downloads watcher
     * shares the Full Disk Access gate, so a single hint surfaces when the gate is
     * closed.
     *
     * The section renders two card groups inside `SectionCard`s:
     *
     *   1. **Downloads** — both Downloads-folder features in one card: the
     *      4-option `downloadsNotifications` ToggleGroup, and the on/off
     *      `Switch` for the global go-to-latest hotkey (whose description
     *      references the LIVE binding, so it updates the moment the user
     *      rebinds in `Keyboard shortcuts`, where the combo is actually
     *      edited). Greyed out when the FDA gate is closed. Carries a stable
     *      anchor id so the downloads-toast "Stop showing these" button can
     *      deep-link here.
     *   2. **Low disk space** — the 3-option ToggleGroup driving
     *      `behavior.fileSystemWatching.lowDiskSpaceNotifications` plus the
     *      percent-threshold number input. NOT FDA-gated: the backend's space
     *      poller reads `statfs`, which needs no TCC permission. Carries a
     *      stable anchor id so the warn toast's "Disable these notifications"
     *      button can deep-link here.
     *
     * (The drive-file-system indexer lives in `DriveIndexingSection.svelte` under
     * the top-level Indexing section.)
     *
     * Card visibility under search is section-owned, not registry-derived: each
     * `SectionCard` frame is wrapped in `{#if anyVisible(shouldShow, ...ids)}`
     * over its member setting ids, the SAME `shouldShow` predicate that gates
     * each row inside, so a card whose rows all filter out hides its frame too
     * (no empty cards). The Downloads card dims via `SectionCard`'s `gated`
     * prop and carries the single FDA hint, per the "Locked copy" decision.
     */
    import { onMount } from 'svelte'
    import { Switch } from '@ark-ui/svelte/switch'
    import SettingsSection from '../components/SettingsSection.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingToggleGroup from '../components/SettingToggleGroup.svelte'
    import SettingNumberInput from '../components/SettingNumberInput.svelte'
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import LinkButton from '$lib/ui/LinkButton.svelte'
    import Trans from '$lib/intl/Trans.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { getSettingDefinition, onSpecificSettingChange } from '$lib/settings'
    import { createShouldShow, anyVisible } from '$lib/settings/settings-search'
    import { getAppLogger } from '$lib/logging/logger'
    import {
        downloadsWatcherStatus,
        openPrivacySettings,
        recheckDownloadsWatcherGate,
        setGlobalGoToLatestShortcut,
    } from '$lib/tauri-commands'
    import {
        getGlobalGoToLatestEnabled,
        getGlobalGoToLatestBinding,
        setGlobalGoToLatestEnabled,
        GLOBAL_GO_TO_LATEST_ENABLED_KEY,
        GLOBAL_GO_TO_LATEST_BINDING_KEY,
    } from '$lib/downloads/global-shortcut-setting'
    import { globalGoToLatestDescription } from '$lib/downloads/global-shortcut-description'
    import {
        DOWNLOADS_NOTIFICATIONS_SETTING_KEY,
        DOWNLOADS_NOTIFICATIONS_ANCHOR_ID,
    } from '$lib/downloads/notifications-mode'
    import {
        LOW_DISK_SPACE_NOTIFICATIONS_SETTING_KEY,
        LOW_DISK_SPACE_THRESHOLD_SETTING_KEY,
        LOW_DISK_SPACE_ANCHOR_ID,
        getLowDiskSpaceNotificationsMode,
    } from '$lib/low-disk-space/notifications-mode'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    const log = getAppLogger('settings')

    const shouldShow = $derived(createShouldShow(searchQuery))

    const notificationsDef = getSettingDefinition(DOWNLOADS_NOTIFICATIONS_SETTING_KEY) ?? {
        label: '',
        description: '',
    }
    const globalShortcutDef = getSettingDefinition(GLOBAL_GO_TO_LATEST_ENABLED_KEY) ?? {
        label: '',
        description: '',
    }
    const lowDiskSpaceDef = getSettingDefinition(LOW_DISK_SPACE_NOTIFICATIONS_SETTING_KEY) ?? {
        label: '',
        description: '',
    }
    const lowDiskSpaceThresholdDef = getSettingDefinition(LOW_DISK_SPACE_THRESHOLD_SETTING_KEY) ?? {
        label: '',
        description: '',
    }

    // Tracked so the threshold input greys out while the warning is off.
    let lowDiskSpaceMode = $state(getLowDiskSpaceNotificationsMode())

    // The on/off state lives here; the binding is edited in `Keyboard
    // shortcuts`. We track the binding only so this toggle's description can
    // reference the live combo, updating the moment the user rebinds there.
    let shortcutEnabled = $state(true)
    let shortcutBinding = $state('\u{2303}\u{2325}\u{2318}J')
    let fdaPending = $state(false)
    /** Watcher dormant. FDA-closed is the common cause; either way we surface the same hint. */
    let watcherRunning = $state(true)

    /** Sub-groups grey out when the FDA gate is closed or the watcher is dormant. */
    const downloadsGated = $derived(fdaPending || !watcherRunning)

    /** Description references the live binding, so a rebind elsewhere updates the helper text. */
    const shortcutDescription = $derived(globalGoToLatestDescription(shortcutBinding))

    async function refreshShortcutStatus() {
        try {
            shortcutEnabled = getGlobalGoToLatestEnabled()
            shortcutBinding = getGlobalGoToLatestBinding()
        } catch (err) {
            log.warn('Failed to read global shortcut settings: {err}', { err: String(err) })
        }
        // Belt-and-braces re-check of the FDA gate so opening Settings recovers
        // from a stale focus-event read (e.g. user granted FDA, came straight here).
        const statusResult = await downloadsWatcherStatus()
        if (statusResult.status === 'ok') {
            fdaPending = statusResult.data.fdaPending
            watcherRunning = statusResult.data.running
        }
        const recheck = await recheckDownloadsWatcherGate()
        if (recheck.status === 'error') {
            log.warn('recheckDownloadsWatcherGate failed: {message}', {
                message: recheck.error.message,
            })
        }
    }

    async function applyShortcut() {
        if (fdaPending) return
        // Ask the backend to apply the current enabled/binding to the live
        // registration. The returned status drives nothing in this row anymore
        // (the binding + its registration feedback live in `Keyboard
        // shortcuts`); we just keep the live-apply contract on the toggle.
        const result = await setGlobalGoToLatestShortcut(shortcutEnabled, shortcutBinding)
        if (result.status === 'error') {
            log.warn('setGlobalGoToLatestShortcut failed: {error}', { error: JSON.stringify(result.error) })
        }
    }

    async function handleShortcutEnabledChange(next: boolean) {
        setGlobalGoToLatestEnabled(next)
        shortcutEnabled = next
        await applyShortcut()
    }

    async function handleOpenSystemSettings() {
        try {
            await openPrivacySettings()
        } catch (err) {
            log.warn('Failed to open System Settings: {err}', { err: String(err) })
        }
    }

    onMount(() => {
        void (async () => {
            await refreshShortcutStatus()
            await applyShortcut()
        })()
        // Keep the description in sync when the binding is rebound in the
        // Keyboard shortcuts section (same window or another), and keep the
        // toggle state in sync if `enabled` changes elsewhere.
        const unsubBinding = onSpecificSettingChange(GLOBAL_GO_TO_LATEST_BINDING_KEY, (_id, value) => {
            shortcutBinding = value
        })
        const unsubEnabled = onSpecificSettingChange(GLOBAL_GO_TO_LATEST_ENABLED_KEY, (_id, value) => {
            shortcutEnabled = value
        })
        const unsubLowDiskSpace = onSpecificSettingChange(LOW_DISK_SPACE_NOTIFICATIONS_SETTING_KEY, () => {
            lowDiskSpaceMode = getLowDiskSpaceNotificationsMode()
        })

        return () => {
            unsubBinding()
            unsubEnabled()
            unsubLowDiskSpace()
        }
    })
</script>

{#snippet settingsLink(children: import('svelte').Snippet)}
    <LinkButton onclick={handleOpenSystemSettings}>{@render children()}</LinkButton>
{/snippet}

<SettingsSection title={tString('settings.section.notifications')}>
    {#if downloadsGated && anyVisible(shouldShow, DOWNLOADS_NOTIFICATIONS_SETTING_KEY, GLOBAL_GO_TO_LATEST_ENABLED_KEY)}
        <p class="fda-hint">
            <Trans key="common.downloadsFdaHint" snippets={{ settingsLink }} />
        </p>
    {/if}

    {#if anyVisible(shouldShow, DOWNLOADS_NOTIFICATIONS_SETTING_KEY, GLOBAL_GO_TO_LATEST_ENABLED_KEY)}
        <SectionCard
            id={DOWNLOADS_NOTIFICATIONS_ANCHOR_ID}
            label={tString('settings.fileSystemWatching.cardDownloads')}
            gated={downloadsGated}
        >
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
            {#if shouldShow(GLOBAL_GO_TO_LATEST_ENABLED_KEY)}
                <SettingRow
                    id={GLOBAL_GO_TO_LATEST_ENABLED_KEY}
                    label={globalShortcutDef.label}
                    description={shortcutDescription}
                    {searchQuery}
                >
                    <Switch.Root
                        checked={shortcutEnabled}
                        onCheckedChange={(details) => void handleShortcutEnabledChange(details.checked)}
                        disabled={downloadsGated}
                        aria-label={globalShortcutDef.label}
                    >
                        <Switch.Control class="go-to-latest-switch-control">
                            <Switch.Thumb class="go-to-latest-switch-thumb" />
                        </Switch.Control>
                        <Switch.HiddenInput data-test="global-shortcut-enabled" />
                    </Switch.Root>
                </SettingRow>
                <p class="shortcut-hint">
                    {tString('settings.fileSystemWatching.globalShortcutHint')}
                </p>
            {/if}
        </SectionCard>
    {/if}

    {#if anyVisible(shouldShow, LOW_DISK_SPACE_NOTIFICATIONS_SETTING_KEY, LOW_DISK_SPACE_THRESHOLD_SETTING_KEY)}
        <SectionCard id={LOW_DISK_SPACE_ANCHOR_ID} label={tString('settings.fileSystemWatching.cardLowDiskSpace')}>
            {#if shouldShow(LOW_DISK_SPACE_NOTIFICATIONS_SETTING_KEY)}
                <SettingRow
                    id={LOW_DISK_SPACE_NOTIFICATIONS_SETTING_KEY}
                    label={lowDiskSpaceDef.label}
                    description={lowDiskSpaceDef.description}
                    {searchQuery}
                >
                    <SettingToggleGroup id={LOW_DISK_SPACE_NOTIFICATIONS_SETTING_KEY} />
                </SettingRow>
            {/if}
            {#if shouldShow(LOW_DISK_SPACE_THRESHOLD_SETTING_KEY)}
                <SettingRow
                    id={LOW_DISK_SPACE_THRESHOLD_SETTING_KEY}
                    label={lowDiskSpaceThresholdDef.label}
                    description={lowDiskSpaceThresholdDef.description}
                    split
                    {searchQuery}
                >
                    <SettingNumberInput
                        id={LOW_DISK_SPACE_THRESHOLD_SETTING_KEY}
                        disabled={lowDiskSpaceMode === 'off'}
                        unit="%"
                    />
                </SettingRow>
            {/if}
        </SectionCard>
    {/if}
</SettingsSection>

<style>
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

    /* The gated Downloads card dims via `SectionCard`'s `gated` prop (it owns the
       `[data-gated='true'] .section-card { opacity: .5 }` rule). Inner controls
       own their own `disabled` state (toggle group + switch both pass
       `downloadsGated` through), so the card only owns the visual cue. */

    .shortcut-hint {
        margin: var(--spacing-xs) 0 0;
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        line-height: 1.4;
    }

    /* Ark UI Switch used inline here (not the registry `SettingSwitch`, because
       the toggle's live-apply runs a custom IPC handler rather than a plain
       `setSetting`). Styling mirrors `SettingSwitch.svelte`; class names are
       local to keep the rules scoped to this component. */
    :global(.go-to-latest-switch-control) {
        display: inline-flex;
        align-items: center;
        width: 36px;
        height: 20px;
        background: var(--color-bg-tertiary);
        border-radius: var(--radius-full);
        padding: var(--spacing-xxs);
        cursor: default;
        transition: background-color var(--transition-base);
    }

    :global(.go-to-latest-switch-control[data-state='checked']) {
        background: var(--color-accent);
    }

    :global(.go-to-latest-switch-control[data-disabled]) {
        cursor: not-allowed;
        opacity: 0.5;
    }

    :global(.go-to-latest-switch-thumb) {
        width: 16px;
        height: 16px;
        background: white;
        border-radius: var(--radius-full);
        transition: transform var(--transition-base);
        box-shadow: var(--shadow-sm);
    }

    :global(.go-to-latest-switch-control[data-state='checked'] .go-to-latest-switch-thumb) {
        transform: translateX(16px);
    }

    :global(.go-to-latest-switch-control[data-state='checked']:hover) {
        background: var(--color-accent-hover);
    }

    :global(.go-to-latest-switch-control[data-focus]) {
        outline: 2px solid var(--color-accent);
        outline-offset: 2px;
        box-shadow: var(--shadow-focus);
    }
</style>
