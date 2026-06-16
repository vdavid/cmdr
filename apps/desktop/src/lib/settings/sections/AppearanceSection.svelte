<script lang="ts">
    import { onMount } from 'svelte'
    import SettingsSection from '../components/SettingsSection.svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSwitch from '../components/SettingSwitch.svelte'
    import SettingToggleGroup from '../components/SettingToggleGroup.svelte'
    import SettingRadioGroup from '../components/SettingRadioGroup.svelte'
    import SettingColorSwatchPicker from '../components/SettingColorSwatchPicker.svelte'
    import LinkButton from '$lib/ui/LinkButton.svelte'
    import { getSettingDefinition, getSetting, setSetting, onSpecificSettingChange } from '$lib/settings'
    import { createShouldShow } from '$lib/settings/settings-search'
    import { openAppearanceSettings } from '$lib/tauri-commands'
    import { isMacOS } from '$lib/shortcuts/key-capture'
    import { systemStrings } from '$lib/system-strings.svelte'
    import { tString } from '$lib/intl/messages.svelte'

    interface Props {
        searchQuery: string
    }

    const { searchQuery }: Props = $props()

    const shouldShow = $derived(createShouldShow(searchQuery))

    // Definitions for rendering (with fallbacks for type safety)
    const themeModeDef = getSettingDefinition('theme.mode') ?? { label: '', description: '' }
    const appColorDef = getSettingDefinition('appearance.appColor') ?? { label: '', description: '' }
    const sizeColorsDef = getSettingDefinition('appearance.sizeColors') ?? { label: '', description: '' }
    const dateColorsDef = getSettingDefinition('appearance.dateColors') ?? { label: '', description: '' }
    const dateTimeDef = getSettingDefinition('appearance.dateTimeFormat') ?? { label: '', description: '' }
    const stripedRowsDef = getSettingDefinition('listing.stripedRows') ?? { label: '', description: '' }
    const tintLocalDef = getSettingDefinition('appearance.tintLocal') ?? { label: '', description: '' }
    const tintSmbDef = getSettingDefinition('appearance.tintSmb') ?? { label: '', description: '' }
    const tintMtpDef = getSettingDefinition('appearance.tintMtp') ?? { label: '', description: '' }

    // App color state
    let appColorValue = $state(getSetting('appearance.appColor'))

    onMount(() => {
        return onSpecificSettingChange('appearance.appColor', (_id, newValue) => {
            appColorValue = newValue
        })
    })

    // Fixed date-format token example, locale-independent (matches the registry
    // default). Not user copy, so it isn't catalogued; the lint keys on literal
    // `placeholder` attributes, so it lives in a named const here instead.
    const DATE_FORMAT_PLACEHOLDER = 'YYYY-MM-DD HH:mm'

    // Format-help rows: the TOKEN is a fixed, locale-independent code literal
    // (not copy, so it stays here, not in the catalog); only the human-readable
    // hint after it is translated. Keeping tokens in script also keeps them out
    // of the no-raw-string lint's text-node scan.
    const FORMAT_HELP_ROWS = [
        { token: 'YYYY', descKey: 'settings.appearance.formatHelpYear' },
        { token: 'MM', descKey: 'settings.appearance.formatHelpMonth' },
        { token: 'DD', descKey: 'settings.appearance.formatHelpDay' },
        { token: 'HH', descKey: 'settings.appearance.formatHelpHour' },
        { token: 'mm', descKey: 'settings.appearance.formatHelpMinute' },
        { token: 'ss', descKey: 'settings.appearance.formatHelpSecond' },
    ] as const

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
            return tString('settings.appearance.dateInvalidFormat')
        }
    }

    function handleCustomFormatChange(event: Event) {
        const target = event.target as HTMLInputElement
        customFormat = target.value
        setSetting('appearance.customDateTimeFormat', target.value)
    }
</script>

<SettingsSection title={tString('settings.section.colorsAndFormats')}>
    {#if shouldShow('theme.mode')}
        <SettingRow id="theme.mode" label={themeModeDef.label} description={themeModeDef.description} {searchQuery}>
            <SettingToggleGroup id="theme.mode" />
        </SettingRow>
    {/if}

    {#if shouldShow('appearance.appColor')}
        <SettingRow id="appearance.appColor" label={appColorDef.label} description="" split {searchQuery}>
            {#snippet descriptionContent()}
                {tString('settings.appearance.appColorHintPrefix')}
                <LinkButton onclick={() => void openAppearanceSettings()}
                    >{isMacOS()
                        ? tString('settings.appearance.appColorHintLinkMac', {
                              systemSettings: systemStrings.systemSettings,
                              appearance: systemStrings.appearance,
                          })
                        : tString('settings.appearance.appColorHintLinkOther')}</LinkButton
                >.
            {/snippet}
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
                    <!-- The system swatch doubles as a click-through to
                         System Settings › Appearance (where the user actually
                         changes the underlying color). No visual cue by
                         intent — discoverable, not advertised.
                         `stopPropagation` keeps the click from also flipping
                         the surrounding radio. -->
                    <span
                        class="color-swatch system-swatch"
                        role="button"
                        tabindex="-1"
                        aria-label={tString('settings.appearance.themeColorSwatchAria')}
                        onclick={(e: MouseEvent) => {
                            e.preventDefault()
                            e.stopPropagation()
                            void openAppearanceSettings()
                        }}
                        onkeydown={() => {}}
                    ></span>
                    <span class="app-color-label">{tString('settings.appearance.appColor.opt.system')}</span>
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
                    <span class="app-color-label">{tString('settings.appearance.appColor.opt.cmdrGold')}</span>
                </label>
            </div>
        </SettingRow>
    {/if}

    {#if shouldShow('appearance.sizeColors')}
        <SettingRow
            id="appearance.sizeColors"
            label={sizeColorsDef.label}
            description={sizeColorsDef.description}
            {searchQuery}
        >
            <SettingToggleGroup id="appearance.sizeColors" />
        </SettingRow>
    {/if}

    {#if shouldShow('appearance.dateColors')}
        <SettingRow
            id="appearance.dateColors"
            label={dateColorsDef.label}
            description={dateColorsDef.description}
            {searchQuery}
        >
            <SettingToggleGroup id="appearance.dateColors" />
        </SettingRow>
    {/if}

    {#if shouldShow('appearance.dateTimeFormat')}
        <SettingRow
            id="appearance.dateTimeFormat"
            label={dateTimeDef.label}
            description={dateTimeDef.description}
            split
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
                                    placeholder={DATE_FORMAT_PLACEHOLDER}
                                />
                                <div class="format-preview">
                                    {tString('settings.appearance.datePreviewLabel')}
                                    <strong>{formatPreview(customFormat)}</strong>
                                </div>
                                <span class="help-toggle-wrapper">
                                    <LinkButton onclick={() => (showFormatHelp = !showFormatHelp)}>
                                        {showFormatHelp
                                            ? tString('settings.appearance.hideFormatHelp')
                                            : tString('settings.appearance.showFormatHelp')}
                                    </LinkButton>
                                </span>
                                {#if showFormatHelp}
                                    <div class="format-help">
                                        <h4>{tString('settings.appearance.formatPlaceholdersTitle')}</h4>
                                        <ul>
                                            {#each FORMAT_HELP_ROWS as row (row.token)}
                                                <li><code>{row.token}</code>: {tString(row.descKey)}</li>
                                            {/each}
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

    {#if shouldShow('listing.stripedRows')}
        <SettingRow
            id="listing.stripedRows"
            label={stripedRowsDef.label}
            description={stripedRowsDef.description}
            {searchQuery}
        >
            <SettingSwitch id="listing.stripedRows" />
        </SettingRow>
    {/if}

    {#if shouldShow('appearance.tintLocal') || shouldShow('appearance.tintSmb') || shouldShow('appearance.tintMtp')}
        <div class="tint-group" role="group" aria-labelledby="tint-group-heading">
            <h3 id="tint-group-heading" class="tint-group-heading">
                {tString('settings.appearance.tintGroupHeading')}
            </h3>
            <p class="tint-group-description">
                {tString('settings.appearance.tintGroupDescription')}
            </p>
            {#if shouldShow('appearance.tintLocal')}
                <SettingRow
                    id="appearance.tintLocal"
                    label={tintLocalDef.label}
                    description={tintLocalDef.description}
                    split
                    {searchQuery}
                >
                    <SettingColorSwatchPicker id="appearance.tintLocal" label={tintLocalDef.label} />
                </SettingRow>
            {/if}
            {#if shouldShow('appearance.tintSmb')}
                <SettingRow
                    id="appearance.tintSmb"
                    label={tintSmbDef.label}
                    description={tintSmbDef.description}
                    split
                    {searchQuery}
                >
                    <SettingColorSwatchPicker id="appearance.tintSmb" label={tintSmbDef.label} />
                </SettingRow>
            {/if}
            {#if shouldShow('appearance.tintMtp')}
                <SettingRow
                    id="appearance.tintMtp"
                    label={tintMtpDef.label}
                    description={tintMtpDef.description}
                    split
                    {searchQuery}
                >
                    <SettingColorSwatchPicker id="appearance.tintMtp" label={tintMtpDef.label} />
                </SettingRow>
            {/if}
        </div>
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

    .date-time-setting {
        /* Fill the split column; min-width prevents collapse */
        width: 100%;
        min-width: 200px;
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

    .help-toggle-wrapper {
        align-self: flex-start;
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
        margin-bottom: var(--spacing-xxs);
    }

    .format-help code {
        background: var(--color-bg-tertiary);
        padding: 1px var(--spacing-xs);
        border-radius: var(--radius-xs);
        font-family: var(--font-mono);
    }

    .tint-group {
        margin-top: var(--spacing-md);
        padding-top: var(--spacing-md);
        /* `border-top` removed — the preceding `.setting-row` already
           paints its own `border-bottom`, so a `border-top` here renders
           a doubled separator between "Striped rows" and "Tint volume
           types". The `padding-top` + `margin-top` give the group
           breathing room without an extra hairline. */
    }

    .tint-group-heading {
        font-size: var(--font-size-md);
        font-weight: 600;
        color: var(--color-text-primary);
        margin: 0 0 var(--spacing-xxs);
    }

    .tint-group-description {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        margin: 0 0 var(--spacing-sm);
    }
</style>
