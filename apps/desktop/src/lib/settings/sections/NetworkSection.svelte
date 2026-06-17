<script lang="ts">
    import SettingsSection from '../components/SettingsSection.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import SettingSelect from '../components/SettingSelect.svelte'
    import SettingRadioGroup from '../components/SettingRadioGroup.svelte'
    import SettingNumberInput from '../components/SettingNumberInput.svelte'
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import { getSettingDefinition } from '$lib/settings'
    import { createShouldShow, anyVisible } from '$lib/settings/settings-search'
    import { openSystemSettingsUrl } from '$lib/tauri-commands'
    import { systemStrings } from '$lib/system-strings.svelte'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    const shouldShow = $derived(createShouldShow(searchQuery))

    const defaultDef = { label: '', description: '' }
    const networkEnabledDef = getSettingDefinition('network.enabled') ?? defaultDef
    const directSmbDef = getSettingDefinition('network.directSmbConnection') ?? defaultDef
    const cacheDurationDef = getSettingDefinition('network.shareCacheDuration') ?? defaultDef
    const timeoutModeDef = getSettingDefinition('network.timeoutMode') ?? defaultDef

    function handlePrivacyLinkClick(event: MouseEvent) {
        event.preventDefault()
        void openSystemSettingsUrl('x-apple.systempreferences:com.apple.preference.security?Privacy_LocalNetwork')
    }

    // allowed-pluralize-noun: "access" is a singular noun, not a count-driven plural; the interpolation is the localized pane label, not a count.
    const localNetworkAccessLabel = $derived(tString('settings.network.localNetworkAccessLabel', { localNetwork: systemStrings.localNetwork }))
</script>

<SettingsSection title={tString('settings.section.smbNetworkShares')}>
    {#if anyVisible(shouldShow, 'network.enabled', 'network.directSmbConnection')}
        <SectionCard label={tString('settings.network.card.connection')}>
            {#if shouldShow('network.enabled')}
                <SettingRow
                    id="network.enabled"
                    label={networkEnabledDef.label}
                    description={networkEnabledDef.description}
                    {searchQuery}
                >
                    <SettingSwitch id="network.enabled" />
                </SettingRow>

                <div class="local-network-info">
                    <h4>{localNetworkAccessLabel}</h4>
                    <p>
                        {tString('settings.network.permissionIntroPrefix', {
                            localNetwork: systemStrings.localNetwork,
                        })}<strong>{tString('settings.network.permissionIntroConnectLink')}</strong
                        >{tString('settings.network.permissionIntroSuffix')}
                    </p>
                    <p>
                        {tString('settings.network.manageAnytimePrefix')}
                        <button type="button" class="link-button" onclick={handlePrivacyLinkClick}
                            >{tString('settings.network.permissionPath', {
                                systemSettings: systemStrings.systemSettings,
                                privacyAndSecurity: systemStrings.privacyAndSecurity,
                                localNetwork: systemStrings.localNetwork,
                            })}</button
                        >.
                    </p>
                    <p class="muted">
                        {tString('settings.network.permissionWithout')}
                    </p>
                </div>
            {/if}

            {#if shouldShow('network.directSmbConnection')}
                <SettingRow
                    id="network.directSmbConnection"
                    label={directSmbDef.label}
                    description={directSmbDef.description}
                    {searchQuery}
                >
                    <SettingSwitch id="network.directSmbConnection" />
                </SettingRow>
            {/if}
        </SectionCard>
    {/if}

    {#if anyVisible(shouldShow, 'network.shareCacheDuration', 'network.timeoutMode')}
        <SectionCard label={tString('settings.network.card.performanceAndTimeouts')}>
            {#if shouldShow('network.shareCacheDuration')}
                <SettingRow
                    id="network.shareCacheDuration"
                    label={cacheDurationDef.label}
                    description={cacheDurationDef.description}
                    split
                    {searchQuery}
                >
                    <SettingSelect id="network.shareCacheDuration" />
                </SettingRow>
            {/if}

            {#if shouldShow('network.timeoutMode')}
                <SettingRow
                    id="network.timeoutMode"
                    label={timeoutModeDef.label}
                    description={timeoutModeDef.description}
                    split
                    {searchQuery}
                >
                    <div class="timeout-setting">
                        <SettingRadioGroup id="network.timeoutMode">
                            {#snippet customContent(value)}
                                {#if value === 'custom'}
                                    <div class="custom-timeout">
                                        <SettingNumberInput
                                            id="network.customTimeout"
                                            unit={tString('settings.network.customTimeoutUnit')}
                                        />
                                    </div>
                                {/if}
                            {/snippet}
                        </SettingRadioGroup>
                    </div>
                </SettingRow>
            {/if}
        </SectionCard>
    {/if}
</SettingsSection>

<style>
    .timeout-setting {
        width: 100%;
        min-width: 0;
    }

    .custom-timeout {
        margin-top: var(--spacing-xs);
    }

    .local-network-info {
        padding: var(--spacing-sm) var(--spacing-md);
        margin: var(--spacing-xs) 0 var(--spacing-md);
        background: var(--color-bg-secondary);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-md);
        font-size: var(--font-size-sm);
        line-height: 1.5;
    }

    .local-network-info h4 {
        margin: 0 0 var(--spacing-xs);
        font-size: var(--font-size-sm);
        font-weight: 600;
    }

    .local-network-info p {
        margin: 0 0 var(--spacing-xs);
    }

    .local-network-info p:last-child {
        margin-bottom: 0;
    }

    .local-network-info .muted {
        color: var(--color-text-secondary);
    }

    .local-network-info .link-button {
        padding: 0;
        margin: 0;
        background: none;
        border: none;
        font: inherit;
        color: var(--color-accent-text);
        text-decoration: underline;
    }

    .local-network-info .link-button:hover {
        text-decoration: none;
    }

    .local-network-info .link-button:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 2px;
        border-radius: var(--radius-sm);
    }
</style>
