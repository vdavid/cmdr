/**
 * Settings the E2E suite restates because it can't import the registry: each must match.
 */
import { describe, it, expect } from 'vitest'
import { getSettingDefinition } from './settings-registry'
import { CLOSED_TAB_HISTORY_DEFAULT, CLOSED_TAB_HISTORY_SIZE } from '../../../test/e2e-playwright/closed-tab-history'

describe('E2E setting mirrors', () => {
  it('restates the closed-tab history default', () => {
    expect(getSettingDefinition(CLOSED_TAB_HISTORY_SIZE)?.default).toBe(CLOSED_TAB_HISTORY_DEFAULT)
  })
})
