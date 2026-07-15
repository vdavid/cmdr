/**
 * Operation log section settings (data only). Logic lives in `../settings-registry.ts`,
 * which concatenates this array into the full registry in section order.
 */

import type { SettingDefinitionSource } from '../types'

export const operationLogSettings: SettingDefinitionSource[] = [
  // ========================================================================
  // Operation log (retention)
  //
  // Both settings are read by the Rust retention loop each prune tick
  // (`settings::load_operation_log_retention_limits`), so a change live-applies
  // with no restart and no `settings-applier` case. Modeled on
  // `network.shareCacheDuration` (preset select + custom). Age is a duration in
  // ms where `0` is the "Forever" sentinel (never prune by age); size is a byte
  // budget. Keys and units are the retention contract — see `operation_log/DETAILS.md`.
  // ========================================================================
  {
    id: 'operationLog.maxAge',
    section: ['Operation log'],
    labelKey: 'settings.operationLog.maxAge.label',
    descriptionKey: 'settings.operationLog.maxAge.description',
    keywords: ['operation', 'log', 'history', 'retention', 'age', 'keep', 'prune', 'forever', 'days', 'undo'],
    type: 'duration',
    default: 0, // 0 ms = the "Forever" sentinel (never prune by age)
    component: 'select',
    constraints: {
      unit: 'd',
      options: [
        { value: 0, labelKey: 'settings.operationLog.maxAge.opt.forever' },
        { value: 2592000000, labelKey: 'settings.operationLog.maxAge.opt.days30' },
        { value: 7776000000, labelKey: 'settings.operationLog.maxAge.opt.days90' },
        { value: 31536000000, labelKey: 'settings.operationLog.maxAge.opt.year1' },
      ],
      allowCustom: true,
      customMin: 3600000, // 1 hour
      customMax: 315360000000, // 10 years
    },
  },
  {
    id: 'operationLog.maxSize',
    section: ['Operation log'],
    labelKey: 'settings.operationLog.maxSize.label',
    descriptionKey: 'settings.operationLog.maxSize.description',
    keywords: ['operation', 'log', 'history', 'retention', 'size', 'disk', 'space', 'limit', 'megabytes', 'gigabytes'],
    type: 'number',
    default: 3221225472, // 3 GB (binary), the default retention budget
    component: 'select',
    constraints: {
      options: [
        { value: 104857600, labelKey: 'settings.operationLog.maxSize.opt.mb100' },
        { value: 262144000, labelKey: 'settings.operationLog.maxSize.opt.mb250' },
        { value: 1073741824, labelKey: 'settings.operationLog.maxSize.opt.gb1' },
        { value: 2147483648, labelKey: 'settings.operationLog.maxSize.opt.gb2' },
        { value: 3221225472, labelKey: 'settings.operationLog.maxSize.opt.gb3' },
        { value: 5368709120, labelKey: 'settings.operationLog.maxSize.opt.gb5' },
      ],
      allowCustom: true,
      customMin: 10485760, // 10 MB
      customMax: 107374182400, // 100 GB
    },
  },
]
