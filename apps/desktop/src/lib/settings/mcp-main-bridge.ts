/**
 * MCP Main Bridge - handles MCP settings events in the main window.
 *
 * The main window is always alive, so these handlers are always available.
 * Replaces the old mcp-settings-bridge.ts which required the settings window to be open.
 *
 * Round-trip protocol:
 * - Backend emits an event with a `requestId` in the payload
 * - Frontend processes it and emits `mcp-response` with `{ requestId, ok, data?, error? }`
 */

import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event'
import {
  getDefaultValue,
  getSettingDefinition,
  settingRendersHere,
  settingsRegistry,
  validateSettingWrite,
} from './settings-registry'
import { getSetting, setSetting, isModified } from './settings-store'
import { MASKED, holdsMaskedPlaceholder, maskSecrets } from './setting-secret-mask'
import type { SettingDefinition, SettingId, SettingsValues } from './types'
import { getEffectiveShortcuts, getDefaultShortcuts, isShortcutModified } from '#lib/shortcuts/index.js'
import { listedCommands } from '#lib/commands/command-registry.js'
import { getAppLogger } from '#lib/logging/logger.js'
import { getSettingLock, isSettingManaged } from '#lib/managed-policy/managed-policy.svelte.js'
import { lockAllowsWrite } from '#lib/managed-policy/overlay.js'

const log = getAppLogger('mcp-main-bridge')

let unlistenFns: UnlistenFn[] = []

// ============================================================================
// mcp-get-all-settings handler (for cmdr://settings resource)
// ============================================================================

interface GetAllSettingsPayload {
  requestId: string
}

/** A setting whose whole value stays home, whatever it looks like. Every other value still passes
 *  `maskSecrets`: AI provider keys live in the OS secret store, but a base URL can carry a credential. */
function isSensitive(def: SettingDefinition): boolean {
  return def.component === 'password-input' || def.mcpMasked === true
}

/** Mask a sensitive value for safe display. */
function maskValue(_def: SettingDefinition, value: unknown): unknown {
  return typeof value === 'string' && value.length > 0 ? MASKED : ''
}

/** Build a YAML representation of all settings grouped by section. */
function buildAllSettingsYaml(): string {
  const lines: string[] = []

  // Group settings by top-level section
  const sectionMap = new Map<string, typeof settingsRegistry>()
  // A setting for a feature this OS lacks (`needsCapability`) renders nowhere, so it isn't offered.
  for (const def of settingsRegistry.filter(settingRendersHere)) {
    const sectionKey = def.section.join(' > ')
    const existing = sectionMap.get(sectionKey) ?? []
    existing.push(def)
    sectionMap.set(sectionKey, existing)
  }

  lines.push('settings:')
  for (const [sectionKey, settings] of sectionMap) {
    lines.push(`  # ${sectionKey}`)
    for (const def of settings) {
      const id = def.id
      const value = getSetting(id)
      const modified = isModified(id)
      const sensitive = isSensitive(def)

      const displayValue = sensitive ? maskValue(def, value) : value
      const defaultValue = getDefaultValue(id)
      const displayDefault = sensitive ? maskValue(def, defaultValue) : defaultValue

      lines.push(`  - id: ${def.id}`)
      lines.push(`    label: ${JSON.stringify(def.label)}`)
      lines.push(`    type: ${def.type}`)
      lines.push(`    value: ${formatYamlValue(displayValue)}`)
      lines.push(`    default: ${formatYamlValue(displayDefault)}`)
      lines.push(`    modified: ${String(modified)}`)
      if (def.constraints) {
        lines.push(`    constraints: ${JSON.stringify(def.constraints)}`)
      }
      if (def.mcpSettable !== undefined) {
        lines.push(`    mcpSettable: ${String(def.mcpSettable)}`)
      }
      if (def.mcpAskFree) {
        lines.push('    mcpAskFree: true')
      }
      // `value` above is already what the setting READS as under the policy; this says why.
      if (isSettingManaged(id)) {
        lines.push('    managed: true')
      }
    }
  }

  // Include shortcuts
  lines.push('')
  lines.push('shortcuts:')
  for (const cmd of listedCommands()) {
    const shortcuts = getEffectiveShortcuts(cmd.id)
    const defaults = getDefaultShortcuts(cmd.id)
    const modified = isShortcutModified(cmd.id)

    lines.push(`  - id: ${cmd.id}`)
    lines.push(`    name: ${JSON.stringify(cmd.name)}`)
    lines.push(`    scope: ${cmd.scope}`)
    lines.push(`    shortcuts: ${formatYamlValue(shortcuts)}`)
    lines.push(`    defaults: ${formatYamlValue(defaults)}`)
    lines.push(`    modified: ${String(modified)}`)
  }

  return lines.join('\n')
}

/** The one way a value gets into the YAML: masked, then JSON, which YAML reads as a flow scalar, so
 *  a quote or a newline in a value can't end its row and forge another one. */
function formatYamlValue(value: unknown): string {
  return JSON.stringify(maskSecrets(value))
}

async function handleGetAllSettings(event: { payload: GetAllSettingsPayload }): Promise<void> {
  const { requestId } = event.payload
  log.debug('Handling mcp-get-all-settings (requestId={requestId})', { requestId })

  try {
    const yaml = buildAllSettingsYaml()
    await emit('mcp-response', { requestId, ok: true, data: yaml })
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error)
    log.error('Failed to build settings YAML: {error}', { error: message })
    await emit('mcp-response', { requestId, ok: false, error: message })
  }
}

// ============================================================================
// mcp-set-setting handler (for set_setting tool)
// ============================================================================

interface SetSettingPayload {
  requestId: string
  settingId: string
  value: unknown
}

/** Why `set_setting` refused a write, carried in the response beside the sentence. */
type SetSettingRefusal = 'notSettableOverMcp' | 'onlyOffOverMcp' | 'managedByOrganization' | 'invalidValue'

/** What a client hears when the value it sent holds `cmdr://settings`'s mask. */
const HOLDS_THE_MASK = `it holds the placeholder ${MASKED}, which stands for a part of the value only the person can see. Ask them to change it in Cmdr itself.`

async function handleSetSetting(event: { payload: SetSettingPayload }): Promise<void> {
  const { requestId, settingId, value } = event.payload
  // ❌ Never the value: a tool's argument stays out of the logs.
  log.debug('Handling mcp-set-setting: {settingId}', { settingId })

  // A setting that records a person's consent answer is theirs alone to change: an AI client
  // must never undo it. Decided by the registry's typed `mcpSettable`, ❌ never an id match.
  const mcpSettable = getSettingDefinition(settingId)?.mcpSettable
  if (mcpSettable === false) {
    const refusal: SetSettingRefusal = 'notSettableOverMcp'
    log.warn('Refused an MCP write to {settingId}: it records a consent answer', { settingId })
    await emit('mcp-response', {
      requestId,
      ok: false,
      refusal,
      error: `'${settingId}' records a person's consent answer, so it can't be set over MCP. Only the person can change it, in Cmdr itself.`,
    })
    return
  }

  // A data-sharing switch: a client may help someone opt out, but only the person opts in.
  if (mcpSettable === 'offOnly' && value !== false) {
    const refusal: SetSettingRefusal = 'onlyOffOverMcp'
    log.warn('Refused an MCP write to {settingId}: a client may only turn it off', { settingId })
    await emit('mcp-response', {
      requestId,
      ok: false,
      refusal,
      error: `'${settingId}' records a person's data-sharing choice, so over MCP it can only be turned off (false). Only the person can turn it on, in Cmdr itself.`,
    })
    return
  }

  // `cmdr://settings` shows a masked form, so a client copying a value it read would write the mask.
  // Here as well as in the question, since direct mode skips the question.
  if (holdsMaskedPlaceholder(value)) {
    const refusal: SetSettingRefusal = 'invalidValue'
    log.warn('Refused an MCP write to {settingId}: it holds the mask', { settingId })
    await emit('mcp-response', { requestId, ok: false, refusal, error: `'${settingId}': ${HOLDS_THE_MASK}` })
    return
  }

  // The organization's policy. The backend refuses first (`managed_policy::refuses_write`), so
  // this only catches a policy change racing the round trip; `setSetting` would refuse silently.
  if (!lockAllowsWrite(getSettingLock(settingId), value)) {
    const refusal: SetSettingRefusal = 'managedByOrganization'
    log.info('Refused an MCP write to {settingId}: the organization manages it', { settingId })
    await emit('mcp-response', {
      requestId,
      ok: false,
      refusal,
      error: `'${settingId}' is managed by the organization's policy on this Mac, so it can't be changed here.`,
    })
    return
  }

  try {
    setSetting(settingId as SettingId, value as SettingsValues[SettingId])
    await emit('mcp-response', { requestId, ok: true })
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error)
    log.error('Failed to set setting via MCP: {error}', { error: message })
    await emit('mcp-response', { requestId, ok: false, error: message })
  }
}

// ============================================================================
// mcp-describe-setting handler (what set_setting's question reads)
// ============================================================================

interface DescribeSettingPayload {
  requestId: string
  settingId: string
  value: unknown
}

/** An enum option's label for `value`, so the question can name it (`null` when the setting has no
 * options, or this value isn't one of them). */
function optionLabel(def: SettingDefinition, value: unknown): string | null {
  return def.constraints?.options?.find((option) => option.value === value)?.label ?? null
}

/** What the registry says about a setting and this value, for `tools/ask_plans.rs` to decide on. */
function describeSetting(settingId: string, value: unknown): Record<string, unknown> | null {
  const def = getSettingDefinition(settingId)
  if (!def) return null
  const sensitive = isSensitive(def)
  const current: unknown = getSetting(settingId as SettingId)
  let invalid: string | null = null
  try {
    validateSettingWrite(settingId, value)
  } catch (error) {
    invalid = error instanceof Error ? error.message : String(error)
  }
  // `cmdr://settings` shows a masked form, so a client copying a value it read would write the mask.
  if (invalid === null && holdsMaskedPlaceholder(value)) invalid = HOLDS_THE_MASK

  return {
    label: def.label,
    // ❌ Never a secret's real form: this crosses to the backend and goes up in a dialog.
    oldValue: sensitive ? maskValue(def, current) : (current ?? null),
    askFree: def.mcpAskFree === true,
    mcpSettable: def.mcpSettable ?? true,
    sensitive,
    locked: !lockAllowsWrite(getSettingLock(settingId), value),
    invalid,
    // Labels are display only, and a secret's never leave: the question shows its mask alone.
    oldValueLabel: sensitive ? null : optionLabel(def, current),
    newValueLabel: sensitive ? null : optionLabel(def, value),
    valueNamesApp: def.valueNamesApp === true,
  }
}

async function handleDescribeSetting(event: { payload: DescribeSettingPayload }): Promise<void> {
  const { requestId, settingId, value } = event.payload
  await emit('mcp-response', { requestId, ok: true, setting: describeSetting(settingId, value) })
}

// ============================================================================
// Setup / Cleanup
// ============================================================================

/** Register all MCP settings event listeners. Call in onMount of the main window. */
export async function setupMcpMainBridge(): Promise<void> {
  unlistenFns.push(await listen<GetAllSettingsPayload>('mcp-get-all-settings', (e) => void handleGetAllSettings(e)))
  unlistenFns.push(await listen<SetSettingPayload>('mcp-set-setting', (e) => void handleSetSetting(e)))
  unlistenFns.push(await listen<DescribeSettingPayload>('mcp-describe-setting', (e) => void handleDescribeSetting(e)))

  log.debug('MCP main bridge listeners set up')
}

/** Remove all MCP settings event listeners. Call in onDestroy of the main window. */
export function cleanupMcpMainBridge(): void {
  for (const unlisten of unlistenFns) {
    unlisten()
  }
  unlistenFns = []
  log.debug('MCP main bridge listeners cleaned up')
}
