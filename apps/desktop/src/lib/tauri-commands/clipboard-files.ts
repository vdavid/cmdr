import { invoke } from '@tauri-apps/api/core'

export interface ClipboardReadResult {
    paths: string[]
    isCut: boolean
}

export async function copyFilesToClipboard(
    listingId: string,
    selectedIndices: number[],
    cursorIndex: number,
    hasParent: boolean,
    includeHidden: boolean,
): Promise<number> {
    return invoke<number>('copy_files_to_clipboard', {
        listingId,
        selectedIndices,
        cursorIndex,
        hasParent,
        includeHidden,
    })
}

export async function cutFilesToClipboard(
    listingId: string,
    selectedIndices: number[],
    cursorIndex: number,
    hasParent: boolean,
    includeHidden: boolean,
): Promise<number> {
    return invoke<number>('cut_files_to_clipboard', {
        listingId,
        selectedIndices,
        cursorIndex,
        hasParent,
        includeHidden,
    })
}

export async function readClipboardFiles(): Promise<ClipboardReadResult> {
    return invoke<ClipboardReadResult>('read_clipboard_files')
}

export async function clearClipboardCutState(): Promise<void> {
    await invoke('clear_clipboard_cut_state')
}
