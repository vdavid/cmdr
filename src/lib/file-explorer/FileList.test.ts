import { describe, it, expect, vi } from 'vitest'
import { mount } from 'svelte'
import FileList from './FileList.svelte'
import type { FileEntry } from './types'
import { createFileEntry } from './test-helpers'

// Mock icon-cache to avoid Tauri dependency
vi.mock('$lib/icon-cache', () => ({
    getCachedIcon: vi.fn().mockReturnValue(undefined),
    prefetchIcons: vi.fn().mockResolvedValue(undefined),
}))

describe('FileList', () => {
    const noop = () => {}

    it('renders file entries', () => {
        const files: FileEntry[] = [
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

        const target = document.createElement('div')
        mount(FileList, {
            target,
            props: { files, selectedIndex: 0, onSelect: noop, onNavigate: noop },
        })

        expect(target.textContent).toContain('Documents')
        expect(target.textContent).toContain('file.txt')
    })

    it('distinguishes directories from files', () => {
        const files: FileEntry[] = [
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

        const target = document.createElement('div')
        mount(FileList, {
            target,
            props: { files, selectedIndex: 0, onSelect: noop, onNavigate: noop },
        })

        const entries = target.querySelectorAll('.file-entry')
        expect(entries[0].classList.contains('is-directory')).toBe(true)
        expect(entries[1].classList.contains('is-directory')).toBe(false)
    })

    it('renders empty list', () => {
        const target = document.createElement('div')
        mount(FileList, {
            target,
            props: { files: [], selectedIndex: 0, onSelect: noop, onNavigate: noop },
        })

        expect(target.querySelector('.file-list')).toBeTruthy()
        expect(target.querySelectorAll('.file-entry')).toHaveLength(0)
    })

    it('displays icons for files and directories', () => {
        const files: FileEntry[] = [
            createFileEntry({
                name: 'Documents',
                path: '/home/user/Documents',
                isDirectory: true,
            }),
        ]

        const target = document.createElement('div')
        mount(FileList, {
            target,
            props: { files, selectedIndex: 0, onSelect: noop, onNavigate: noop },
        })

        const icons = target.querySelectorAll('.icon, .icon-emoji')
        expect(icons.length).toBeGreaterThan(0)
    })

    it('shows selected state on correct item', () => {
        const files: FileEntry[] = [
            createFileEntry({ name: 'a', path: '/a', isDirectory: false }),
            createFileEntry({ name: 'b', path: '/b', isDirectory: false }),
        ]

        const target = document.createElement('div')
        mount(FileList, {
            target,
            props: { files, selectedIndex: 1, onSelect: noop, onNavigate: noop },
        })

        const entries = target.querySelectorAll('.file-entry')
        expect(entries[0].classList.contains('is-selected')).toBe(false)
        expect(entries[1].classList.contains('is-selected')).toBe(true)
    })

    it('formats parent directory as ..', () => {
        const files: FileEntry[] = [createFileEntry({ name: '..', path: '/home', isDirectory: true })]

        const target = document.createElement('div')
        mount(FileList, {
            target,
            props: { files, selectedIndex: 0, onSelect: noop, onNavigate: noop },
        })

        expect(target.textContent).toContain('..')
    })

    it('calls onSelect when item is clicked', () => {
        const files: FileEntry[] = [createFileEntry({ name: 'test', path: '/test', isDirectory: false })]
        const onSelect = vi.fn()

        const target = document.createElement('div')
        mount(FileList, {
            target,
            props: { files, selectedIndex: 0, onSelect, onNavigate: noop },
        })

        const entry = target.querySelector('.file-entry') as HTMLElement
        entry.click()

        expect(onSelect).toHaveBeenCalledWith(0)
    })
})
