// Test helper for creating FileEntry objects with sensible defaults

import type { FileEntry } from './types'

/**
 * Creates a FileEntry with all required fields, using sensible defaults.
 * Only name, path, and isDirectory are required; all other fields have defaults.
 */
export function createFileEntry(partial: {
    name: string
    path: string
    isDirectory: boolean
    isSymlink?: boolean
    size?: number
    modifiedAt?: number
    createdAt?: number
    permissions?: number
    owner?: string
    group?: string
    iconId?: string
    extendedMetadataLoaded?: boolean
}): FileEntry {
    const isDir = partial.isDirectory
    return {
        name: partial.name,
        path: partial.path,
        isDirectory: isDir,
        isSymlink: partial.isSymlink ?? false,
        size: partial.size,
        modifiedAt: partial.modifiedAt,
        createdAt: partial.createdAt,
        permissions: partial.permissions ?? (isDir ? 0o755 : 0o644),
        owner: partial.owner ?? 'testuser',
        group: partial.group ?? 'staff',
        iconId: partial.iconId ?? (isDir ? 'dir' : 'file'),
        extendedMetadataLoaded: partial.extendedMetadataLoaded ?? true,
    }
}

/**
 * Creates a realistic directory listing with a mix of file types.
 * Matches what the backend would return with directories first, sorted alphabetically.
 */
export function createMockDirectoryListing(): FileEntry[] {
    const now = Math.floor(Date.now() / 1000)
    return [
        // Hidden directories first
        createFileEntry({
            name: '.git',
            path: '/.git',
            isDirectory: true,
            modifiedAt: now - 3600,
        }),
        createFileEntry({
            name: '.hidden_dir',
            path: '/.hidden_dir',
            isDirectory: true,
            modifiedAt: now - 7200,
        }),
        // Visible directories
        createFileEntry({
            name: 'Documents',
            path: '/Documents',
            isDirectory: true,
            modifiedAt: now - 86400,
        }),
        createFileEntry({
            name: 'Downloads',
            path: '/Downloads',
            isDirectory: true,
            modifiedAt: now - 172800,
        }),
        // Hidden files
        createFileEntry({
            name: '.bashrc',
            path: '/.bashrc',
            isDirectory: false,
            size: 1234,
            modifiedAt: now - 259200,
        }),
        createFileEntry({
            name: '.gitignore',
            path: '/.gitignore',
            isDirectory: false,
            size: 456,
            modifiedAt: now - 345600,
        }),
        // Visible files
        createFileEntry({
            name: 'README.md',
            path: '/README.md',
            isDirectory: false,
            size: 2048,
            modifiedAt: now - 432000,
        }),
        createFileEntry({
            name: 'file.txt',
            path: '/file.txt',
            isDirectory: false,
            size: 512,
            modifiedAt: now - 518400,
        }),
    ]
}

/**
 * Filters a file list based on hidden file setting (matches backend behavior).
 * Always preserves the parent entry (..) regardless of hidden setting.
 */
export function filterHiddenFiles(entries: FileEntry[], showHidden: boolean): FileEntry[] {
    if (showHidden) return entries
    return entries.filter((e) => e.name === '..' || !e.name.startsWith('.'))
}

/**
 * Creates mock entries with configurable counts for stress testing.
 */
export function createMockEntriesWithCount(
    count: number,
    options?: { includeHidden?: boolean; onlyDirs?: boolean },
): FileEntry[] {
    const entries: FileEntry[] = []
    const now = Math.floor(Date.now() / 1000)

    for (let i = 0; i < count; i++) {
        const isDir = options?.onlyDirs ?? i % 10 === 0 // Every 10th is a directory
        const isHidden = options?.includeHidden ?? i % 20 === 0 // Every 20th is hidden
        const name = isHidden ? `.file_${i.toString().padStart(5, '0')}` : `file_${i.toString().padStart(5, '0')}`
        entries.push(
            createFileEntry({
                name: isDir ? name.replace('file', 'folder') : name,
                path: `/${name}`,
                isDirectory: isDir,
                size: isDir ? undefined : i * 100,
                modifiedAt: now - i * 1000,
            }),
        )
    }

    // Sort: directories first, then alphabetically
    return entries.sort((a, b) => {
        if (a.isDirectory !== b.isDirectory) return a.isDirectory ? -1 : 1
        return a.name.localeCompare(b.name)
    })
}
