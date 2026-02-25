<script lang="ts">
    import { onMount } from 'svelte'
    import SettingsSection from '../components/SettingsSection.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import SettingSelect from '../components/SettingSelect.svelte'
    import SettingToggleGroup from '../components/SettingToggleGroup.svelte'
    import SettingRadioGroup from '../components/SettingRadioGroup.svelte'
    import { getSettingDefinition, getSetting, setSetting, onSpecificSettingChange } from '$lib/settings'
    import { createShouldShow } from '$lib/settings/settings-search'
    import { openAppearanceSettings } from '$lib/tauri-commands'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    const shouldShow = $derived(createShouldShow(searchQuery))

    // Get definitions for rendering (with fallbacks for type safety)
    const appColorDef = getSettingDefinition('appearance.appColor') ?? { label: '', description: '' }
    const uiDensityDef = getSettingDefinition('appearance.uiDensity') ?? { label: '', description: '' }
    const appIconsDef = getSettingDefinition('appearance.useAppIconsForDocuments') ?? { label: '', description: '' }
    const fileSizeDef = getSettingDefinition('appearance.fileSizeFormat') ?? { label: '', description: '' }
    const dateTimeDef = getSettingDefinition('appearance.dateTimeFormat') ?? { label: '', description: '' }

    // App color state
    let appColorValue = $state(getSetting('appearance.appColor'))

    onMount(() => {
        return onSpecificSettingChange('appearance.appColor', (_id, newValue) => {
            appColorValue = newValue
        })
    })

    // Custom date format state
    let customFormat = $state(getSetting('appearance.customDateTimeFormat'))
    let showFormatHelp = $state(false)

    // Live preview of date format
    function formatPreview(format: string): string {
        const now = new Date()
        try {
            return format
                .replace('YYYY', String(now.getFullYear()))
                .replace('MM', String(now.getMonth() + 1).padStart(2, '0'))
                .replace('DD', String(now.getDate()).padStart(2, '0'))
                .replace('HH', String(now.getHours()).padStart(2, '0'))
                .replace('mm', String(now.getMinutes()).padStart(2, '0'))
                .replace('ss', String(now.getSeconds()).padStart(2, '0'))
        } catch {
            return 'Invalid format'
        }
    }

    function handleCustomFormatChange(event: Event) {
        const target = event.target as HTMLInputElement
        customFormat = target.value
        setSetting('appearance.customDateTimeFormat', target.value)
    }
</script>

<SettingsSection title="Appearance">
    {#if shouldShow('appearance.appColor')}
        <SettingRow id="appearance.appColor" label={appColorDef.label} description="" {searchQuery}>
            <div class="app-color-options">
                <label class="app-color-option">
                    <input
                        type="radio"
                        name="appColor"
                        value="system"
                        checked={appColorValue === 'system'}
                        onchange={() => {
                            appColorValue = 'system'
                            setSetting('appearance.appColor', 'system')
                        }}
                    />
                    <span class="color-swatch system-swatch"></span>
                    <span class="app-color-label">System theme color</span>
                </label>
                <label class="app-color-option">
                    <input
                        type="radio"
                        name="appColor"
                        value="cmdr-gold"
                        checked={appColorValue === 'cmdr-gold'}
                        onchange={() => {
                            appColorValue = 'cmdr-gold'
                            setSetting('appearance.appColor', 'cmdr-gold')
                        }}
                    />
                    <span class="color-swatch gold-swatch"></span>
                    <span class="app-color-label">Cmdr gold</span>
                </label>
            </div>
        </SettingRow>
        <p class="app-color-description">
            To change your system theme color, go to
            <button type="button" class="appearance-link" onclick={() => void openAppearanceSettings()}
                >System Settings &gt; Appearance</button
            >.
        </p>
    {/if}

    {#if shouldShow('appearance.uiDensity')}
        <SettingRow
            id="appearance.uiDensity"
            label={uiDensityDef.label}
            description={uiDensityDef.description}
            {searchQuery}
        >
            <SettingToggleGroup id="appearance.uiDensity" />
        </SettingRow>
    {/if}

    {#if shouldShow('appearance.useAppIconsForDocuments')}
        <SettingRow
            id="appearance.useAppIconsForDocuments"
            label={appIconsDef.label}
            description={appIconsDef.description}
            {searchQuery}
        >
            <SettingSwitch id="appearance.useAppIconsForDocuments" />
        </SettingRow>
    {/if}

    {#if shouldShow('appearance.fileSizeFormat')}
        <SettingRow
            id="appearance.fileSizeFormat"
            label={fileSizeDef.label}
            description={fileSizeDef.description}
            {searchQuery}
        >
            <SettingSelect id="appearance.fileSizeFormat" />
        </SettingRow>
    {/if}

    {#if shouldShow('appearance.dateTimeFormat')}
        <SettingRow
            id="appearance.dateTimeFormat"
            label={dateTimeDef.label}
            description={dateTimeDef.description}
            {searchQuery}
        >
            <div class="date-time-setting">
                <SettingRadioGroup id="appearance.dateTimeFormat">
                    {#snippet customContent(value)}
                        {#if value === 'custom'}
                            <div class="custom-format">
                                <input
                                    type="text"
                                    class="format-input"
                                    value={customFormat}
                                    oninput={handleCustomFormatChange}
                                    placeholder="YYYY-MM-DD HH:mm"
                                />
                                <div class="format-preview">
                                    Preview: <strong>{formatPreview(customFormat)}</strong>
                                </div>
                                <button
                                    type="button"
                                    class="help-toggle"
                                    onclick={() => (showFormatHelp = !showFormatHelp)}
                                >
                                    {showFormatHelp ? 'Hide format help' : 'Show format help'}
                                </button>
                                {#if showFormatHelp}
                                    <div class="format-help">
                                        <h4>Format placeholders</h4>
                                        <ul>
                                            <li><code>YYYY</code> — 4-digit year (2025)</li>
                                            <li><code>MM</code> — 2-digit month (01-12)</li>
                                            <li><code>DD</code> — 2-digit day (01-31)</li>
                                            <li><code>HH</code> — 2-digit hour (00-23)</li>
                                            <li><code>mm</code> — 2-digit minute (00-59)</li>
                                            <li><code>ss</code> — 2-digit second (00-59)</li>
                                        </ul>
                                    </div>
                                {/if}
                            </div>
                        {/if}
                    {/snippet}
                </SettingRadioGroup>
            </div>
        </SettingRow>
    {/if}
</SettingsSection>

<style>
    .app-color-options {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
    }

    .app-color-option {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
        padding: var(--spacing-xs) 0;
        cursor: default;
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
    }

    .app-color-option input[type='radio'] {
        appearance: none;
        width: 16px;
        height: 16px;
        min-width: 16px;
        border: 2px solid var(--color-border-strong);
        border-radius: var(--radius-full);
        background: var(--color-bg-primary);
        margin: 0;
        flex-shrink: 0;
        transition: all var(--transition-base);
    }

    .app-color-option input[type='radio']:checked {
        border-color: var(--color-accent);
        background: var(--color-accent);
        box-shadow: inset 0 0 0 3px var(--color-bg-primary);
    }

    .app-color-option input[type='radio']:focus-visible {
        outline: 2px solid var(--color-accent);
        outline-offset: 2px;
        box-shadow: var(--shadow-focus);
    }

    .color-swatch {
        width: 24px;
        height: 14px;
        min-width: 24px;
        border-radius: var(--radius-sm);
        flex-shrink: 0;
        border: 1px solid var(--color-border);
    }

    .system-swatch {
        background-color: var(--color-system-accent);
    }

    .gold-swatch {
        background-color: var(--color-cmdr-gold);
    }

    .app-color-label {
        font-size: var(--font-size-sm);
    }

    .app-color-description {
        color: var(--color-text-tertiary);
        font-size: var(--font-size-sm);
        margin: 0 0 var(--spacing-md);
        padding-left: 0;
    }

    .appearance-link {
        color: var(--color-accent);
        font-size: var(--font-size-sm);
        text-decoration: underline;
        padding: 0;
        background: none;
        border: none;
        cursor: pointer;
    }

    .appearance-link:hover {
        color: var(--color-accent-hover);
    }

    .date-time-setting {
        /* Fixed width to prevent layout shift when custom content appears */
        width: 250px;
    }

    .custom-format {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
    }

    .format-input {
        width: 180px;
        padding: var(--spacing-xs) var(--spacing-sm);
        border: 1px solid var(--color-border);
        border-radius: var(--radius-sm);
        background: var(--color-bg-primary);
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        font-family: var(--font-mono);
    }

    .format-input:focus {
        outline: none;
        border-color: var(--color-accent);
        box-shadow: var(--shadow-focus);
    }

    .format-preview {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
    }

    .format-preview strong {
        font-family: var(--font-mono);
    }

    .help-toggle {
        align-self: flex-start;
        padding: 0;
        background: none;
        border: none;
        color: var(--color-accent);
        font-size: var(--font-size-sm);
        cursor: pointer;
        text-decoration: underline;
    }

    .format-help {
        background: var(--color-bg-secondary);
        border-radius: var(--radius-sm);
        padding: var(--spacing-sm);
        font-size: var(--font-size-sm);
    }

    .format-help h4 {
        margin: 0 0 var(--spacing-xs);
        font-weight: 600;
    }

    .format-help ul {
        margin: 0;
        padding-left: var(--spacing-md);
    }

    .format-help li {
        margin-bottom: 2px;
    }

    .format-help code {
        background: var(--color-bg-tertiary);
        padding: 1px 4px;
        border-radius: 2px;
        font-family: var(--font-mono);
    }
</style>
