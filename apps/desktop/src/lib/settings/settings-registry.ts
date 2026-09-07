/**
 * Settings registry - single source of truth for all settings.
 *
 * The registry stores message KEYS for everything user-facing (label,
 * description, enum-option labels), not English. `resolveDefinition` turns each
 * authored `SettingDefinitionSource` into a `SettingDefinition` whose `label` /
 * `description` / option labels are getter-backed: reading them resolves the
 * current catalog string through `t()` (`$lib/intl`). This keeps the whole
 * `getSettingDefinition(...).label` consumer surface unchanged while making the
 * copy translation-ready and single-sourced in `messages/en/settings.json`.
 * Section identity (`section: string[]`) stays English on purpose — it's a
 * structural key for routing, the section tree, and search, not a render path;
 * the rendered section TITLES live in the section components via `t()`.
 */

import type {
  EnumOption,
  EnumOptionSource,
  SettingConstraints,
  SettingConstraintsSource,
  SettingDefinition,
  SettingDefinitionSource,
  SettingId,
  SettingsValues,
} from './types'
import { SettingValidationError } from './types'
import { tString } from '$lib/intl/messages.svelte'
import { appearanceSettings } from './definitions/appearance'
import { behaviorSettings } from './definitions/behavior'
import { indexingSettings } from './definitions/indexing'
import { aiSettings } from './definitions/ai'
import { fileSystemsSettings } from './definitions/file-systems'
import { viewerSettings } from './definitions/viewer'
import { updatesPrivacySettings } from './definitions/updates-privacy'
import { advancedSettings } from './definitions/advanced'
import { searchableRows } from './sections/searchable-rows'

// ============================================================================
// Settings Definitions
//
// The authored data lives in `definitions/*.ts`, one array per top-level
// section. Concatenation order here IS the registry order: `buildSectionTree()`
// uses first-appearance order for each (sub)section name, and search / Advanced
// grouping preserve it. Keep this order in sync with `SettingsSidebar.svelte`'s
// `TOP_LEVEL_ORDER` (special non-registry sections — Keyboard shortcuts,
// License — are interleaved there).
// ============================================================================

const settingsRegistrySource: SettingDefinitionSource[] = [
  ...appearanceSettings,
  ...behaviorSettings,
  ...indexingSettings,
  ...aiSettings,
  ...fileSystemsSettings,
  ...viewerSettings,
  ...updatesPrivacySettings,
  ...advancedSettings,
]

// ============================================================================
// Resolution: authored keys → rendered (getter-backed) definitions
//
// `label`/`description`/option labels are getters that resolve the catalog
// string through `t()` at READ time. So every `getSettingDefinition(...).label`
// consumer gets a rendered string (the pre-i18n behavior), reactivity works in
// markup, and snapshot semantics hold in plain `.ts` (matching the transfer
// pilot). An option with a literal `label` (brand names, numerals) passes
// through unchanged; option labels authored with a `labelKey` resolve lazily.
// ============================================================================

/** Resolves one authored option to a rendered `EnumOption` (getter-backed). */
function resolveOption(opt: EnumOptionSource | EnumOption): EnumOption {
  if ('label' in opt) return opt // literal label (brand names, numerals)
  const out: EnumOption = {
    value: opt.value,
    get label() {
      return tString(opt.labelKey)
    },
  }
  if (opt.icon !== undefined) out.icon = opt.icon
  if (opt.descriptionKey !== undefined) {
    const descKey = opt.descriptionKey
    Object.defineProperty(out, 'description', { enumerable: true, get: () => tString(descKey) })
  }
  return out
}

/** Resolves authored constraints, mapping option keys to rendered options. */
function resolveConstraints(c: SettingConstraintsSource | undefined): SettingConstraints | undefined {
  if (!c) return undefined
  const { options, ...rest } = c
  if (!options) return rest
  return { ...rest, options: options.map(resolveOption) }
}

/** Turns an authored source into a `SettingDefinition` with resolved copy. */
function resolveDefinition(src: SettingDefinitionSource): SettingDefinition {
  const { labelKey, descriptionKey, cardKey, constraints, ...rest } = src
  const def = {
    ...rest,
    constraints: resolveConstraints(constraints),
    get label() {
      return tString(labelKey)
    },
    get description() {
      return descriptionKey === undefined ? '' : tString(descriptionKey)
    },
    get card() {
      return cardKey === undefined ? undefined : tString(cardKey)
    },
  } as SettingDefinition
  return def
}

export const settingsRegistry: SettingDefinition[] = settingsRegistrySource.map(resolveDefinition)

// ============================================================================
// Registry Lookup Helpers
// ============================================================================

const registryMap = new Map<string, SettingDefinition>()
for (const setting of settingsRegistry) {
  registryMap.set(setting.id, setting)
}

/**
 * Get the definition for a setting by ID.
 */
export function getSettingDefinition(id: string): SettingDefinition | undefined {
  return registryMap.get(id)
}

/**
 * Get all settings in a section path.
 */
export function getSettingsInSection(sectionPath: string[]): SettingDefinition[] {
  return settingsRegistry.filter((s) => {
    if (s.section.length < sectionPath.length) return false
    return sectionPath.every((part, i) => s.section[i] === part)
  })
}

/**
 * Get all settings that live in the Advanced section. `section[0] === 'Advanced'`
 * is the single home: the Advanced page auto-renders exactly these (no mirrors on
 * feature pages), grouped into cards by `cardKey`. `hidden` entries are excluded
 * (internal state that renders nowhere).
 */
export function getAdvancedSettings(): SettingDefinition[] {
  return settingsRegistry.filter((s) => s.section[0] === 'Advanced' && !s.hidden)
}

/**
 * Get the default value for a setting.
 */
export function getDefaultValue<K extends SettingId>(id: K): SettingsValues[K] {
  const def = registryMap.get(id)
  if (!def) throw new Error(`Unknown setting: ${id}`)
  return def.default as SettingsValues[K]
}

// ============================================================================
// Validation
// ============================================================================

/**
 * Validate a value against a setting's constraints.
 * Throws SettingValidationError if invalid.
 */
export function validateSettingValue(id: string, value: unknown): void {
  const def = registryMap.get(id)
  if (!def) {
    throw new SettingValidationError(id, 'Unknown setting')
  }

  // Type checking
  switch (def.type) {
    case 'boolean':
      if (typeof value !== 'boolean') {
        throw new SettingValidationError(id, `Expected boolean, got ${typeof value}`)
      }
      break

    case 'number':
    case 'duration':
      if (typeof value !== 'number') {
        throw new SettingValidationError(id, `Expected number, got ${typeof value}`)
      }
      if (!Number.isFinite(value)) {
        throw new SettingValidationError(id, 'Value must be a finite number')
      }
      validateNumberConstraints(id, value, def)
      break

    case 'string':
      if (typeof value !== 'string') {
        throw new SettingValidationError(id, `Expected string, got ${typeof value}`)
      }
      break

    case 'enum':
      validateEnumValue(id, value, def)
      break

    case 'string-array':
      if (!Array.isArray(value) || !value.every((v): v is string => typeof v === 'string')) {
        throw new SettingValidationError(id, `Expected an array of strings, got ${typeof value}`)
      }
      break
  }
}

function validateNumberConstraints(id: string, value: number, def: SettingDefinition): void {
  const c = def.constraints
  if (!c) return

  // For duration type, check minMs/maxMs
  if (def.type === 'duration') {
    if (c.minMs !== undefined && value < c.minMs) {
      throw new SettingValidationError(id, `Value ${String(value)}ms is below minimum ${String(c.minMs)}ms`)
    }
    if (c.maxMs !== undefined && value > c.maxMs) {
      throw new SettingValidationError(id, `Value ${String(value)}ms exceeds maximum ${String(c.maxMs)}ms`)
    }
    return
  }

  // For number type, check min/max
  if (c.min !== undefined && value < c.min) {
    throw new SettingValidationError(id, `Value ${String(value)} is below minimum ${String(c.min)}`)
  }
  if (c.max !== undefined && value > c.max) {
    throw new SettingValidationError(id, `Value ${String(value)} exceeds maximum ${String(c.max)}`)
  }
}

function validateEnumValue(id: string, value: unknown, def: SettingDefinition): void {
  const c = def.constraints
  if (!c?.options) return

  const validValues = c.options.map((o) => o.value)

  // Check if it's one of the predefined options
  if (validValues.includes(value as string | number)) {
    return
  }

  // Check if custom values are allowed
  if (c.allowCustom && typeof value === 'number') {
    if (c.customMin !== undefined && value < c.customMin) {
      throw new SettingValidationError(id, `Custom value ${String(value)} is below minimum ${String(c.customMin)}`)
    }
    if (c.customMax !== undefined && value > c.customMax) {
      throw new SettingValidationError(id, `Custom value ${String(value)} exceeds maximum ${String(c.customMax)}`)
    }
    return
  }

  throw new SettingValidationError(id, `Invalid value '${String(value)}'. Valid options: ${validValues.join(', ')}`)
}

// ============================================================================
// Section Tree Building
// ============================================================================

export interface SettingsSection {
  name: string
  path: string[]
  subsections: SettingsSection[]
  settings: SettingDefinition[]
}

/**
 * Build the nav tree: every (sub)section that has something to show, in
 * first-appearance order.
 *
 * Two sources, and the asymmetry is the point. A SETTING creates its section's
 * node and lands in `section.settings`. A SEARCHABLE ROW creates the node only,
 * and only when it says `anchorsSection` — that's for a page whose whole content
 * is action rows (`Servers (SFTP, WebDAV)`), which no setting would otherwise
 * put in the sidebar. So `section.settings` stays controls only, and a row still
 * never decides what renders.
 */
export function buildSectionTree(): SettingsSection[] {
  const root: SettingsSection[] = []
  const sectionMap = new Map<string, SettingsSection>()

  /**
   * Walks `path`, creating any missing node, and answers the deepest one. A node
   * created here lands last among its siblings, which for a setting IS registry
   * order; `after` is how an anchoring row states a place it doesn't have.
   */
  function ensureSection(path: string[], after?: string): SettingsSection {
    let currentLevel = root
    let currentPath: string[] = []
    let section!: SettingsSection

    for (const sectionName of path) {
      currentPath = [...currentPath, sectionName]
      const pathKey = currentPath.join('/')

      let existing = sectionMap.get(pathKey)
      if (!existing) {
        existing = {
          name: sectionName,
          path: [...currentPath],
          subsections: [],
          settings: [],
        }
        sectionMap.set(pathKey, existing)

        const isLeaf = currentPath.length === path.length
        const afterIndex =
          isLeaf && after !== undefined ? currentLevel.findIndex((sibling) => sibling.name === after) : -1
        if (afterIndex === -1) currentLevel.push(existing)
        else currentLevel.splice(afterIndex + 1, 0, existing)
      }

      section = existing
      currentLevel = existing.subsections
    }

    return section
  }

  for (const setting of settingsRegistry) {
    // Internal-only settings (e.g. `network.firstTriggerDone`) are not nav rows,
    // and don't create one either.
    if (setting.hidden) continue
    ensureSection(setting.section).settings.push(setting)
  }

  // Anchoring rows run last so every sibling a page names already exists.
  for (const row of searchableRows) {
    if (row.anchorsSection !== undefined) ensureSection(row.section, row.anchorsSection.after)
  }

  return root
}
