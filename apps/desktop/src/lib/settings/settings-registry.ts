/**
 * Settings registry - single source of truth for all settings.
 */

import type { EnumOption, SettingDefinition, SettingId, SettingsValues } from './types'
import { SettingValidationError, VOLUME_TINT_COLORS } from './types'
import { isMacOS } from '$lib/shortcuts/key-capture'
import { cloudProviderPresets } from './cloud-providers'

/** Options list for the three `appearance.tint{Local,Smb,Mtp}` settings. */
const TINT_COLOR_OPTIONS: EnumOption[] = [
  { value: 'none', label: 'No tint' },
  ...VOLUME_TINT_COLORS.map((c) => ({ value: c, label: c.charAt(0).toUpperCase() + c.slice(1) })),
]

// ============================================================================
// Settings Definitions
//
// Top-level section order is driven by the order entries appear here.
// `buildSectionTree()` uses first-appearance order for each (sub)section name.
// Special non-registry sections (Keyboard shortcuts, License, Advanced) are
// interleaved in `SettingsSidebar.svelte`.
// ============================================================================

export const settingsRegistry: SettingDefinition[] = [
  // ========================================================================
  // Appearance › Colors and formats
  // ========================================================================
  {
    id: 'theme.mode',
    section: ['Appearance', 'Colors and formats'],
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
  {
    id: 'appearance.appColor',
    section: ['Appearance', 'Colors and formats'],
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
    id: 'appearance.sizeColors',
    section: ['Appearance', 'Colors and formats'],
    label: 'Size colors',
    description:
      'Color file sizes in the file list by tier. Rainbow uses green/yellow/orange/red/purple. App uses shades of the app color.',
    keywords: ['size', 'color', 'tier', 'rainbow', 'app', 'highlight', 'kb', 'mb', 'gb', 'tb'],
    type: 'enum',
    default: 'none',
    component: 'toggle-group',
    constraints: {
      options: [
        { value: 'none', label: 'None' },
        { value: 'app', label: 'App' },
        { value: 'rainbow', label: 'Rainbow' },
      ],
    },
  },
  {
    id: 'appearance.dateColors',
    section: ['Appearance', 'Colors and formats'],
    label: 'Date colors',
    description:
      'Color modified dates in the file list by age. App fades older dates toward the default text color. Wilting goes green for fresh files, yellow for aging, and brown for old.',
    keywords: ['date', 'color', 'age', 'modified', 'wilting', 'app', 'fresh', 'old'],
    type: 'enum',
    default: 'none',
    component: 'toggle-group',
    constraints: {
      options: [
        { value: 'none', label: 'None' },
        { value: 'app', label: 'App' },
        { value: 'wilting', label: 'Wilting' },
      ],
    },
  },
  {
    id: 'appearance.dateTimeFormat',
    section: ['Appearance', 'Colors and formats'],
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
    section: ['Appearance', 'Colors and formats'],
    label: 'Custom date/time format',
    description:
      'Format string for custom date/time display. Use placeholders like YYYY, MM, DD, HH, mm, ss. Add a single `|` to split the date and time into two aligned columns (e.g. `YYYY-MM-DD | HH:mm`).',
    keywords: ['custom', 'format', 'date', 'time', 'placeholder'],
    type: 'string',
    default: 'YYYY-MM-DD | HH:mm',
    component: 'text-input',
  },
  {
    id: 'listing.stripedRows',
    section: ['Appearance', 'Colors and formats'],
    label: 'Striped rows',
    description: 'Alternate row shading for easier line tracking. Applies to both Full and Brief view modes.',
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
    label: 'Tint local-volume panes',
    description: 'Background tint applied to panes showing a local drive.',
    keywords: ['tint', 'pane', 'color', 'volume', 'local', 'background', 'highlight'],
    type: 'enum',
    default: 'none',
    constraints: { options: TINT_COLOR_OPTIONS },
  },
  {
    id: 'appearance.tintSmb',
    section: ['Appearance', 'Colors and formats'],
    label: 'Tint SMB/network panes',
    description: 'Background tint applied to panes showing an SMB or network share.',
    keywords: ['tint', 'pane', 'color', 'volume', 'smb', 'network', 'background', 'highlight'],
    type: 'enum',
    default: 'none',
    constraints: { options: TINT_COLOR_OPTIONS },
  },
  {
    id: 'appearance.tintMtp',
    section: ['Appearance', 'Colors and formats'],
    label: 'Tint MTP panes',
    description: 'Background tint applied to panes showing an Android, Kindle, or camera device.',
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
    label: 'Text size',
    description:
      'Scales text and UI throughout the app. Compounds with the macOS Accessibility text size — 100% means "exactly the system size".',
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

  // ========================================================================
  // Appearance › File and folder sizes
  // ========================================================================
  {
    id: 'listing.sizeDisplay',
    section: ['Appearance', 'File and folder sizes'],
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
    id: 'listing.sizeUnit',
    section: ['Appearance', 'File and folder sizes'],
    label: 'Size unit',
    description:
      'Dynamic picks the friendliest unit per file (1.02 MB). Fixed units make sizes apples-to-apples across the list. Bytes shows the exact count for precise comparison.',
    keywords: ['size', 'human', 'bytes', 'unit', 'format', 'raw', 'precise', 'kb', 'mb', 'gb', 'dynamic'],
    type: 'enum',
    default: 'dynamic',
    component: 'toggle-group',
    constraints: {
      options: [
        { value: 'dynamic', label: 'Dynamic' },
        { value: 'bytes', label: 'Bytes' },
        { value: 'kB', label: 'kB' },
        { value: 'MB', label: 'MB' },
        { value: 'GB', label: 'GB' },
      ],
    },
  },
  {
    id: 'appearance.fileSizeFormat',
    section: ['Appearance', 'File and folder sizes'],
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
    id: 'listing.sizeMismatchWarning',
    section: ['Appearance', 'File and folder sizes'],
    label: 'Size mismatch warning',
    description: 'Shows a warning icon on folders where content and on-disk sizes differ by more than 50% and 200 MB.',
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
    label: 'Use app icons for documents',
    description:
      "Show the app's icon for documents instead of generic file type icons. More colorful but slightly slower.",
    keywords: ['icon', 'document', 'file', 'app', 'colorful', 'finder'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    id: 'listing.directorySortMode',
    section: ['Appearance', 'Listing'],
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
    id: 'listing.briefColumnWidthMode',
    section: ['Appearance', 'Listing'],
    label: 'Maximum column width in Brief mode',
    description:
      'Limits how wide Brief mode columns can grow to fit long filenames. Columns are always capped at the pane width regardless; the chosen limit only kicks in when it would be smaller than the pane.',
    keywords: ['brief', 'column', 'width', 'max', 'maximum', 'limit', 'pane', 'shrink-wrap'],
    type: 'enum',
    default: 'paneWidth',
    component: 'radio',
    constraints: {
      options: [
        { value: 'paneWidth', label: 'Pane width (no limit)' },
        { value: 'limited', label: 'Limit to' },
      ],
    },
  },
  {
    id: 'listing.briefColumnWidthMaxPx',
    section: ['Appearance', 'Listing'],
    label: 'Brief column width limit',
    description: '',
    keywords: ['brief', 'column', 'width', 'max', 'maximum', 'limit', 'pixel', 'slider'],
    type: 'number',
    default: 400,
    component: 'slider',
    constraints: {
      min: 250,
      max: 1000,
      step: 25,
      sliderStops: [250, 400, 600, 800, 1000],
    },
  },

  // ========================================================================
  // Behavior › File operations
  // ========================================================================
  {
    id: 'fileOperations.allowFileExtensionChanges',
    section: ['Behavior', 'File operations'],
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

  // ========================================================================
  // Behavior › Drive indexing
  // ========================================================================
  {
    id: 'indexing.enabled',
    section: ['Behavior', 'Drive indexing'],
    label: 'Drive indexing',
    description: 'Index your drive in the background for instant directory sizes.',
    keywords: ['index', 'drive', 'scan', 'size', 'directory', 'folder', 'background'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },

  // ========================================================================
  // Behavior › Search
  // ========================================================================
  {
    id: 'search.autoApply',
    section: ['Behavior', 'Search'],
    label: 'Auto-apply searches',
    description:
      'Run filename and regex searches automatically as you type (1 second after you stop). AI searches always wait for Enter — they cost money. When off, press Enter or click the run button to search.',
    keywords: ['search', 'auto', 'apply', 'live', 'debounce', 'filename', 'regex', 'instant'],
    type: 'boolean',
    default: true,
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
    keywords: ['ai', 'provider', 'cloud', 'openai', 'anthropic', 'claude', 'gemini', 'local', 'llm', 'off', 'model'],
    type: 'enum',
    default: 'off',
    component: 'toggle-group',
    constraints: {
      options: [
        { value: 'off', label: 'Off' },
        { value: 'cloud', label: 'Cloud AI' },
        { value: 'local', label: 'Local LLM' },
      ],
    },
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
  // File systems › SMB/Network shares
  // ========================================================================
  {
    id: 'network.enabled',
    section: ['File systems', 'SMB/Network shares'],
    label: 'Enable networking',
    description:
      "Discover SMB servers on your local network and connect to them. When off, Cmdr can still read and write files on already-mounted shares, but won't ask macOS for Local Network access.",
    keywords: ['network', 'enable', 'enabled', 'smb', 'discovery', 'mdns', 'bonjour', 'local', 'permission'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    id: 'network.firstTriggerDone',
    section: ['File systems', 'SMB/Network shares'],
    label: 'Network discovery started',
    description: 'Internal: tracks whether discovery has been triggered at least once. Hidden from the UI.',
    keywords: [],
    type: 'boolean',
    default: false,
    component: 'switch',
    hidden: true,
  },
  {
    id: 'network.directSmbConnection',
    section: ['File systems', 'SMB/Network shares'],
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
    section: ['File systems', 'SMB/Network shares'],
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
    section: ['File systems', 'SMB/Network shares'],
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
    section: ['File systems', 'SMB/Network shares'],
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
  {
    id: 'network.smbConcurrency',
    section: ['File systems', 'SMB/Network shares'],
    label: 'Concurrent operations per SMB connection',
    description:
      'How many file transfers Cmdr runs in parallel on a single SMB connection. Higher values speed up batch copies of many files (especially many small files) but use more server resources. Default 10 is safe for most home NAS hardware. Changes apply on the next batch copy.',
    keywords: ['smb', 'concurrency', 'parallel', 'copy', 'batch', 'performance', 'transfer', 'speed'],
    type: 'number',
    default: 10,
    component: 'number-input',
    showInAdvanced: true,
    constraints: {
      min: 1,
      max: 32,
      step: 1,
    },
  },

  // ========================================================================
  // File systems › MTP (Android/Kindle/cameras)
  // ========================================================================
  {
    id: 'fileOperations.mtpEnabled',
    section: ['File systems', 'MTP (Android/Kindle/cameras)'],
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
    section: ['File systems', 'MTP (Android/Kindle/cameras)'],
    label: 'Warn when a device connects',
    description: 'Show a notification when an Android or camera device connects over USB.',
    keywords: ['mtp', 'warning', 'notification', 'connect', 'toast', 'android'],
    type: 'boolean',
    default: true,
    component: 'checkbox',
  },

  // ========================================================================
  // File systems › Git
  // ========================================================================
  {
    id: 'fileExplorer.git.showRepoChip',
    section: ['File systems', 'Git'],
    label: 'Show repository chip',
    description:
      'Display the current branch, ahead/behind, and dirty state above the file list when inside a git repository.',
    keywords: ['git', 'chip', 'branch', 'breadcrumb', 'repo', 'status', 'ahead', 'behind', 'dirty'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    id: 'fileExplorer.git.showStatusColumn',
    section: ['File systems', 'Git'],
    label: 'Show git status column',
    description: 'Add a column showing per-file git status (modified, untracked, etc.) in Full mode.',
    keywords: ['git', 'status', 'column', 'modified', 'untracked', 'ignored', 'added', 'deleted'],
    type: 'boolean',
    default: false,
    component: 'switch',
  },
  {
    id: 'fileExplorer.git.showVirtualGitPortal',
    section: ['File systems', 'Git'],
    label: 'Show virtual git portal',
    description:
      'When entering `.git`, show branches, tags, commits, and worktrees as browsable virtual folders. Disable to see the raw `.git` contents instead.',
    keywords: ['git', 'portal', 'virtual', 'branches', 'tags', 'commits', 'worktrees', 'history'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },

  // ========================================================================
  // Viewer
  // ========================================================================
  {
    id: 'viewer.wordWrap',
    section: ['Viewer'],
    label: 'Word wrap',
    description: 'Wrap long lines at the window edge in the file viewer instead of scrolling horizontally.',
    keywords: ['viewer', 'wrap', 'word', 'line', 'horizontal', 'scroll'],
    type: 'boolean',
    default: false,
    component: 'switch',
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
    // Dev and prod intentionally differ so a developer can run both side-by-side. Mirrors
    // `DEFAULT_PORT` in `apps/desktop/src-tauri/src/mcp/config.rs`. Both in 10000–29999 per
    // AGENTS.md no-standard-ports rule.
    default: import.meta.env.DEV ? 19225 : 19224,
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
    label: 'Verbose console output (developer)',
    description:
      'Bumps the dev terminal and browser devtools console to debug level. The on-disk log file always captures debug detail regardless, so error reports are unaffected by this toggle. RUST_LOG always wins for the terminal.',
    keywords: ['log', 'debug', 'verbose', 'troubleshoot', 'performance', 'console'],
    type: 'boolean',
    default: false,
    component: 'switch',
  },

  // ========================================================================
  // Updates
  // ========================================================================
  {
    id: 'updates.autoCheck',
    section: ['Updates'],
    label: 'Automatically check for updates',
    description: 'Periodically check for new versions in the background.',
    keywords: ['update', 'auto', 'check', 'version', 'background'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    id: 'updates.crashReports',
    section: ['Updates'],
    label: 'Send crash reports',
    description:
      'Automatically send crash reports when Cmdr quits unexpectedly. Includes app version, macOS version, and crash location. Never file names or personal data.',
    keywords: ['crash', 'report', 'privacy', 'telemetry', 'bug', 'error'],
    type: 'boolean',
    default: false,
    component: 'switch',
  },
  {
    id: 'updates.errorReports',
    section: ['Updates'],
    label: 'Send error reports automatically',
    description:
      'Send a small log snippet to the developer when an error occurs. Helps fix bugs faster. Off by default. You can always send a manual report from the Help menu.',
    keywords: ['error', 'report', 'auto', 'send', 'privacy', 'telemetry', 'bug', 'log', 'snippet', 'diagnostics'],
    type: 'boolean',
    default: false,
    component: 'switch',
  },

  // ========================================================================
  // Advanced (auto-generated UI, `showInAdvanced: true`).
  //
  // Entries below carry a `section` path purely for reference / search; they
  // are excluded from the section tree and rendered only inside the Advanced
  // section. `network.smbConcurrency` above and the two `fileOperations.*`
  // entries below (maxConflictsToShow, progressUpdateInterval) keep their
  // natural section path AND surface here.
  // ========================================================================
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
    id: 'advanced.diskSpaceChangeThreshold',
    section: ['Advanced'],
    label: 'Disk space change threshold (MB)',
    description:
      'Minimum change in available disk space before updating the status bar. The status bar polls disk space every few seconds; small changes below this threshold are ignored to reduce visual noise.',
    keywords: ['disk', 'space', 'threshold', 'poll', 'refresh', 'status', 'bar'],
    type: 'number',
    default: 1,
    component: 'number-input',
    showInAdvanced: true,
    constraints: {
      min: 0,
      max: 1000,
      step: 1,
    },
  },
  {
    id: 'fileViewer.suppressBinaryWarning',
    section: ['Advanced'],
    label: 'Suppress the raw-view warning for binary files',
    description:
      "F3 opens Cmdr's file viewer, which shows raw bytes (with lossy UTF-8 for non-text content). When you open an image, PDF, archive, or other binary file, the viewer shows a red banner explaining that ⇧Space (Quick Look) or Enter (open in the associated app) is probably what you wanted. Turn this on (or click 'Never show this warning again' in the banner) to suppress the warning for good.",
    keywords: ['viewer', 'binary', 'image', 'pdf', 'raw', 'warning', 'banner', 'f3', 'quick', 'look'],
    type: 'boolean',
    default: false,
    component: 'switch',
    showInAdvanced: true,
  },
  {
    id: 'fileExplorer.suppressQuickLookHint',
    section: ['Advanced'],
    label: 'Suppress the Space-key Quick Look hint',
    description:
      "Cmdr uses Space to toggle file selection and ⇧Space for Quick Look (Finder uses plain Space for Quick Look). Each time you press Space in the file list, Cmdr shows a one-paragraph reminder of this difference. Turn this on (or click 'Don't show again' in the toast) to suppress the reminder for good.",
    keywords: ['quick', 'look', 'preview', 'space', 'finder', 'hint', 'toast', 'reminder'],
    type: 'boolean',
    default: false,
    component: 'switch',
    showInAdvanced: true,
  },
  {
    id: 'fileExplorer.tabs.closedTabHistorySize',
    section: ['Advanced'],
    label: 'Number of closed tabs to remember per pane',
    description:
      'How many recently closed tabs to keep per pane for "Reopen closed tab" (⌘⇧T). Higher values let you reopen further back in history; lower values free up memory sooner. Only applies to tabs closed in the current session.',
    keywords: ['tab', 'closed', 'reopen', 'history', 'undo', 'pane'],
    type: 'number',
    default: 10,
    component: 'number-input',
    showInAdvanced: true,
    constraints: {
      min: 1,
      max: 50,
      step: 1,
    },
  },
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
    id: 'fileExplorer.typeToJump.resetDelay',
    section: ['Advanced'],
    label: 'Type-to-jump reset delay',
    description:
      'How long the type-to-jump buffer stays alive after the last keystroke. Lower values reset faster between searches; higher values are more forgiving for slow typists.',
    keywords: ['type', 'jump', 'reset', 'delay', 'fuzzy', 'search', 'navigation', 'keystroke', 'buffer'],
    type: 'number',
    default: 1000,
    component: 'number-input',
    showInAdvanced: true,
    constraints: {
      min: 300,
      max: 3000,
      step: 100,
    },
  },
  {
    id: 'fileOperations.maxConflictsToShow',
    section: ['Behavior', 'File operations'],
    label: 'Maximum conflicts to show',
    description: 'Maximum number of file conflicts to display in the preview before an operation.',
    keywords: ['conflict', 'max', 'limit', 'preview', 'operation'],
    type: 'number',
    default: 100,
    component: 'select',
    showInAdvanced: true,
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
  {
    id: 'fileOperations.progressUpdateInterval',
    section: ['Behavior', 'File operations'],
    label: 'Progress update interval',
    description:
      'How often to refresh progress during file operations. Lower values feel more responsive but use more CPU.',
    keywords: ['progress', 'update', 'interval', 'refresh', 'cpu', 'performance'],
    type: 'number',
    default: 500,
    component: 'slider',
    showInAdvanced: true,
    constraints: {
      min: 50,
      max: 5000,
      step: 50,
      sliderStops: [100, 250, 500, 1000, 2000],
    },
  },
  {
    id: 'advanced.maxLogStorageMb',
    section: ['Advanced'],
    label: 'Maximum disk space for log files (MB)',
    description:
      'Maximum disk space for log files. Set to 0 to disable log storage; error reports cannot be sent without logs. Changes to a non-zero value take effect on next app launch.',
    keywords: ['log', 'storage', 'disk', 'mb', 'cap', 'rotation', 'error', 'report', 'privacy'],
    type: 'number',
    default: 200,
    component: 'number-input',
    showInAdvanced: true,
    constraints: {
      min: 0,
      max: 5000,
      step: 50,
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
    id: 'search.recentSearches.maxCount',
    section: ['Advanced'],
    label: 'Recent searches to remember',
    description:
      'How many recent searches to keep for the in-dialog footer and history popover. Older entries roll off as new ones land. 0 disables history (the footer and popover hide and no new entries are recorded).',
    keywords: ['search', 'recent', 'history', 'cap', 'limit', 'max', 'count'],
    type: 'number',
    default: 1000,
    component: 'number-input',
    showInAdvanced: true,
    constraints: {
      min: 0,
      max: 10000,
      step: 1,
    },
  },
  {
    id: 'selection.recentSelections.maxCount',
    section: ['Advanced'],
    label: 'Recent selections to remember',
    description:
      'How many recent selections to keep for the Select / Deselect files dialog footer and history popover. Older entries roll off as new ones land. 0 disables history (the footer and popover hide and no new entries are recorded).',
    keywords: ['selection', 'select', 'recent', 'history', 'cap', 'limit', 'max', 'count'],
    type: 'number',
    default: 1000,
    component: 'number-input',
    showInAdvanced: true,
    constraints: {
      min: 0,
      max: 10000,
      step: 1,
    },
  },
  {
    id: 'onboarding.upgradeNudgeShown',
    section: ['Advanced'],
    label: 'Onboarding upgrade nudge shown',
    description:
      'Internal: tracks whether the one-time "the Onboarding menu item now exists" toast has fired for an existing user after the wizard revamp. Hidden from the UI.',
    keywords: [],
    type: 'boolean',
    default: false,
    component: 'switch',
    hidden: true,
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
  return settingsRegistry.filter((s) => s.showInAdvanced && !s.hidden)
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
    if (setting.hidden) continue // Internal-only settings (e.g., network.firstTriggerDone)

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
