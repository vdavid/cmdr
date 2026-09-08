<script lang="ts">
    /**
     * `File systems > Android (ADB)`: the switch, where Cmdr found `adb`, and the
     * two things a person does when it didn't find it (install the platform tools,
     * or point at their own copy).
     *
     * ❗ **Re-check runs ONE call per click**, ❌ never on mount and ❌ never on a
     * timer: it is the only path allowed to retry `adb start-server`, and a poll
     * would spawn processes behind the user's back. The status shown when the page
     * opens is `getAdbInstallStatus`, which reads what is already known.
     *
     * Both settings live-apply together through `adb-settings.ts`; nothing here
     * pushes to the backend itself.
     */
    import { onMount } from 'svelte'
    import { open } from '@tauri-apps/plugin-dialog'
    import SettingsSection from '../components/SettingsSection.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import Button from '$lib/ui/Button.svelte'
    import CopyBox from '$lib/ui/CopyBox.svelte'
    import TextInput from '$lib/ui/TextInput.svelte'
    import { tString } from '$lib/intl/messages.svelte'
    import { getSetting, getSettingDefinition, setSetting, onSpecificSettingChange } from '$lib/settings'
    import { createShouldShow, anyVisible } from '$lib/settings/settings-search'
    import { getAdbInstallStatus, recheckAdbInstall } from '$lib/tauri-commands'
    import { pushAdbConfigToBackend } from '$lib/adb/adb-settings'
    import type { AdbInstallStatus } from '$lib/ipc/bindings'
    import { getAppLogger } from '$lib/logging/logger'

    /** What to install when `adb` is nowhere on this Mac. */
    const INSTALL_COMMAND = 'brew install android-platform-tools'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    const log = getAppLogger('settings')

    const shouldShow = $derived(createShouldShow(searchQuery))

    const defaultDef = { label: '', description: '' }
    const enabledDef = getSettingDefinition('fileOperations.adbEnabled') ?? defaultDef
    const binaryPathDef = getSettingDefinition('fileOperations.adbBinaryPath') ?? defaultDef

    let status = $state<AdbInstallStatus | null>(null)
    let rechecking = $state(false)
    let binaryPath = $state(getSetting('fileOperations.adbBinaryPath'))

    onMount(() => {
        void refreshStatus()
        // An external reset (or the other window) writing the path has to reach the
        // field, the way every registry-bound control follows the store.
        return onSpecificSettingChange('fileOperations.adbBinaryPath', (next) => {
            binaryPath = next
        })
    })

    async function refreshStatus(): Promise<void> {
        try {
            status = await getAdbInstallStatus()
        } catch (error) {
            log.warn('Could not read the ADB install status: {error}', { error: String(error) })
        }
    }

    async function handleRecheck(): Promise<void> {
        if (rechecking) return
        rechecking = true
        try {
            status = await recheckAdbInstall()
        } catch (error) {
            log.warn('Could not re-check for adb: {error}', { error: String(error) })
        } finally {
            rechecking = false
        }
    }

    function commitPath(next: string): void {
        binaryPath = next
        setSetting('fileOperations.adbBinaryPath', next)
    }

    async function handleBrowse(): Promise<void> {
        try {
            const picked = await open({ multiple: false, directory: false, title: tString('settings.adb.pickerTitle') })
            if (typeof picked !== 'string') return
            commitPath(picked)
            // ❗ AWAITED, and before the re-check. `setSetting`'s applier fires
            // its own `pushAdbConfigToBackend()` unawaited, and Tauri runs the
            // push and the re-check as independent tasks, so a re-check started
            // beside it can reach `recheck_adb_install` first and report on the
            // OLD binary. The push re-reads both settings fresh, so running it
            // twice costs a round-trip and changes nothing.
            await pushAdbConfigToBackend()
            // ❗ A re-check, ❌ not a status read: the path only takes effect once
            // the tracker restarts under it, so `getAdbInstallStatus` here would
            // answer about the OLD binary. Choosing a file is a person saying "look
            // here now", which is exactly the human action `recheckAdbInstall` is
            // budgeted per.
            await handleRecheck()
        } catch (error) {
            log.warn('Could not open the adb file picker: {error}', { error: String(error) })
        }
    }
</script>

<SettingsSection title={tString('settings.section.adb')}>
    {#if anyVisible(shouldShow, 'fileOperations.adbEnabled', 'row:adb.status', 'fileOperations.adbBinaryPath')}
        <SectionCard>
            {#if shouldShow('fileOperations.adbEnabled')}
                <SettingRow
                    id="fileOperations.adbEnabled"
                    label={enabledDef.label}
                    description={enabledDef.description}
                    {searchQuery}
                >
                    <SettingSwitch id="fileOperations.adbEnabled" />
                </SettingRow>
            {/if}

            <!-- Its OWN gate, not the switch's: the status block is a searchable row
                 of its own, so a hit on "re-check" must render it even when the
                 switch above filtered out. -->
            {#if shouldShow('row:adb.status')}
                <div class="status-row">
                    <span class="status-label">{tString('settings.adb.status.label')}</span>
                    <div class="status-body">
                        <span class="status-value">
                            {#if status?.binaryPath}
                                {tString('settings.adb.status.foundAt', { path: status.binaryPath })}
                            {:else}
                                {tString('settings.adb.status.notFound')}
                            {/if}
                        </span>
                        <Button variant="secondary" size="mini" disabled={rechecking} onclick={() => void handleRecheck()}>
                            {tString('settings.adb.recheck')}
                        </Button>
                    </div>
                </div>
                {#if status !== null}
                    <p class="status-note">
                        {status.tracking
                            ? tString('settings.adb.status.watching')
                            : tString('settings.adb.status.notWatching')}
                    </p>
                {/if}

                {#if status !== null && !status.binaryPath}
                    <div class="install">
                        <p class="install-intro">{tString('settings.adb.install.intro')}</p>
                        <CopyBox text={INSTALL_COMMAND} />
                    </div>
                {/if}
            {/if}

            {#if shouldShow('fileOperations.adbBinaryPath')}
                <SettingRow
                    id="fileOperations.adbBinaryPath"
                    label={binaryPathDef.label}
                    description={binaryPathDef.description}
                    split
                    {searchQuery}
                >
                    <div class="path-field">
                        <TextInput
                            value={binaryPath}
                            placeholder={tString('settings.adb.pathPlaceholder')}
                            oninput={(event: Event & { currentTarget: HTMLInputElement }) => {
                                binaryPath = event.currentTarget.value
                            }}
                            onblur={() => {
                                commitPath(binaryPath)
                            }}
                            ariaLabel={binaryPathDef.label}
                        />
                        <Button variant="secondary" size="mini" onclick={() => void handleBrowse()}>
                            {tString('settings.adb.browse')}
                        </Button>
                    </div>
                </SettingRow>
            {/if}
        </SectionCard>
    {/if}
</SettingsSection>

<style>
    .status-row {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: var(--spacing-md);
        padding: var(--spacing-sm) 0;
        border-bottom: 1px solid var(--color-border-subtle);
    }

    .status-label {
        color: var(--color-text-primary);
    }

    .status-body {
        display: flex;
        min-width: 0;
        align-items: center;
        gap: var(--spacing-sm);
    }

    .status-value {
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
        overflow-wrap: anywhere;
    }

    .status-note {
        margin: var(--spacing-xs) 0 0;
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
    }

    .install {
        margin: var(--spacing-sm) 0;
    }

    .install-intro {
        margin: 0 0 var(--spacing-xs);
        color: var(--color-text-secondary);
        font-size: var(--font-size-sm);
    }

    .path-field {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
    }
</style>
