<script lang="ts">
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import SettingSelect from '../components/SettingSelect.svelte'
    import SettingToggleGroup from '../components/SettingToggleGroup.svelte'
    import SettingRadioGroup from '../components/SettingRadioGroup.svelte'
    import { getSettingDefinition, getSetting, setSetting } from '$lib/settings'

    interface Props {
        searchQuery: string
    }

    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    const { searchQuery }: Props = $props()

    // Get definitions for rendering (with fallbacks for type safety)
    const uiDensityDef = getSettingDefinition('appearance.uiDensity') ?? { label: '', description: '' }
    const appIconsDef = getSettingDefinition('appearance.useAppIconsForDocuments') ?? { label: '', description: '' }
    const fileSizeDef = getSettingDefinition('appearance.fileSizeFormat') ?? { label: '', description: '' }
    const dateTimeDef = getSettingDefinition('appearance.dateTimeFormat') ?? { label: '', description: '' }

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

<div class="section">
    <h2 class="section-title">Appearance</h2>

    <SettingRow id="appearance.uiDensity" label={uiDensityDef.label} description={uiDensityDef.description}>
        <SettingToggleGroup id="appearance.uiDensity" />
    </SettingRow>

    <SettingRow id="appearance.useAppIconsForDocuments" label={appIconsDef.label} description={appIconsDef.description}>
        <SettingSwitch id="appearance.useAppIconsForDocuments" />
    </SettingRow>

    <SettingRow id="appearance.fileSizeFormat" label={fileSizeDef.label} description={fileSizeDef.description}>
        <SettingSelect id="appearance.fileSizeFormat" />
    </SettingRow>

    <SettingRow id="appearance.dateTimeFormat" label={dateTimeDef.label} description={dateTimeDef.description}>
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
</div>

<style>
    .section {
        margin-bottom: var(--spacing-md);
    }

    .section-title {
        font-size: 16px;
        font-weight: 600;
        color: var(--color-text-primary);
        margin: 0 0 var(--spacing-sm);
        padding-bottom: var(--spacing-xs);
        border-bottom: 1px solid var(--color-border);
    }

    .date-time-setting {
        width: 100%;
    }

    .custom-format {
        display: flex;
        flex-direction: column;
        gap: var(--spacing-xs);
    }

    .format-input {
        padding: var(--spacing-xs) var(--spacing-sm);
        border: 1px solid var(--color-border);
        border-radius: 4px;
        background: var(--color-bg-primary);
        color: var(--color-text-primary);
        font-size: var(--font-size-sm);
        font-family: monospace;
    }

    .format-input:focus {
        outline: none;
        border-color: var(--color-accent);
    }

    .format-preview {
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
    }

    .format-preview strong {
        font-family: monospace;
    }

    .help-toggle {
        align-self: flex-start;
        padding: 0;
        background: none;
        border: none;
        color: var(--color-accent);
        font-size: var(--font-size-xs);
        cursor: pointer;
        text-decoration: underline;
    }

    .format-help {
        background: var(--color-bg-secondary);
        border-radius: 4px;
        padding: var(--spacing-sm);
        font-size: var(--font-size-xs);
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
        font-family: monospace;
    }
</style>
