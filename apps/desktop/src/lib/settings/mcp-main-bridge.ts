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
import { getSettingDefinition, settingsRegistry } from './settings-registry'
import { getSetting, setSetting, isModified } from './settings-store'
import type { SettingDefinition, SettingId, SettingsValues } from './types'
import { getEffectiveShortcuts, getDefaultShortcuts, isShortcutModified } from '$lib/shortcuts'
import { commands } from '$lib/commands/command-registry'
import { getAppLogger } from '$lib/logging/logger'
import { getSettingLock, isSettingManaged } from '$lib/managed-policy/managed-policy.svelte'
import { lockAllowsWrite } from '$lib/managed-policy/overlay'

const log = getAppLogger('mcp-main-bridge')

let unlistenFns: UnlistenFn[] = []

// ============================================================================
// mcp-get-all-settings handler (for cmdr://settings resource)
// ============================================================================

interface GetAllSettingsPayload {
  requestId: string
}

const maskedPlaceholder = '********'

/** Returns true for settings that contain secrets (API keys, tokens, etc.). AI provider API keys
 *  live in the OS secret store now, so `ai.cloudProviderConfigs` no longer needs special-case
 *  redaction: it only holds model and base URL. */
function isSensitive(def: SettingDefinition): boolean {
  return def.component === 'password-input'
}

/** Mask a sensitive value for safe display. */
function maskValue(_def: SettingDefinition, value: unknown): unknown {
  return typeof value === 'string' && value.length > 0 ? maskedPlaceholder : ''
}

/** Build a YAML representation of all settings grouped by section. */
function buildAllSettingsYaml(): string {
  const lines: string[] = []

  // Group settings by top-level section
  const sectionMap = new Map<string, typeof settingsRegistry>()
  for (const def of settingsRegistry) {
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
      const displayDefault = sensitive ? maskValue(def, def.default) : def.default

      lines.push(`  - id: ${def.id}`)
      lines.push(`    label: "${def.label}"`)
      lines.push(`    type: ${def.type}`)
      lines.push(`    value: ${formatYamlValue(displayValue)}`)
      lines.push(`    default: ${formatYamlValue(displayDefault)}`)
      lines.push(`    modified: ${String(modified)}`)
      if (def.constraints) {
        lines.push(`    constraints: ${JSON.stringify(def.constraints)}`)
      }
      if (def.mcpSettable === false) {
        lines.push('    mcpSettable: false')
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
  for (const cmd of commands) {
    const shortcuts = getEffectiveShortcuts(cmd.id)
    const defaults = getDefaultShortcuts(cmd.id)
    const modified = isShortcutModified(cmd.id)

    lines.push(`  - id: ${cmd.id}`)
    lines.push(`    name: "${cmd.name}"`)
    lines.push(`    scope: ${cmd.scope}`)
    lines.push(`    shortcuts: [${shortcuts.map((s) => `"${s}"`).join(', ')}]`)
    lines.push(`    defaults: [${defaults.map((s) => `"${s}"`).join(', ')}]`)
    lines.push(`    modified: ${String(modified)}`)
  }

  return lines.join('\n')
}

function formatYamlValue(value: unknown): string {
  if (typeof value === 'string') return `"${value}"`
  if (typeof value === 'boolean' || typeof value === 'number') return String(value)
  return JSON.stringify(value)
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
type SetSettingRefusal = 'notSettableOverMcp' | 'managedByOrganization'

async function handleSetSetting(event: { payload: SetSettingPayload }): Promise<void> {
  const { requestId, settingId, value } = event.payload
  log.debug('Handling mcp-set-setting: {settingId} = {value}', { settingId, value })

  // A setting that records a person's consent answer is theirs alone to change: an AI client
  // must never undo it. Decided by the registry's typed `mcpSettable`, ❌ never an id match.
  if (getSettingDefinition(settingId)?.mcpSettable === false) {
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
// Setup / Cleanup
// ============================================================================

/** Register all MCP settings event listeners. Call in onMount of the main window. */
export async function setupMcpMainBridge(): Promise<void> {
  unlistenFns.push(await listen<GetAllSettingsPayload>('mcp-get-all-settings', (e) => void handleGetAllSettings(e)))
  unlistenFns.push(await listen<SetSettingPayload>('mcp-set-setting', (e) => void handleSetSetting(e)))

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
