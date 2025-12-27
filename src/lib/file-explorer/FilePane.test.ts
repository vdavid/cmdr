import { describe, it, expect, beforeEach, vi } from 'vitest'
import { mount } from 'svelte'
import { tick } from 'svelte'
import FilePane from './FilePane.svelte'
import { MockFileService } from '$lib/file-service'
import type { FileEntry } from './types'
import { createFileEntry } from './test-helpers'

// Mock icon-cache to avoid Tauri dependency
// Use a manual mock store instead of importing writable
vi.mock('$lib/icon-cache', () => ({
    getCachedIcon: vi.fn().mockReturnValue(undefined),
    prefetchIcons: vi.fn().mockResolvedValue(undefined),
    refreshDirectoryIcons: vi.fn().mockResolvedValue(undefined),
    iconCacheVersion: {
        subscribe: vi.fn((fn: (value: number) => void) => {
            fn(0)
            return () => {}
        }),
    },
}))

describe('FilePane', () => {
    let mockService: MockFileService
    let mockFiles: FileEntry[]

    beforeEach(() => {
        mockService = new MockFileService()
        mockFiles = [
            createFileEntry({
                name: 'Documents',
                path: '/home/user/Documents',
                isDirectory: true,
            }),
            createFileEntry({
                name: 'file.txt',
                path: '/home/user/file.txt',
                isDirectory: false,
            }),
        ]
    })

    it('renders without crashing', () => {
        mockService.setMockData('/test', mockFiles)
        const target = document.createElement('div')
        mount(FilePane, { target, props: { initialPath: '/test', fileService: mockService } })
        expect(target.querySelector('.file-pane')).toBeTruthy()
    })

    it('displays loading state initially', () => {
        mockService.setMockData('/test', mockFiles)
        const target = document.createElement('div')
        mount(FilePane, { target, props: { initialPath: '/test', fileService: mockService } })
        expect(target.textContent).toContain('Loading')
    })

    it('displays file list after loading', async () => {
        mockService.setMockData('/test', mockFiles)
        const target = document.createElement('div')
        mount(FilePane, { target, props: { initialPath: '/test', fileService: mockService } })

        await tick()
        await tick() // Wait for async operations

        // Directory names are displayed without brackets
        expect(target.textContent).toContain('Documents')
        expect(target.textContent).toContain('file.txt')
    })

    it('displays error message when directory cannot be read', async () => {
        const target = document.createElement('div')
        mount(FilePane, { target, props: { initialPath: '/nonexistent', fileService: mockService } })

        await tick()
        await tick()

        expect(target.querySelector('.error-message')).toBeTruthy()
        expect(target.textContent).toContain('Mock data not configured')
    })

    it('displays parent directory entry except at root', async () => {
        mockService.setMockData('/home/user', mockFiles)
        const target = document.createElement('div')
        mount(FilePane, { target, props: { initialPath: '/home/user', fileService: mockService } })

        await tick()
        await tick()

        // Should show `..` for parent navigation
        expect(target.textContent).toContain('..')
    })

    it('does not display parent entry at root', async () => {
        mockService.setMockData('/', mockFiles)
        const target = document.createElement('div')
        mount(FilePane, { target, props: { initialPath: '/', fileService: mockService } })

        await tick()
        await tick()

        expect(target.textContent).not.toMatch(/📁\s*\.\./)
    })

    it('hides hidden files when showHiddenFiles is false', async () => {
        const filesWithHidden: FileEntry[] = [
            createFileEntry({ name: '.hidden', path: '/test/.hidden', isDirectory: false }),
            createFileEntry({ name: '.config', path: '/test/.config', isDirectory: true }),
            createFileEntry({ name: 'visible.txt', path: '/test/visible.txt', isDirectory: false }),
        ]
        mockService.setMockData('/test', filesWithHidden)
        const target = document.createElement('div')
        mount(FilePane, {
            target,
            props: { initialPath: '/test', fileService: mockService, showHiddenFiles: false },
        })

        await tick()
        await tick()

        expect(target.textContent).toContain('visible.txt')
        expect(target.textContent).not.toContain('.hidden')
        expect(target.textContent).not.toContain('.config')
    })

    it('shows hidden files when showHiddenFiles is true', async () => {
        const filesWithHidden: FileEntry[] = [
            createFileEntry({ name: '.hidden', path: '/test/.hidden', isDirectory: false }),
            createFileEntry({ name: 'visible.txt', path: '/test/visible.txt', isDirectory: false }),
        ]
        mockService.setMockData('/test', filesWithHidden)
        const target = document.createElement('div')
        mount(FilePane, {
            target,
            props: { initialPath: '/test', fileService: mockService, showHiddenFiles: true },
        })

        await tick()
        await tick()

        expect(target.textContent).toContain('visible.txt')
        expect(target.textContent).toContain('.hidden')
    })

    it('always shows .. entry even when showHiddenFiles is false', async () => {
        const filesWithHidden: FileEntry[] = [
            createFileEntry({ name: '.hidden', path: '/test/.hidden', isDirectory: false }),
        ]
        mockService.setMockData('/test', filesWithHidden)
        const target = document.createElement('div')
        mount(FilePane, {
            target,
            props: { initialPath: '/test', fileService: mockService, showHiddenFiles: false },
        })

        await tick()
        await tick()

        // Parent entry should still be visible
        expect(target.textContent).toContain('..')
    })
})
