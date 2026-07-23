/**
 * Appearance section settings (data only). Logic lives in `../settings-registry.ts`,
 * which concatenates this array into the full registry in section order.
 */

import type { EnumOption, EnumOptionSource, SettingDefinitionSource } from '../types'
import { VOLUME_TINT_COLORS } from '../types'
import { availableLocales } from '$lib/intl/messages.svelte'
import type { MessageKey } from '$lib/intl/keys.gen'

/**
 * Display name for a locale tag, in the locale's OWN language (`de` → "Deutsch",
 * `pt-BR` → "português (Brasil)"), so the picker is self-describing and we never
 * hardcode a language-name list. Falls back to the raw tag when `Intl` can't
 * resolve a name. `Intl.DisplayNames` is not a number/date formatter, so it's
 * exempt from the `no-raw-locale-format` rule.
 */
function localeDisplayName(tag: string): string {
  try {
    const name = new Intl.DisplayNames([tag], { type: 'language' }).of(tag)
    if (name !== undefined && name !== tag) {
      // Capitalize the first letter: many languages lowercase their endonym, but
      // a selector option reads better title-first. Locale-aware via the tag.
      return name.charAt(0).toLocaleUpperCase(tag) + name.slice(1)
    }
  } catch {
    // fall through to the raw tag
  }
  return tag
}

/**
 * The language-picker options: "System default" (value `'system'`, follows the
 * OS locale) plus one option per AVAILABLE catalog locale, derived live from
 * `availableLocales()` so a newly-added locale dir auto-appears with no edit
 * here. Per-locale options carry a literal `label` (the locale's endonym, not
 * catalogued copy), so they pass through `resolveOption` unchanged.
 */
function languageOptions(): (EnumOptionSource | EnumOption)[] {
  return [
    { value: 'system', labelKey: 'settings.appearance.language.opt.system' },
    ...availableLocales().map((tag) => ({ value: tag, label: localeDisplayName(tag) })),
  ]
}

/** Message-key leaf for each tint color, used by the tint enum options. */
const TINT_COLOR_KEY: Record<string, MessageKey> = {
  none: 'settings.tint.none',
  red: 'settings.tint.red',
  orange: 'settings.tint.orange',
  amber: 'settings.tint.amber',
  lime: 'settings.tint.lime',
  green: 'settings.tint.green',
  teal: 'settings.tint.teal',
  cyan: 'settings.tint.cyan',
  blue: 'settings.tint.blue',
  indigo: 'settings.tint.indigo',
  purple: 'settings.tint.purple',
  pink: 'settings.tint.pink',
  brown: 'settings.tint.brown',
}

/** Options list for the three `appearance.tint{Local,Smb,Mtp}` settings. */
const TINT_COLOR_OPTIONS: EnumOptionSource[] = [
  { value: 'none', labelKey: TINT_COLOR_KEY.none },
  ...VOLUME_TINT_COLORS.map((c) => ({ value: c, labelKey: TINT_COLOR_KEY[c] })),
]

export const appearanceSettings: SettingDefinitionSource[] = [
  // ========================================================================
  // Appearance › Colors and formats
  // ========================================================================
  {
    id: 'appearance.language',
    section: ['Appearance', 'Colors and formats'],
    labelKey: 'settings.appearance.language.label',
    descriptionKey: 'settings.appearance.language.description',
    cardKey: 'settings.appearance.card.language',
    keywords: ['language', 'locale', 'translation', 'i18n', 'region', 'english'],
    type: 'enum',
    default: 'system',
    component: 'select',
    // Options are derived from the loaded catalogs at module load: "System
    // default" plus every available locale. A new locale dir auto-appears (no
    // edit here). See `languageOptions`.
    constraints: { options: languageOptions() },
  },
  {
    id: 'theme.mode',
    section: ['Appearance', 'Colors and formats'],
    labelKey: 'settings.theme.mode.label',
    descriptionKey: 'settings.theme.mode.description',
    cardKey: 'settings.appearance.card.theme',
    keywords: ['theme', 'dark', 'light', 'mode', 'appearance', 'color'],
    type: 'enum',
    default: 'system',
    component: 'toggle-group',
    constraints: {
      options: [
        { value: 'light', labelKey: 'settings.theme.mode.opt.light', icon: 'sun' },
        { value: 'dark', labelKey: 'settings.theme.mode.opt.dark', icon: 'moon' },
        { value: 'system', labelKey: 'settings.theme.mode.opt.system', icon: 'monitor' },
      ],
    },
  },
  {
    id: 'appearance.appColor',
    section: ['Appearance', 'Colors and formats'],
    labelKey: 'settings.appearance.appColor.label',
    // The rendered description is a `<LinkButton>` snippet in `AppearanceSection`
    // (platform-branching via `isMacOS()`); this key feeds search only.
    descriptionKey: 'settings.appearance.appColor.description',
    cardKey: 'settings.appearance.card.theme',
    keywords: ['color', 'accent', 'theme', 'gold', 'system', 'brand'],
    type: 'enum',
    default: 'system',
    component: 'radio',
    constraints: {
      options: [
        { value: 'system', labelKey: 'settings.appearance.appColor.opt.system' },
        { value: 'cmdr-gold', labelKey: 'settings.appearance.appColor.opt.cmdrGold' },
      ],
    },
  },
  {
    id: 'appearance.sizeColors',
    section: ['Appearance', 'Colors and formats'],
    labelKey: 'settings.appearance.sizeColors.label',
    descriptionKey: 'settings.appearance.sizeColors.description',
    cardKey: 'settings.appearance.card.listColoring',
    keywords: ['size', 'color', 'tier', 'rainbow', 'app', 'highlight', 'kb', 'mb', 'gb', 'tb'],
    type: 'enum',
    default: 'none',
    component: 'toggle-group',
    constraints: {
      options: [
        { value: 'none', labelKey: 'settings.appearance.sizeColors.opt.none' },
        { value: 'app', labelKey: 'settings.appearance.sizeColors.opt.app' },
        { value: 'rainbow', labelKey: 'settings.appearance.sizeColors.opt.rainbow' },
      ],
    },
  },
  {
    id: 'appearance.dateColors',
    section: ['Appearance', 'Colors and formats'],
    labelKey: 'settings.appearance.dateColors.label',
    descriptionKey: 'settings.appearance.dateColors.description',
    cardKey: 'settings.appearance.card.listColoring',
    keywords: ['date', 'color', 'age', 'modified', 'wilting', 'app', 'fresh', 'old'],
    type: 'enum',
    default: 'none',
    component: 'toggle-group',
    constraints: {
      options: [
        { value: 'none', labelKey: 'settings.appearance.dateColors.opt.none' },
        { value: 'app', labelKey: 'settings.appearance.dateColors.opt.app' },
        { value: 'wilting', labelKey: 'settings.appearance.dateColors.opt.wilting' },
      ],
    },
  },
  {
    id: 'appearance.dateTimeFormat',
    section: ['Appearance', 'Colors and formats'],
    labelKey: 'settings.appearance.dateTimeFormat.label',
    descriptionKey: 'settings.appearance.dateTimeFormat.description',
    cardKey: 'settings.appearance.card.dateAndTime',
    keywords: ['date', 'time', 'format', 'iso', 'custom', 'timestamp'],
    type: 'enum',
    default: 'iso',
    component: 'radio',
    constraints: {
      options: [
        { value: 'system', labelKey: 'settings.appearance.dateTimeFormat.opt.system' },
        {
          value: 'iso',
          labelKey: 'settings.appearance.dateTimeFormat.opt.iso',
          descriptionKey: 'settings.appearance.dateTimeFormat.optDesc.iso',
        },
        {
          value: 'short',
          labelKey: 'settings.appearance.dateTimeFormat.opt.short',
          descriptionKey: 'settings.appearance.dateTimeFormat.optDesc.short',
        },
        { value: 'custom', labelKey: 'settings.appearance.dateTimeFormat.opt.custom' },
      ],
      allowCustom: true,
    },
  },
  {
    id: 'appearance.customDateTimeFormat',
    section: ['Appearance', 'Colors and formats'],
    labelKey: 'settings.appearance.customDateTimeFormat.label',
    descriptionKey: 'settings.appearance.customDateTimeFormat.description',
    cardKey: 'settings.appearance.card.dateAndTime',
    keywords: ['custom', 'format', 'date', 'time', 'placeholder'],
    type: 'string',
    default: 'YYYY-MM-DD HH:mm',
    component: 'text-input',
  },
  {
    id: 'listing.stripedRows',
    section: ['Appearance', 'Colors and formats'],
    labelKey: 'settings.listing.stripedRows.label',
    descriptionKey: 'settings.listing.stripedRows.description',
    cardKey: 'settings.appearance.card.listColoring',
    keywords: ['stripe', 'zebra', 'alternate', 'row', 'shading', 'accessibility', 'a11y'],
    type: 'boolean',
    default: false,
    component: 'switch',
  },
  // Volume tints (12-color picker, rendered by `AppearanceSection.svelte` via
  // `SettingColorSwatchPicker`, not the registry-driven enum components).
  // Enum type carries the valid values for MCP agents and runtime validation;
  // live-apply happens reactively in `FilePane.svelte` via `volume-tint.svelte.ts`.
  {
    id: 'appearance.tintLocal',
    section: ['Appearance', 'Colors and formats'],
    labelKey: 'settings.appearance.tintLocal.label',
    descriptionKey: 'settings.appearance.tintLocal.description',
    cardKey: 'settings.appearance.card.paneTints',
    keywords: ['tint', 'pane', 'color', 'volume', 'local', 'background', 'highlight'],
    type: 'enum',
    default: 'none',
    constraints: { options: TINT_COLOR_OPTIONS },
  },
  {
    id: 'appearance.tintSmb',
    section: ['Appearance', 'Colors and formats'],
    labelKey: 'settings.appearance.tintSmb.label',
    descriptionKey: 'settings.appearance.tintSmb.description',
    cardKey: 'settings.appearance.card.paneTints',
    keywords: ['tint', 'pane', 'color', 'volume', 'smb', 'network', 'background', 'highlight'],
    type: 'enum',
    default: 'none',
    constraints: { options: TINT_COLOR_OPTIONS },
  },
  {
    id: 'appearance.tintMtp',
    section: ['Appearance', 'Colors and formats'],
    labelKey: 'settings.appearance.tintMtp.label',
    descriptionKey: 'settings.appearance.tintMtp.description',
    cardKey: 'settings.appearance.card.paneTints',
    keywords: ['tint', 'pane', 'color', 'volume', 'mtp', 'android', 'kindle', 'camera', 'background', 'highlight'],
    type: 'enum',
    default: 'none',
    constraints: { options: TINT_COLOR_OPTIONS },
  },

  // ========================================================================
  // Appearance › Zoom and density
  // ========================================================================
  {
    id: 'appearance.textSize',
    section: ['Appearance', 'Zoom and density'],
    labelKey: 'settings.appearance.textSize.label',
    descriptionKey: 'settings.appearance.textSize.description',
    keywords: ['text', 'size', 'font', 'larger', 'smaller', 'accessibility', 'a11y', 'zoom', 'scale'],
    type: 'number',
    default: 100,
    component: 'slider',
    constraints: {
      min: 75,
      max: 150,
      step: 5,
      sliderStops: [75, 100, 125, 150],
    },
  },
  {
    id: 'appearance.uiDensity',
    section: ['Appearance', 'Zoom and density'],
    labelKey: 'settings.appearance.uiDensity.label',
    descriptionKey: 'settings.appearance.uiDensity.description',
    keywords: ['compact', 'comfortable', 'spacious', 'size', 'spacing', 'dense'],
    type: 'enum',
    default: 'comfortable',
    component: 'toggle-group',
    constraints: {
      options: [
        { value: 'compact', labelKey: 'settings.appearance.uiDensity.opt.compact' },
        { value: 'comfortable', labelKey: 'settings.appearance.uiDensity.opt.comfortable' },
        { value: 'spacious', labelKey: 'settings.appearance.uiDensity.opt.spacious' },
      ],
    },
  },

  // ========================================================================
  // Appearance › File and folder sizes
  // ========================================================================
  {
    id: 'listing.sizeDisplay',
    section: ['Appearance', 'File and folder sizes'],
    labelKey: 'settings.listing.sizeDisplay.label',
    descriptionKey: 'settings.listing.sizeDisplay.description',
    keywords: ['size', 'display', 'logical', 'physical', 'smart', 'disk', 'content', 'sparse'],
    type: 'enum',
    default: 'smart',
    component: 'toggle-group',
    constraints: {
      options: [
        { value: 'smart', labelKey: 'settings.listing.sizeDisplay.opt.smart' },
        { value: 'logical', labelKey: 'settings.listing.sizeDisplay.opt.logical' },
        { value: 'physical', labelKey: 'settings.listing.sizeDisplay.opt.physical' },
      ],
    },
  },
  {
    id: 'listing.sizeUnit',
    section: ['Appearance', 'File and folder sizes'],
    labelKey: 'settings.listing.sizeUnit.label',
    descriptionKey: 'settings.listing.sizeUnit.description',
    keywords: ['size', 'human', 'bytes', 'unit', 'format', 'raw', 'precise', 'kb', 'mb', 'gb', 'dynamic'],
    type: 'enum',
    default: 'dynamic',
    component: 'toggle-group',
    constraints: {
      options: [
        { value: 'dynamic', labelKey: 'settings.listing.sizeUnit.opt.dynamic' },
        { value: 'bytes', labelKey: 'settings.listing.sizeUnit.opt.bytes' },
        { value: 'kB', labelKey: 'settings.listing.sizeUnit.opt.kB' },
        { value: 'MB', labelKey: 'settings.listing.sizeUnit.opt.mB' },
        { value: 'GB', labelKey: 'settings.listing.sizeUnit.opt.gB' },
      ],
    },
  },
  {
    id: 'appearance.fileSizeFormat',
    section: ['Appearance', 'File and folder sizes'],
    labelKey: 'settings.appearance.fileSizeFormat.label',
    descriptionKey: 'settings.appearance.fileSizeFormat.description',
    keywords: ['size', 'bytes', 'binary', 'decimal', 'kb', 'mb', 'kib', 'mib'],
    type: 'enum',
    default: 'binary',
    component: 'toggle-group',
    constraints: {
      options: [
        { value: 'binary', labelKey: 'settings.appearance.fileSizeFormat.opt.binary' },
        { value: 'si', labelKey: 'settings.appearance.fileSizeFormat.opt.si' },
      ],
    },
  },
  {
    id: 'listing.sizeMismatchWarning',
    section: ['Appearance', 'File and folder sizes'],
    labelKey: 'settings.listing.sizeMismatchWarning.label',
    descriptionKey: 'settings.listing.sizeMismatchWarning.description',
    keywords: ['size', 'mismatch', 'warning', 'alert', 'disk', 'content', 'difference'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },

  // ========================================================================
  // Appearance › Listing
  // ========================================================================
  {
    id: 'appearance.useAppIconsForDocuments',
    section: ['Appearance', 'Listing'],
    labelKey: 'settings.appearance.useAppIconsForDocuments.label',
    descriptionKey: 'settings.appearance.useAppIconsForDocuments.description',
    cardKey: 'settings.appearance.card.namesAndIcons',
    keywords: ['icon', 'document', 'file', 'app', 'colorful', 'finder'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    id: 'appearance.showFunctionKeyBar',
    section: ['Appearance', 'Listing'],
    labelKey: 'settings.appearance.showFunctionKeyBar.label',
    descriptionKey: 'settings.appearance.showFunctionKeyBar.description',
    cardKey: 'settings.appearance.card.namesAndIcons',
    keywords: ['function', 'key', 'bar', 'f-key', 'fkey', 'shortcut', 'buttons', 'bottom', 'toolbar'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    id: 'listing.showExtensionInName',
    section: ['Appearance', 'Listing'],
    labelKey: 'settings.listing.showExtensionInName.label',
    descriptionKey: 'settings.listing.showExtensionInName.description',
    cardKey: 'settings.appearance.card.namesAndIcons',
    keywords: ['extension', 'ext', 'filename', 'name', 'column', 'full', 'split', 'suffix'],
    type: 'boolean',
    default: false,
    component: 'switch',
  },
  {
    id: 'listing.showTags',
    section: ['Appearance', 'Listing'],
    labelKey: 'settings.listing.showTags.label',
    descriptionKey: 'settings.listing.showTags.description',
    cardKey: 'settings.appearance.card.namesAndIcons',
    keywords: ['tag', 'tags', 'finder', 'label', 'labels', 'color', 'colour', 'dot', 'dots'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    id: 'listing.directorySortMode',
    section: ['Appearance', 'Listing'],
    labelKey: 'settings.listing.directorySortMode.label',
    descriptionKey: 'settings.listing.directorySortMode.description',
    cardKey: 'settings.appearance.card.namesAndIcons',
    keywords: ['sort', 'directory', 'folder', 'order', 'listing', 'name', 'size'],
    type: 'enum',
    default: 'likeFiles',
    component: 'toggle-group',
    constraints: {
      options: [
        { value: 'likeFiles', labelKey: 'settings.listing.directorySortMode.opt.likeFiles' },
        { value: 'alwaysByName', labelKey: 'settings.listing.directorySortMode.opt.alwaysByName' },
      ],
    },
  },
  {
    id: 'listing.briefColumnWidthMode',
    section: ['Appearance', 'Listing'],
    labelKey: 'settings.listing.briefColumnWidthMode.label',
    descriptionKey: 'settings.listing.briefColumnWidthMode.description',
    cardKey: 'settings.appearance.card.briefMode',
    keywords: ['brief', 'column', 'width', 'max', 'maximum', 'limit', 'pane', 'shrink-wrap'],
    type: 'enum',
    default: 'paneWidth',
    component: 'radio',
    constraints: {
      options: [
        { value: 'paneWidth', labelKey: 'settings.listing.briefColumnWidthMode.opt.paneWidth' },
        { value: 'limited', labelKey: 'settings.listing.briefColumnWidthMode.opt.limited' },
      ],
    },
  },
  {
    id: 'listing.briefColumnWidthMaxPx',
    section: ['Appearance', 'Listing'],
    labelKey: 'settings.listing.briefColumnWidthMaxPx.label',
    cardKey: 'settings.appearance.card.briefMode',
    keywords: ['brief', 'column', 'width', 'max', 'maximum', 'limit', 'pixel'],
    type: 'number',
    default: 400,
    component: 'number-input',
    constraints: {
      min: 250,
      max: 1000,
      step: 25,
    },
  },
]
