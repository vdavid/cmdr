/**
 * Settings registry - single source of truth for all settings.
 */

import type { SettingDefinition, SettingId, SettingsValues } from './types'
import { SettingValidationError } from './types'
import { isMacOS } from '$lib/shortcuts/key-capture'
import { cloudProviderPresets } from './cloud-providers'

// ============================================================================
// Settings Definitions
// ============================================================================

export const settingsRegistry: SettingDefinition[] = [
  // ========================================================================
  // General › Appearance
  // ========================================================================
  {
    id: 'appearance.appColor',
    section: ['General', 'Appearance'],
    label: 'App color',
    description: isMacOS()
      ? 'To change your system theme color, go to System Settings > Appearance.'
      : 'To change your system theme color, open your desktop appearance settings.',
    keywords: ['color', 'accent', 'theme', 'gold', 'system', 'brand'],
    type: 'enum',
    default: 'system',
    component: 'radio',
    constraints: {
      options: [
        { value: 'system', label: 'System theme color' },
        { value: 'cmdr-gold', label: 'Cmdr gold' },
      ],
    },
  },
  {
    id: 'appearance.uiDensity',
    section: ['General', 'Appearance'],
    label: 'UI density',
    description: 'Controls the spacing and size of UI elements throughout the app.',
    keywords: ['compact', 'comfortable', 'spacious', 'size', 'spacing', 'dense'],
    type: 'enum',
    default: 'comfortable',
    component: 'toggle-group',
    constraints: {
      options: [
        { value: 'compact', label: 'Compact' },
        { value: 'comfortable', label: 'Comfortable' },
        { value: 'spacious', label: 'Spacious' },
      ],
    },
  },
  {
    id: 'appearance.useAppIconsForDocuments',
    section: ['General', 'Appearance'],
    label: 'Use app icons for documents',
    description:
      "Show the app's icon for documents instead of generic file type icons. More colorful but slightly slower.",
    keywords: ['icon', 'document', 'file', 'app', 'colorful', 'finder'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    id: 'appearance.fileSizeFormat',
    section: ['General', 'Appearance'],
    label: 'File size format',
    description: 'How to display file sizes in the file list.',
    keywords: ['size', 'bytes', 'binary', 'decimal', 'kb', 'mb', 'kib', 'mib'],
    type: 'enum',
    default: 'binary',
    component: 'select',
    constraints: {
      options: [
        { value: 'binary', label: 'Binary (KiB, MiB, GiB)', description: '1 KiB = 1024 bytes' },
        { value: 'si', label: 'SI decimal (KB, MB, GB)', description: '1 KB = 1000 bytes' },
      ],
    },
  },
  {
    id: 'appearance.dateTimeFormat',
    section: ['General', 'Appearance'],
    label: 'Date and time format',
    description: 'How to display dates and times in the file list.',
    keywords: ['date', 'time', 'format', 'iso', 'custom', 'timestamp'],
    type: 'enum',
    default: 'system',
    component: 'radio',
    constraints: {
      options: [
        { value: 'system', label: 'System default' },
        { value: 'iso', label: 'ISO 8601', description: 'e.g., 2025-01-25 14:30' },
        { value: 'short', label: 'Short', description: 'e.g., Jan 25, 2:30 PM' },
        { value: 'custom', label: 'Custom...' },
      ],
      allowCustom: true,
    },
  },
  {
    id: 'appearance.customDateTimeFormat',
    section: ['General', 'Appearance'],
    label: 'Custom date/time format',
    description: 'Format string for custom date/time display. Use placeholders like YYYY, MM, DD, HH, mm, ss.',
    keywords: ['custom', 'format', 'date', 'time', 'placeholder'],
    type: 'string',
    default: 'YYYY-MM-DD HH:mm',
    component: 'text-input',
  },

  // ========================================================================
  // General › Listing
  // ========================================================================
  {
    id: 'listing.directorySortMode',
    section: ['General', 'Listing'],
    label: 'Sort directories',
    description: 'How directories are sorted when changing the sort column.',
    keywords: ['sort', 'directory', 'folder', 'order', 'listing', 'name', 'size'],
    type: 'enum',
    default: 'likeFiles',
    component: 'toggle-group',
    constraints: {
      options: [
        { value: 'likeFiles', label: 'Like files' },
        { value: 'alwaysByName', label: 'Always by name' },
      ],
    },
  },

  {
    id: 'listing.sizeDisplay',
    section: ['General', 'Listing'],
    label: 'Size display',
    description:
      'Smart shows the smaller of content and on-disk size. This helps with disk images and compressed files where the two differ.',
    keywords: ['size', 'display', 'logical', 'physical', 'smart', 'disk', 'content', 'sparse'],
    type: 'enum',
    default: 'smart',
    component: 'toggle-group',
    constraints: {
      options: [
        { value: 'smart', label: 'Smart' },
        { value: 'logical', label: 'Content' },
        { value: 'physical', label: 'On disk' },
      ],
    },
  },

  {
    id: 'listing.sizeMismatchWarning',
    section: ['General', 'Listing'],
    label: 'Size mismatch warning',
    description: 'Shows a warning icon on folders where content and on-disk sizes differ by more than 50% and 200 MB.',
    keywords: ['size', 'mismatch', 'warning', 'alert', 'disk', 'content', 'difference'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },

  {
    id: 'listing.stripedRows',
    section: ['General', 'Listing'],
    label: 'Striped rows',
    description: 'Alternate row shading for easier line tracking. Applies to both Full and Brief view modes.',
    keywords: ['stripe', 'zebra', 'alternate', 'row', 'shading', 'accessibility', 'a11y'],
    type: 'boolean',
    default: false,
    component: 'switch',
  },

  // ========================================================================
  // General › File operations
  // ========================================================================
  {
    id: 'fileOperations.mtpEnabled',
    section: ['General', 'MTP'],
    label: 'Android/Kindle/camera support (PTP and MTP)',
    description:
      'Detect and connect to Android and other devices over a USB cable for file browsing and transfers. To use this feature on an Android phone, you\'ll want to use a USB cable, then on your phone, go to something like Settings > USB Preferences, and set the connection to "File transfer", "Android Auto", or similar. (Varies by device.)',
    keywords: ['mtp', 'android', 'usb', 'device', 'phone', 'ptpcamerad', 'mobile'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    id: 'fileOperations.mtpConnectionWarning',
    section: ['General', 'MTP'],
    label: 'Warn when a device connects',
    description: 'Show a notification when an Android or camera device connects over USB.',
    keywords: ['mtp', 'warning', 'notification', 'connect', 'toast', 'android'],
    type: 'boolean',
    default: true,
    component: 'checkbox',
  },
  {
    id: 'fileOperations.allowFileExtensionChanges',
    section: ['General', 'File operations'],
    label: 'Allow file extension changes',
    description: 'What to do when you rename a file and the extension changes.',
    keywords: ['extension', 'rename', 'file', 'change', 'ask', 'confirm'],
    type: 'enum',
    default: 'ask',
    component: 'radio',
    constraints: {
      options: [
        { value: 'yes', label: 'Always allow' },
        { value: 'no', label: 'Never allow' },
        { value: 'ask', label: 'Always ask' },
      ],
    },
  },
  {
    id: 'fileOperations.progressUpdateInterval',
    section: ['General', 'File operations'],
    label: 'Progress update interval',
    description:
      'How often to refresh progress during file operations. Lower values feel more responsive but use more CPU.',
    keywords: ['progress', 'update', 'interval', 'refresh', 'cpu', 'performance'],
    type: 'number',
    default: 500,
    component: 'slider',
    constraints: {
      min: 50,
      max: 5000,
      step: 50,
      sliderStops: [100, 250, 500, 1000, 2000],
    },
  },
  {
    id: 'fileOperations.maxConflictsToShow',
    section: ['General', 'File operations'],
    label: 'Maximum conflicts to show',
    description: 'Maximum number of file conflicts to display in the preview before an operation.',
    keywords: ['conflict', 'max', 'limit', 'preview', 'operation'],
    type: 'number',
    default: 100,
    component: 'select',
    constraints: {
      options: [
        { value: 1, label: '1' },
        { value: 2, label: '2' },
        { value: 3, label: '3' },
        { value: 5, label: '5' },
        { value: 10, label: '10' },
        { value: 50, label: '50' },
        { value: 100, label: '100' },
        { value: 200, label: '200' },
        { value: 500, label: '500' },
      ],
      allowCustom: true,
      customMin: 1,
      customMax: 1000,
    },
  },

  // ========================================================================
  // General › Updates
  // ========================================================================
  {
    id: 'updates.autoCheck',
    section: ['General', 'Updates'],
    label: 'Automatically check for updates',
    description: 'Periodically check for new versions in the background.',
    keywords: ['update', 'auto', 'check', 'version', 'background'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    id: 'updates.crashReports',
    section: ['General', 'Updates'],
    label: 'Send crash reports',
    description:
      'Automatically send crash reports when Cmdr quits unexpectedly. Includes app version, macOS version, and crash location — no file names or personal data.',
    keywords: ['crash', 'report', 'privacy', 'telemetry', 'bug', 'error'],
    type: 'boolean',
    default: false,
    component: 'switch',
  },

  // ========================================================================
  // Network › SMB/Network shares
  // ========================================================================
  {
    id: 'network.directSmbConnection',
    section: ['Network', 'SMB/Network shares'],
    label: 'Connect directly to SMB shares',
    description:
      'When enabled, Cmdr establishes a direct connection to SMB shares for faster file operations. The system mount stays for Finder and other apps.',
    keywords: ['smb', 'direct', 'fast', 'connection', 'network', 'performance', 'smb2'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    id: 'network.shareCacheDuration',
    section: ['Network', 'SMB/Network shares'],
    label: 'Share cache duration',
    description: 'How long to cache the list of available shares on a server before refreshing.',
    keywords: ['cache', 'smb', 'share', 'network', 'refresh', 'ttl'],
    type: 'duration',
    default: 30000, // 30 seconds in ms
    component: 'select',
    constraints: {
      unit: 's',
      options: [
        { value: 30000, label: '30 seconds' },
        { value: 300000, label: '5 minutes' },
        { value: 3600000, label: '1 hour' },
        { value: 86400000, label: '1 day' },
        { value: 2592000000, label: '30 days' },
      ],
      allowCustom: true,
      customMin: 1000,
      customMax: 2592000000,
    },
  },
  {
    id: 'network.timeoutMode',
    section: ['Network', 'SMB/Network shares'],
    label: 'Network timeout mode',
    description: 'How long to wait when connecting to network shares.',
    keywords: ['timeout', 'network', 'slow', 'vpn', 'connection', 'latency'],
    type: 'enum',
    default: 'normal',
    component: 'radio',
    constraints: {
      options: [
        { value: 'normal', label: 'Normal', description: 'For typical local networks (15s timeout)' },
        {
          value: 'slow',
          label: 'Slow network',
          description: 'For VPNs or high-latency connections (45s timeout)',
        },
        { value: 'custom', label: 'Custom' },
      ],
      allowCustom: true,
    },
  },
  {
    id: 'network.customTimeout',
    section: ['Network', 'SMB/Network shares'],
    label: 'Custom timeout',
    description: 'Custom timeout in seconds for network operations.',
    keywords: ['timeout', 'custom', 'seconds'],
    type: 'number',
    default: 15,
    component: 'number-input',
    constraints: {
      min: 5,
      max: 120,
      step: 1,
    },
  },

  // ========================================================================
  // Themes
  // ========================================================================
  {
    id: 'theme.mode',
    section: ['Themes'],
    label: 'Theme mode',
    description: 'Choose between light, dark, or system-based theme.',
    keywords: ['theme', 'dark', 'light', 'mode', 'appearance', 'color'],
    type: 'enum',
    default: 'system',
    component: 'toggle-group',
    constraints: {
      options: [
        { value: 'light', label: '☀️ Light' },
        { value: 'dark', label: '🌙 Dark' },
        { value: 'system', label: '💻 System' },
      ],
    },
  },

  // ========================================================================
  // General > Drive indexing
  // ========================================================================
  {
    id: 'indexing.enabled',
    section: ['General', 'Drive indexing'],
    label: 'Drive indexing',
    description: 'Index your drive in the background for instant directory sizes.',
    keywords: ['index', 'drive', 'scan', 'size', 'directory', 'folder', 'background'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },

  // ========================================================================
  // Viewer
  // ========================================================================
  {
    id: 'viewer.wordWrap',
    section: ['General', 'Viewer'],
    label: 'Word wrap',
    description: 'Wrap long lines at the window edge in the file viewer instead of scrolling horizontally.',
    keywords: ['viewer', 'wrap', 'word', 'line', 'horizontal', 'scroll'],
    type: 'boolean',
    default: false,
    component: 'switch',
  },

  // ========================================================================
  // AI
  // ========================================================================
  {
    id: 'ai.provider',
    section: ['AI'],
    label: 'Provider',
    description: 'Choose how AI features are powered.',
    keywords: ['ai', 'provider', 'openai', 'local', 'llm', 'off', 'model'],
    type: 'enum',
    default: 'local',
    component: 'toggle-group',
    constraints: {
      options: [
        { value: 'off', label: 'Off' },
        { value: 'openai-compatible', label: 'Cloud / API' },
        { value: 'local', label: 'Local LLM' },
      ],
    },
  },
  {
    id: 'ai.openaiApiKey',
    section: ['AI'],
    label: 'API key',
    description: 'Your OpenAI-compatible API key.',
    keywords: ['api', 'key', 'openai', 'secret', 'token'],
    type: 'string',
    default: '',
    component: 'password-input',
  },
  {
    id: 'ai.openaiBaseUrl',
    section: ['AI'],
    label: 'Base URL',
    description: 'API endpoint. Change this for Groq, Together AI, Azure OpenAI, or a local server.',
    keywords: ['url', 'endpoint', 'base', 'api', 'groq', 'together', 'azure', 'ollama'],
    type: 'string',
    default: 'https://api.openai.com/v1',
    component: 'text-input',
  },
  {
    id: 'ai.openaiModel',
    section: ['AI'],
    label: 'Model',
    description: 'The model name to use for completions.',
    keywords: ['model', 'gpt', 'openai', 'name'],
    type: 'string',
    default: 'gpt-4o-mini',
    component: 'text-input',
  },
  {
    id: 'ai.cloudProvider',
    section: ['AI'],
    label: 'Service',
    description: 'Which cloud AI service to use.',
    keywords: [
      'cloud',
      'provider',
      'service',
      'openai',
      'anthropic',
      'groq',
      'together',
      'fireworks',
      'mistral',
      'ollama',
      'deepseek',
      'xai',
      'perplexity',
      'openrouter',
      'gemini',
      'azure',
      'lm-studio',
      'custom',
    ],
    type: 'enum',
    default: 'openai',
    component: 'select',
    constraints: {
      options: cloudProviderPresets.map((p) => ({ value: p.id, label: p.name })),
    },
  },
  {
    id: 'ai.cloudProviderConfigs',
    section: ['AI'],
    label: 'Provider configurations',
    description: 'Per-provider API keys and model settings.',
    keywords: [],
    type: 'string',
    default: '{}',
    component: 'text-input',
  },
  {
    id: 'ai.localContextSize',
    section: ['AI'],
    label: 'Context window',
    description: 'Number of tokens the local model can process at once. Larger values use more memory.',
    keywords: ['context', 'window', 'tokens', 'memory', 'size', 'local'],
    type: 'enum',
    default: '4096',
    component: 'select',
    constraints: {
      options: [
        { value: '2048', label: '2048' },
        { value: '4096', label: '4096' },
        { value: '8192', label: '8192' },
        { value: '16384', label: '16384' },
        { value: '32768', label: '32768' },
        { value: '65536', label: '65536' },
        { value: '131072', label: '131072' },
        { value: '262144', label: '262144' },
      ],
    },
  },

  // ========================================================================
  // Developer › MCP server
  // ========================================================================
  {
    id: 'developer.mcpEnabled',
    section: ['Developer', 'MCP server'],
    label: 'Enable MCP server',
    description: 'Start a Model Context Protocol server for AI assistant integration.',
    keywords: ['mcp', 'server', 'ai', 'assistant', 'protocol', 'model'],
    type: 'boolean',
    default: false,
    component: 'switch',
  },
  {
    id: 'developer.mcpPort',
    section: ['Developer', 'MCP server'],
    label: 'Port',
    description: 'Preferred port for the MCP server. If in use, the next available port is used automatically.',
    keywords: ['port', 'mcp', 'network'],
    type: 'number',
    default: 9224,
    component: 'number-input',
    constraints: {
      min: 1024,
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
    label: 'Verbose logging',
    description: 'Log detailed debug information. Useful for troubleshooting. May impact performance.',
    keywords: ['log', 'debug', 'verbose', 'troubleshoot', 'performance'],
    type: 'boolean',
    default: false,
    component: 'switch',
  },

  // ========================================================================
  // Advanced (generated UI)
  // ========================================================================
  {
    id: 'advanced.dragThreshold',
    section: ['Advanced'],
    label: 'Drag threshold',
    description: 'Minimum distance in pixels before a drag operation starts.',
    keywords: ['drag', 'threshold', 'pixel', 'distance'],
    type: 'number',
    default: 5,
    component: 'number-input',
    showInAdvanced: true,
    constraints: {
      min: 1,
      max: 50,
      step: 1,
    },
  },
  {
    id: 'advanced.prefetchBufferSize',
    section: ['Advanced'],
    label: 'Prefetch buffer size',
    description: 'Number of items to prefetch around the visible range for smoother scrolling.',
    keywords: ['prefetch', 'buffer', 'scroll', 'performance'],
    type: 'number',
    default: 200,
    component: 'number-input',
    showInAdvanced: true,
    constraints: {
      min: 50,
      max: 1000,
      step: 50,
    },
  },
  {
    id: 'advanced.virtualizationBufferRows',
    section: ['Advanced'],
    label: 'Virtualization buffer (rows)',
    description: 'Extra rows to render above and below the visible area.',
    keywords: ['virtualization', 'buffer', 'row', 'render'],
    type: 'number',
    default: 20,
    component: 'number-input',
    showInAdvanced: true,
    constraints: {
      min: 5,
      max: 100,
      step: 5,
    },
  },
  {
    id: 'advanced.virtualizationBufferColumns',
    section: ['Advanced'],
    label: 'Virtualization buffer (columns)',
    description: 'Extra columns to render in brief view.',
    keywords: ['virtualization', 'buffer', 'column', 'brief'],
    type: 'number',
    default: 2,
    component: 'number-input',
    showInAdvanced: true,
    constraints: {
      min: 1,
      max: 10,
      step: 1,
    },
  },
  {
    id: 'advanced.fileWatcherDebounce',
    section: ['Advanced'],
    label: 'File watcher debounce',
    description: 'Delay after file system changes before refreshing.',
    keywords: ['watcher', 'debounce', 'refresh', 'delay'],
    type: 'duration',
    default: 200,
    component: 'duration',
    showInAdvanced: true,
    constraints: {
      unit: 'ms',
      minMs: 50,
      maxMs: 2000,
    },
  },
  {
    id: 'advanced.serviceResolveTimeout',
    section: ['Advanced'],
    label: 'Service resolve timeout',
    description: 'Timeout for resolving network services via Bonjour.',
    keywords: ['bonjour', 'resolve', 'timeout', 'mdns'],
    type: 'duration',
    default: 5000,
    component: 'duration',
    showInAdvanced: true,
    constraints: {
      unit: 's',
      minMs: 1000,
      maxMs: 30000,
    },
  },
  {
    id: 'advanced.mountTimeout',
    section: ['Advanced'],
    label: 'Mount timeout',
    description: 'Timeout for mounting network shares.',
    keywords: ['mount', 'timeout', 'network', 'share'],
    type: 'duration',
    default: 20000,
    component: 'duration',
    showInAdvanced: true,
    constraints: {
      unit: 's',
      minMs: 5000,
      maxMs: 120000,
    },
  },
  {
    id: 'advanced.updateCheckInterval',
    section: ['Advanced'],
    label: 'Update check interval',
    description: 'How often to check for updates in the background.',
    keywords: ['update', 'interval', 'background', 'check'],
    type: 'duration',
    default: 3600000, // 60 minutes
    component: 'duration',
    showInAdvanced: true,
    constraints: {
      unit: 'min',
      minMs: 300000, // 5 min
      maxMs: 86400000, // 24 hours
    },
  },
  {
    id: 'advanced.filterSafeSaveArtifacts',
    section: ['Advanced'],
    label: 'Filter safe-save artifacts on SMB',
    description:
      'Hide temporary files created by macOS safe-save (like ".sb-" files from TextEdit) in the SMB file watcher. These are transient and normally invisible.',
    keywords: ['smb', 'safe-save', 'artifact', 'temp', 'sb', 'filter', 'watcher'],
    type: 'boolean',
    default: true,
    component: 'switch',
    showInAdvanced: true,
  },
]

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
 * Get all settings marked for the Advanced section.
 */
export function getAdvancedSettings(): SettingDefinition[] {
  return settingsRegistry.filter((s) => s.showInAdvanced)
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
 * Build a hierarchical tree structure from the flat settings registry.
 */
export function buildSectionTree(): SettingsSection[] {
  const root: SettingsSection[] = []
  const sectionMap = new Map<string, SettingsSection>()

  for (const setting of settingsRegistry) {
    if (setting.showInAdvanced) continue // Advanced settings are handled separately

    let currentLevel = root
    let currentPath: string[] = []

    for (let i = 0; i < setting.section.length; i++) {
      const sectionName = setting.section[i]
      currentPath = [...currentPath, sectionName]
      const pathKey = currentPath.join('/')

      let section = sectionMap.get(pathKey)
      if (!section) {
        section = {
          name: sectionName,
          path: [...currentPath],
          subsections: [],
          settings: [],
        }
        sectionMap.set(pathKey, section)
        currentLevel.push(section)
      }

      if (i === setting.section.length - 1) {
        section.settings.push(setting)
      } else {
        currentLevel = section.subsections
      }
    }
  }

  return root
}
