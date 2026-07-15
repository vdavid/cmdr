/**
 * Developer section settings (data only). Logic lives in `../settings-registry.ts`,
 * which concatenates this array into the full registry in section order.
 */

import type { SettingDefinitionSource } from '../types'

export const developerSettings: SettingDefinitionSource[] = [
  // ========================================================================
  // Developer › MCP server
  // ========================================================================
  {
    id: 'developer.mcpEnabled',
    section: ['Developer', 'MCP server'],
    labelKey: 'settings.developer.mcpEnabled.label',
    descriptionKey: 'settings.developer.mcpEnabled.description',
    keywords: ['mcp', 'server', 'ai', 'assistant', 'protocol', 'model'],
    type: 'boolean',
    default: false,
    component: 'switch',
  },
  {
    id: 'developer.mcpPort',
    section: ['Developer', 'MCP server'],
    labelKey: 'settings.developer.mcpPort.label',
    descriptionKey: 'settings.developer.mcpPort.description',
    keywords: ['port', 'mcp', 'network', 'ephemeral'],
    type: 'number',
    // 0 = ephemeral. The backend binds 127.0.0.1:0 and writes the actual port to
    // `<data_dir>/mcp.port` so external clients can discover it. Pinning a non-zero port
    // is still supported for tooling that needs a fixed target. See
    // `docs/tooling/instance-isolation.md` § "Per-resource breakdown" (Cmdr MCP HTTP port row).
    default: 0,
    component: 'number-input',
    constraints: {
      min: 0,
      max: 65535,
      step: 1,
    },
  },

  // ========================================================================
  // Developer › Logging
  // ========================================================================
  {
    id: 'developer.verboseLogging',
    section: ['Developer', 'Logging'],
    labelKey: 'settings.developer.verboseLogging.label',
    descriptionKey: 'settings.developer.verboseLogging.description',
    keywords: ['log', 'debug', 'verbose', 'troubleshoot', 'performance', 'console'],
    type: 'boolean',
    default: false,
    component: 'switch',
  },
]
