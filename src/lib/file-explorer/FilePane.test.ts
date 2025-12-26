import { describe, it, expect, beforeEach } from 'vitest'
import { mount } from 'svelte'
import { tick } from 'svelte'
import FilePane from './FilePane.svelte'
import { MockFileService } from '$lib/file-service'
import type { FileEntry } from './types'

describe('FilePane', () => {
    let mockService: MockFileService
    let mockFiles: FileEntry[]

    beforeEach(() => {
        mockService = new MockFileService()
        mockFiles = [
            {
                name: 'Documents',
                path: '/home/user/Documents',
                isDirectory: true,
            },
            {
                name: 'file.txt',
                path: '/home/user/file.txt',
                isDirectory: false,
            },
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

        // Directory names are now wrapped in brackets
        expect(target.textContent).toContain('[Documents]')
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

        // Should show [..] for parent navigation
        expect(target.textContent).toContain('[..]')
    })

    it('does not display parent entry at root', async () => {
        mockService.setMockData('/', mockFiles)
        const target = document.createElement('div')
        mount(FilePane, { target, props: { initialPath: '/', fileService: mockService } })

        await tick()
        await tick()

        expect(target.textContent).not.toContain('[..]')
    })

    it('hides hidden files when showHiddenFiles is false', async () => {
        const filesWithHidden: FileEntry[] = [
            { name: '.hidden', path: '/test/.hidden', isDirectory: false },
            { name: '.config', path: '/test/.config', isDirectory: true },
            { name: 'visible.txt', path: '/test/visible.txt', isDirectory: false },
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
        expect(target.textContent).not.toContain('[.config]')
    })

    it('shows hidden files when showHiddenFiles is true', async () => {
        const filesWithHidden: FileEntry[] = [
            { name: '.hidden', path: '/test/.hidden', isDirectory: false },
            { name: 'visible.txt', path: '/test/visible.txt', isDirectory: false },
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
        const filesWithHidden: FileEntry[] = [{ name: '.hidden', path: '/test/.hidden', isDirectory: false }]
        mockService.setMockData('/test', filesWithHidden)
        const target = document.createElement('div')
        mount(FilePane, {
            target,
            props: { initialPath: '/test', fileService: mockService, showHiddenFiles: false },
        })

        await tick()
        await tick()

        // Parent entry should still be visible
        expect(target.textContent).toContain('[..]')
    })
})
