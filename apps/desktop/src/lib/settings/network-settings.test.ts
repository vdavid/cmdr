import { describe, it, expect, vi, beforeEach } from 'vitest'
import { getNetworkTimeoutMs, getMountTimeoutMs, getShareCacheTtlMs } from './network-settings'

vi.mock('$lib/settings/settings-store', () => ({
    getSetting: vi.fn(),
}))

import { getSetting } from './settings-store'

const mockGetSetting = vi.mocked(getSetting)

describe('getNetworkTimeoutMs', () => {
    beforeEach(() => {
        vi.resetAllMocks()
    })

    it('returns 15000 ms for normal mode', () => {
        mockGetSetting.mockImplementation((key: string) => {
            if (key === 'network.timeoutMode') return 'normal'
            return ''
        })
        expect(getNetworkTimeoutMs()).toBe(15_000)
    })

    it('returns 45000 ms for slow mode', () => {
        mockGetSetting.mockImplementation((key: string) => {
            if (key === 'network.timeoutMode') return 'slow'
            return ''
        })
        expect(getNetworkTimeoutMs()).toBe(45_000)
    })

    it('returns custom timeout converted from seconds to ms', () => {
        mockGetSetting.mockImplementation((key: string) => {
            if (key === 'network.timeoutMode') return 'custom'
            if (key === 'network.customTimeout') return 30
            return ''
        })
        expect(getNetworkTimeoutMs()).toBe(30_000)
    })

    it('handles fractional custom timeout', () => {
        mockGetSetting.mockImplementation((key: string) => {
            if (key === 'network.timeoutMode') return 'custom'
            if (key === 'network.customTimeout') return 0.5
            return ''
        })
        expect(getNetworkTimeoutMs()).toBe(500)
    })

    it('handles zero custom timeout', () => {
        mockGetSetting.mockImplementation((key: string) => {
            if (key === 'network.timeoutMode') return 'custom'
            if (key === 'network.customTimeout') return 0
            return ''
        })
        expect(getNetworkTimeoutMs()).toBe(0)
    })
})

describe('getMountTimeoutMs', () => {
    beforeEach(() => {
        vi.resetAllMocks()
    })

    it('returns the mount timeout from settings', () => {
        mockGetSetting.mockReturnValue(30_000)
        expect(getMountTimeoutMs()).toBe(30_000)
    })
})

describe('getShareCacheTtlMs', () => {
    beforeEach(() => {
        vi.resetAllMocks()
    })

    it('returns the share cache duration from settings', () => {
        mockGetSetting.mockReturnValue(60_000)
        expect(getShareCacheTtlMs()).toBe(60_000)
    })
})
