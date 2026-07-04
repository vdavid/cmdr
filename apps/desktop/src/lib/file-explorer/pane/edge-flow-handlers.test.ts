import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { TabManager } from '../tabs/tab-state-manager.svelte'
import type { VolumeInfo } from '../types'
import type { NavigationHistory } from '../navigation/navigation-history'
import type { NavigateIntent, NavigateResult } from './navigate'
import type { FilePaneAPI } from './types'

const {
    getDefaultVolumeIdSpy,
    resolvePathVolumeSpy,
    pathExistsSpy,
    requestVolumeRefreshSpy,
    resolveValidPathSpy,
    getCurrentEntrySpy,
    canGoBackSpy,
} = vi.hoisted(() => ({
    getDefaultVolumeIdSpy: vi.fn<() => Promise<string>>(),
    resolvePathVolumeSpy: vi.fn<() => Promise<{ volume: { id: string } | null }>>(),
    pathExistsSpy: vi.fn<() => Promise<boolean>>(),
    requestVolumeRefreshSpy: vi.fn(),
    resolveValidPathSpy: vi.fn<() => Promise<string | null>>(),
    getCurrentEntrySpy: vi.fn(),
    canGoBackSpy: vi.fn<() => boolean>(),
}))

vi.mock('$lib/tauri-commands', () => ({
    getDefaultVolumeId: getDefaultVolumeIdSpy,
    resolvePathVolume: resolvePathVolumeSpy,
    pathExists: pathExistsSpy,
}))
vi.mock('$lib/stores/volume-store.svelte', () => ({ requestVolumeRefresh: requestVolumeRefreshSpy }))
vi.mock('../navigation/path-resolution', () => ({ resolveValidPath: resolveValidPathSpy }))
vi.mock('../navigation/navigation-history', () => ({
    getCurrentEntry: getCurrentEntrySpy,
    canGoBack: canGoBackSpy,
}))
vi.mock('$lib/logging/logger', () => ({
    getAppLogger: () => ({ warn: vi.fn(), info: vi.fn(), error: vi.fn(), debug: vi.fn() }),
}))

import { createEdgeFlowHandlers, type EdgeFlowHandlersDeps } from './edge-flow-handlers'

function makePaneRef(overrides: Partial<FilePaneAPI> = {}): FilePaneAPI {
    return {
        setNetworkHost: vi.fn(),
        navigateToPath: vi.fn(() => Promise.resolve()),
        ...overrides,
    } as unknown as FilePaneAPI
}

function makeTabMgr(unreachable: unknown): TabManager {
    return { tabs: [{ id: 't', unreachable }], activeTabId: 't' } as unknown as TabManager
}

function setup(opts: {
    paneRef?: FilePaneAPI
    volumes?: VolumeInfo[]
    history?: NavigationHistory
    volumeIdByPane?: Record<'left' | 'right', string>
    tabMgr?: TabManager
}) {
    const navigate = vi.fn<(i: NavigateIntent) => NavigateResult>(
        () => ({ status: 'started' }) as unknown as NavigateResult,
    )
    const focusContainer = vi.fn()
    const deps: EdgeFlowHandlersDeps = {
        navigate,
        getPaneRef: () => opts.paneRef,
        getPaneHistory: () => opts.history ?? ({} as NavigationHistory),
        getPaneVolumeId: (p) => opts.volumeIdByPane?.[p] ?? 'root',
        getTabMgr: () => opts.tabMgr ?? makeTabMgr(null),
        getVolumes: () => opts.volumes ?? [],
        focusContainer,
    }
    return { handlers: createEdgeFlowHandlers(deps), navigate, focusContainer }
}

describe('createEdgeFlowHandlers', () => {
    beforeEach(() => {
        vi.clearAllMocks()
        getDefaultVolumeIdSpy.mockResolvedValue('root')
        pathExistsSpy.mockResolvedValue(true)
    })

    describe('handleCancelLoading', () => {
        it('network entry re-commits the network volume without a history push and restores the host', () => {
            getCurrentEntrySpy.mockReturnValue({
                volumeId: 'network',
                path: 'smb://host/share',
                networkHost: { name: 'h' },
            })
            const paneRef = makePaneRef()
            const { handlers, navigate, focusContainer } = setup({ paneRef })

            handlers.handleCancelLoading('left', '/whatever')

            expect(navigate).toHaveBeenCalledWith({
                pane: 'left',
                to: { selectVolume: { volumeId: 'network', path: 'smb://host/share' } },
                source: 'fallback',
                pushHistory: false,
            })
            expect(paneRef.setNetworkHost).toHaveBeenCalledWith({ name: 'h' })
            expect(focusContainer).toHaveBeenCalled()
        })

        it('when the cancelled path is current and history can go back, navigates back', () => {
            getCurrentEntrySpy.mockReturnValue({ volumeId: 'root', path: '/a/b' })
            canGoBackSpy.mockReturnValue(true)
            const { handlers, navigate } = setup({ paneRef: makePaneRef() })

            handlers.handleCancelLoading('right', '/a/b')

            expect(navigate).toHaveBeenCalledWith({ pane: 'right', to: { history: 'back' }, source: 'cancel' })
        })

        it('when the cancelled path is current and there is no history, walks up to a valid parent', async () => {
            getCurrentEntrySpy.mockReturnValue({ volumeId: 'root', path: '/a/b' })
            canGoBackSpy.mockReturnValue(false)
            resolveValidPathSpy.mockResolvedValue('/a')
            const { handlers, navigate } = setup({
                paneRef: makePaneRef(),
                volumeIdByPane: { left: 'root', right: 'root' },
            })

            handlers.handleCancelLoading('left', '/a/b')
            await vi.waitFor(() => expect(navigate).toHaveBeenCalled())

            expect(navigate).toHaveBeenCalledWith({
                pane: 'left',
                to: { selectVolume: { volumeId: 'root', path: '/a' } },
                source: 'fallback',
                pushHistory: false,
            })
        })

        it('when the listing did not complete, navigates the pane straight to the previous folder', () => {
            getCurrentEntrySpy.mockReturnValue({ volumeId: 'root', path: '/prev' })
            const paneRef = makePaneRef()
            const { handlers, navigate } = setup({ paneRef })

            handlers.handleCancelLoading('left', '/different', 'pick.txt')

            expect(paneRef.navigateToPath).toHaveBeenCalledWith('/prev', 'pick.txt')
            expect(navigate).not.toHaveBeenCalled()
        })
    })

    it('handleMtpFatalError falls back to the default volume at its path (history push)', async () => {
        getDefaultVolumeIdSpy.mockResolvedValue('root')
        const { handlers, navigate } = setup({
            volumes: [{ id: 'root', name: 'Root', path: '/' } as unknown as VolumeInfo],
        })

        await handlers.handleMtpFatalError('left', 'device gone')

        expect(navigate).toHaveBeenCalledWith({
            pane: 'left',
            to: { selectVolume: { volumeId: 'root', path: '/' } },
            source: 'fallback',
        })
    })

    it('handleRetryUnreachable clears unreachable, navigates, and refreshes volumes', async () => {
        resolvePathVolumeSpy.mockResolvedValue({ volume: { id: 'usb' } })
        const tabMgr = makeTabMgr({ originalPath: '/Volumes/USB/x' })
        const { handlers, navigate } = setup({ tabMgr })

        await handlers.handleRetryUnreachable('left')

        const tab = tabMgr.tabs[0] as unknown as { unreachable: unknown }
        expect(tab.unreachable).toBeNull()
        expect(navigate).toHaveBeenCalledWith({
            pane: 'left',
            to: { selectVolume: { volumeId: 'usb', path: '/Volumes/USB/x' } },
            source: 'fallback',
        })
        expect(requestVolumeRefreshSpy).toHaveBeenCalled()
    })

    it('handleRetryUnreachable is a no-op when the tab is not unreachable', async () => {
        const { handlers, navigate } = setup({ tabMgr: makeTabMgr(null) })
        await handlers.handleRetryUnreachable('left')
        expect(navigate).not.toHaveBeenCalled()
    })

    it('handleOpenHome clears unreachable and navigates home on the default volume', async () => {
        getDefaultVolumeIdSpy.mockResolvedValue('root')
        const tabMgr = makeTabMgr({ originalPath: '/gone' })
        const { handlers, navigate } = setup({ tabMgr })

        await handlers.handleOpenHome('right')

        const tab = tabMgr.tabs[0] as unknown as { unreachable: unknown }
        expect(tab.unreachable).toBeNull()
        expect(navigate).toHaveBeenCalledWith({
            pane: 'right',
            to: { selectVolume: { volumeId: 'root', path: '~' } },
            source: 'fallback',
        })
    })

    it('handleVolumeUnmount redirects only affected panes, with no history push, falling back to / when home is gone', async () => {
        getDefaultVolumeIdSpy.mockResolvedValue('root')
        pathExistsSpy.mockResolvedValue(false)
        const { handlers, navigate } = setup({ volumeIdByPane: { left: 'usb', right: 'root' } })

        await handlers.handleVolumeUnmount('usb')

        expect(navigate).toHaveBeenCalledTimes(1)
        expect(navigate).toHaveBeenCalledWith({
            pane: 'left',
            to: { selectVolume: { volumeId: 'root', path: '/' } },
            source: 'fallback',
            pushHistory: false,
        })
    })
})
