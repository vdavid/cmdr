/**
 * File systems section settings (data only). Logic lives in `../settings-registry.ts`,
 * which concatenates this array into the full registry in section order.
 */

import type { SettingDefinitionSource } from '../types'

export const fileSystemsSettings: SettingDefinitionSource[] = [
  // ========================================================================
  // File systems › SMB/Network shares
  // ========================================================================
  {
    id: 'network.enabled',
    section: ['File systems', 'SMB/Network shares'],
    labelKey: 'settings.network.enabled.label',
    descriptionKey: 'settings.network.enabled.description',
    cardKey: 'settings.network.card.connection',
    keywordsKey: 'settings.network.enabled.keywords',
    terms: ['smb', 'mdns', 'bonjour'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    id: 'network.firstTriggerDone',
    section: ['File systems', 'SMB/Network shares'],
    labelKey: 'settings.network.firstTriggerDone.label',
    descriptionKey: 'settings.network.firstTriggerDone.description',
    type: 'boolean',
    default: false,
    component: 'switch',
    hidden: true,
  },
  {
    // Internal: hidden from the Settings UI. Remembers whether the person last
    // opened or collapsed the servers hub's "found nearby" group; `auto` until
    // they toggle it, which is when the hub picks by whether they saved a server.
    id: 'network.nearbyServersGroup',
    section: ['File systems', 'SMB/Network shares'],
    labelKey: 'settings.network.nearbyServersGroup.label',
    // A `string`, not an `enum`: an enum's options each need a rendered label, and
    // nothing ever renders these. `NearbyServersGroupChoice` keeps the three
    // tokens type-safe at every call site.
    type: 'string',
    default: 'auto',
    component: 'text-input',
    hidden: true,
  },
  {
    id: 'network.directSmbConnection',
    section: ['File systems', 'SMB/Network shares'],
    labelKey: 'settings.network.directSmbConnection.label',
    descriptionKey: 'settings.network.directSmbConnection.description',
    cardKey: 'settings.network.card.connection',
    keywordsKey: 'settings.network.directSmbConnection.keywords',
    terms: ['smb', 'smb2'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    id: 'network.shareCacheDuration',
    section: ['File systems', 'SMB/Network shares'],
    labelKey: 'settings.network.shareCacheDuration.label',
    descriptionKey: 'settings.network.shareCacheDuration.description',
    cardKey: 'settings.network.card.performanceAndTimeouts',
    keywordsKey: 'settings.network.shareCacheDuration.keywords',
    terms: ['smb', 'ttl'],
    type: 'duration',
    default: 30000, // 30 seconds in ms
    component: 'select',
    constraints: {
      unit: 's',
      options: [
        { value: 30000, labelKey: 'settings.network.shareCacheDuration.opt.s30' },
        { value: 300000, labelKey: 'settings.network.shareCacheDuration.opt.min5' },
        { value: 3600000, labelKey: 'settings.network.shareCacheDuration.opt.hour1' },
        { value: 86400000, labelKey: 'settings.network.shareCacheDuration.opt.day1' },
        { value: 2592000000, labelKey: 'settings.network.shareCacheDuration.opt.days30' },
      ],
      allowCustom: true,
      customMin: 1000,
      customMax: 2592000000,
    },
  },
  {
    id: 'network.timeoutMode',
    section: ['File systems', 'SMB/Network shares'],
    labelKey: 'settings.network.timeoutMode.label',
    descriptionKey: 'settings.network.timeoutMode.description',
    cardKey: 'settings.network.card.performanceAndTimeouts',
    keywordsKey: 'settings.network.timeoutMode.keywords',
    terms: ['vpn'],
    type: 'enum',
    default: 'normal',
    component: 'radio',
    constraints: {
      options: [
        {
          value: 'normal',
          labelKey: 'settings.network.timeoutMode.opt.normal',
          descriptionKey: 'settings.network.timeoutMode.optDesc.normal',
        },
        {
          value: 'slow',
          labelKey: 'settings.network.timeoutMode.opt.slow',
          descriptionKey: 'settings.network.timeoutMode.optDesc.slow',
        },
        { value: 'custom', labelKey: 'settings.network.timeoutMode.opt.custom' },
      ],
      allowCustom: true,
    },
  },
  {
    id: 'network.customTimeout',
    section: ['File systems', 'SMB/Network shares'],
    labelKey: 'settings.network.customTimeout.label',
    descriptionKey: 'settings.network.customTimeout.description',
    cardKey: 'settings.network.card.performanceAndTimeouts',
    keywordsKey: 'settings.network.customTimeout.keywords',
    type: 'number',
    default: 15,
    component: 'number-input',
    constraints: {
      min: 5,
      max: 120,
      step: 1,
    },
  },

  // ❗ `File systems › Servers (SFTP, WebDAV)` has NO entry here, on purpose. Its
  // whole content is the trusted-host-key list, one Forget per row, and the saved
  // servers live in the hub rather than here
  // (`src/lib/file-explorer/network/DETAILS.md` § `ServersHub.svelte` says why a
  // pane state and not a Settings list). Its sidebar row and its search hits come
  // from `sections/ServersSection.rows.ts`.

  // ========================================================================
  // File systems › MTP (Android/Kindle/cameras)
  // ========================================================================
  {
    id: 'fileOperations.mtpEnabled',
    section: ['File systems', 'MTP (Android/Kindle/cameras)'],
    labelKey: 'settings.fileOperations.mtpEnabled.label',
    descriptionKey: 'settings.fileOperations.mtpEnabled.description',
    keywordsKey: 'settings.fileOperations.mtpEnabled.keywords',
    terms: ['mtp', 'android', 'usb', 'ptpcamerad'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    id: 'fileOperations.mtpConnectionWarning',
    section: ['File systems', 'MTP (Android/Kindle/cameras)'],
    labelKey: 'settings.fileOperations.mtpConnectionWarning.label',
    descriptionKey: 'settings.fileOperations.mtpConnectionWarning.description',
    keywordsKey: 'settings.fileOperations.mtpConnectionWarning.keywords',
    terms: ['mtp', 'android'],
    type: 'boolean',
    default: true,
    component: 'checkbox',
  },

  // ========================================================================
  // File systems › Android (ADB)
  //
  // Both are live-applied together (`adb-settings.ts` re-reads and pushes both
  // through `set_adb_settings`, wired in `settings-applier.ts`), because the
  // backend restarts the device tracker under whichever binary the path names.
  // `AdbSection.svelte` renders them, plus the status, Re-check, and Browse rows,
  // which are not settings.
  // ========================================================================
  {
    id: 'fileOperations.adbEnabled',
    section: ['File systems', 'Android (ADB)'],
    labelKey: 'settings.fileOperations.adbEnabled.label',
    descriptionKey: 'settings.fileOperations.adbEnabled.description',
    keywordsKey: 'settings.fileOperations.adbEnabled.keywords',
    terms: ['adb', 'android', 'platform-tools'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    id: 'fileOperations.adbBinaryPath',
    section: ['File systems', 'Android (ADB)'],
    labelKey: 'settings.fileOperations.adbBinaryPath.label',
    descriptionKey: 'settings.fileOperations.adbBinaryPath.description',
    keywordsKey: 'settings.fileOperations.adbBinaryPath.keywords',
    terms: ['adb', 'android', 'platform-tools', 'sdk'],
    type: 'string',
    default: '',
    component: 'text-input',
  },

  // ========================================================================
  // File systems › Git
  // ========================================================================
  {
    id: 'fileExplorer.git.showRepoChip',
    section: ['File systems', 'Git'],
    labelKey: 'settings.fileExplorer.git.showRepoChip.label',
    descriptionKey: 'settings.fileExplorer.git.showRepoChip.description',
    keywordsKey: 'settings.fileExplorer.git.showRepoChip.keywords',
    terms: ['git'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    id: 'fileExplorer.git.showStatusColumn',
    section: ['File systems', 'Git'],
    labelKey: 'settings.fileExplorer.git.showStatusColumn.label',
    descriptionKey: 'settings.fileExplorer.git.showStatusColumn.description',
    keywordsKey: 'settings.fileExplorer.git.showStatusColumn.keywords',
    terms: ['git'],
    type: 'boolean',
    default: false,
    component: 'switch',
  },
  {
    id: 'fileExplorer.git.showVirtualGitPortal',
    section: ['File systems', 'Git'],
    labelKey: 'settings.fileExplorer.git.showVirtualGitPortal.label',
    descriptionKey: 'settings.fileExplorer.git.showVirtualGitPortal.description',
    keywordsKey: 'settings.fileExplorer.git.showVirtualGitPortal.keywords',
    terms: ['git'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    // Internal (FE-owned): whether the once-ever `.git` portal intro toast has
    // fired (`file-explorer/git/git-portal-intro.ts`). No UI row; hidden from
    // search and the section tree, the way `behavior.openTerminalHereToastSeen` is.
    id: 'fileExplorer.git.portalIntroSeen',
    section: ['File systems', 'Git'],
    labelKey: 'settings.fileExplorer.git.portalIntroSeen.label',
    descriptionKey: 'settings.fileExplorer.git.portalIntroSeen.description',
    type: 'boolean',
    default: false,
    component: 'switch',
    hidden: true,
  },
]
