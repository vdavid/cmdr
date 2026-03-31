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
import { settingsRegistry } from './settings-registry'
import { getSetting, setSetting, isModified } from './settings-store'
import type { SettingDefinition, SettingId, SettingsValues } from './types'
import { getEffectiveShortcuts, getDefaultShortcuts, isShortcutModified } from '$lib/shortcuts'
import { commands } from '$lib/commands/command-registry'
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('mcp-main-bridge')

let unlistenFns: UnlistenFn[] = []

// ============================================================================
// mcp-get-all-settings handler (for cmdr://settings resource)
// ============================================================================

interface GetAllSettingsPayload {
  requestId: string
}

const maskedPlaceholder = '********'

/** Returns true for settings that contain secrets (API keys, tokens, etc.). */
function isSensitive(def: SettingDefinition): boolean {
  return def.component === 'password-input' || def.id === 'ai.cloudProviderConfigs'
}

/** Mask a sensitive value for safe display. Password-input fields are fully masked.
 *  `ai.cloudProviderConfigs` is a JSON blob — we redact any `apiKey` fields inside it. */
function maskValue(def: SettingDefinition, value: unknown): unknown {
  if (def.component === 'password-input') {
    // Fully mask; show placeholder only when a value is actually set
    return typeof value === 'string' && value.length > 0 ? maskedPlaceholder : ''
  }

  if (def.id === 'ai.cloudProviderConfigs' && typeof value === 'string') {
    try {
      const parsed = JSON.parse(value) as Record<string, Record<string, unknown>>
      for (const providerConfig of Object.values(parsed)) {
        if (typeof providerConfig.apiKey === 'string' && providerConfig.apiKey.length > 0) {
          providerConfig.apiKey = maskedPlaceholder
        }
      }
      return JSON.stringify(parsed)
    } catch {
      // Not valid JSON — mask the entire value to be safe
      return value.length > 2 ? maskedPlaceholder : value
    }
  }

  return value
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
      const id = def.id as SettingId
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

async function handleSetSetting(event: { payload: SetSettingPayload }): Promise<void> {
  const { requestId, settingId, value } = event.payload
  log.debug('Handling mcp-set-setting: {settingId} = {value}', { settingId, value })

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
