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
    keywords: ['network', 'enable', 'enabled', 'smb', 'discovery', 'mdns', 'bonjour', 'local', 'permission'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    id: 'network.firstTriggerDone',
    section: ['File systems', 'SMB/Network shares'],
    labelKey: 'settings.network.firstTriggerDone.label',
    descriptionKey: 'settings.network.firstTriggerDone.description',
    keywords: [],
    type: 'boolean',
    default: false,
    component: 'switch',
    hidden: true,
  },
  {
    id: 'network.directSmbConnection',
    section: ['File systems', 'SMB/Network shares'],
    labelKey: 'settings.network.directSmbConnection.label',
    descriptionKey: 'settings.network.directSmbConnection.description',
    cardKey: 'settings.network.card.connection',
    keywords: ['smb', 'direct', 'fast', 'connection', 'network', 'performance', 'smb2'],
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
    keywords: ['cache', 'smb', 'share', 'network', 'refresh', 'ttl'],
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
    keywords: ['timeout', 'network', 'slow', 'vpn', 'connection', 'latency'],
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
  // File systems › MTP (Android/Kindle/cameras)
  // ========================================================================
  {
    id: 'fileOperations.mtpEnabled',
    section: ['File systems', 'MTP (Android/Kindle/cameras)'],
    labelKey: 'settings.fileOperations.mtpEnabled.label',
    descriptionKey: 'settings.fileOperations.mtpEnabled.description',
    keywords: ['mtp', 'android', 'usb', 'device', 'phone', 'ptpcamerad', 'mobile'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    id: 'fileOperations.mtpConnectionWarning',
    section: ['File systems', 'MTP (Android/Kindle/cameras)'],
    labelKey: 'settings.fileOperations.mtpConnectionWarning.label',
    descriptionKey: 'settings.fileOperations.mtpConnectionWarning.description',
    keywords: ['mtp', 'warning', 'notification', 'connect', 'toast', 'android'],
    type: 'boolean',
    default: true,
    component: 'checkbox',
  },

  // ========================================================================
  // File systems › Android (ADB)
  // ========================================================================
  // ❗ Both are `hidden` until the section component exists. They are fully live
  // otherwise: persisted, synced across windows, and pushed to the backend by
  // `adb-settings.ts` through `settings-applier.ts`. Without the flag the sidebar
  // grows an "Android (ADB)" row that opens an empty page, because
  // `SettingsContent.svelte` routes a section to a hand-written component and
  // there isn't one yet (`docs/guides/adding-a-new-setting.md` step 2).
  // Dropping `hidden` is what the section's own commit does, alongside adding the
  // `<section>` and the `settings.spec.ts` order entry.
  {
    id: 'fileOperations.adbEnabled',
    section: ['File systems', 'Android (ADB)'],
    labelKey: 'settings.fileOperations.adbEnabled.label',
    descriptionKey: 'settings.fileOperations.adbEnabled.description',
    keywords: ['adb', 'android', 'usb debugging', 'developer options', 'phone', 'device', 'platform-tools'],
    type: 'boolean',
    default: true,
    component: 'switch',
    hidden: true,
  },
  {
    id: 'fileOperations.adbBinaryPath',
    section: ['File systems', 'Android (ADB)'],
    labelKey: 'settings.fileOperations.adbBinaryPath.label',
    descriptionKey: 'settings.fileOperations.adbBinaryPath.description',
    keywords: ['adb', 'android', 'path', 'binary', 'platform-tools', 'sdk', 'location'],
    type: 'string',
    default: '',
    component: 'text-input',
    hidden: true,
  },

  // ========================================================================
  // File systems › Git
  // ========================================================================
  {
    id: 'fileExplorer.git.showRepoChip',
    section: ['File systems', 'Git'],
    labelKey: 'settings.fileExplorer.git.showRepoChip.label',
    descriptionKey: 'settings.fileExplorer.git.showRepoChip.description',
    keywords: ['git', 'chip', 'branch', 'breadcrumb', 'repo', 'status', 'ahead', 'behind', 'dirty'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
  {
    id: 'fileExplorer.git.showStatusColumn',
    section: ['File systems', 'Git'],
    labelKey: 'settings.fileExplorer.git.showStatusColumn.label',
    descriptionKey: 'settings.fileExplorer.git.showStatusColumn.description',
    keywords: ['git', 'status', 'column', 'modified', 'untracked', 'ignored', 'added', 'deleted'],
    type: 'boolean',
    default: false,
    component: 'switch',
  },
  {
    id: 'fileExplorer.git.showVirtualGitPortal',
    section: ['File systems', 'Git'],
    labelKey: 'settings.fileExplorer.git.showVirtualGitPortal.label',
    descriptionKey: 'settings.fileExplorer.git.showVirtualGitPortal.description',
    keywords: ['git', 'portal', 'virtual', 'branches', 'tags', 'commits', 'worktrees', 'history'],
    type: 'boolean',
    default: true,
    component: 'switch',
  },
]
