// Reactive state for the in-app drag overlay.
// Manages overlay visibility, position, file infos, and target info.

const maxDisplayedNames = 20
const firstNamesWhenTruncated = 10

export type DragOperation = 'copy' | 'move'

export interface OverlayFileInfo {
    name: string
    iconUrl: string | undefined
    isDirectory: boolean
}

export interface OverlayNameLine {
    text: string
    iconUrl: string | undefined
    isDirectory: boolean
    /** True for the "and N more" summary line. */
    isSummary: boolean
}

interface OverlayState {
    visible: boolean
    x: number
    y: number
    fileInfos: OverlayFileInfo[]
    totalCount: number
    targetName: string | null
    operation: DragOperation
    canDrop: boolean
}

const state = $state<OverlayState>({
    visible: false,
    x: 0,
    y: 0,
    fileInfos: [],
    totalCount: 0,
    targetName: null,
    operation: 'copy',
    canDrop: false,
})

/** Shows the overlay with the given file infos. Only stores the first entries needed for display. */
export function showOverlay(fileInfos: OverlayFileInfo[], totalCount: number): void {
    state.visible = true
    state.fileInfos = fileInfos.slice(0, maxDisplayedNames)
    state.totalCount = totalCount
    state.canDrop = false
    state.targetName = null
}

/** Updates the overlay position and target info. */
export function updateOverlay(
    x: number,
    y: number,
    targetName: string | null,
    canDrop: boolean,
    operation: DragOperation,
): void {
    state.x = x
    state.y = y
    state.targetName = targetName
    state.canDrop = canDrop
    state.operation = operation
}

/** Hides the overlay and resets state. */
export function hideOverlay(): void {
    state.visible = false
    state.x = 0
    state.y = 0
    state.fileInfos = []
    state.totalCount = 0
    state.targetName = null
    state.canDrop = false
}

/** Returns current overlay visibility. */
export function getOverlayVisible(): boolean {
    return state.visible
}

/** Returns current overlay x position. */
export function getOverlayX(): number {
    return state.x
}

/** Returns current overlay y position. */
export function getOverlayY(): number {
    return state.y
}

/** Returns file infos to display (already sliced to the display limit). */
export function getOverlayFileInfos(): OverlayFileInfo[] {
    return state.fileInfos
}

/** Returns the total file count (may be larger than displayed entries). */
export function getOverlayTotalCount(): number {
    return state.totalCount
}

/** Returns the resolved target folder name, or null if no target. */
export function getOverlayTargetName(): string | null {
    return state.targetName
}

/** Returns the current drag operation type. */
export function getOverlayOperation(): DragOperation {
    return state.operation
}

/** Returns whether dropping is allowed at the current position. */
export function getOverlayCanDrop(): boolean {
    return state.canDrop
}

/**
 * Builds structured display lines for the overlay: file entries with icons, optional "and N more" line.
 * Separate from the action line which is rendered differently.
 */
export function buildOverlayNameLines(fileInfos: OverlayFileInfo[], totalCount: number): OverlayNameLine[] {
    const lines: OverlayNameLine[] = []

    if (totalCount <= maxDisplayedNames) {
        for (const info of fileInfos) {
            lines.push({ text: info.name, iconUrl: info.iconUrl, isDirectory: info.isDirectory, isSummary: false })
        }
    } else {
        for (let i = 0; i < firstNamesWhenTruncated; i++) {
            if (i < fileInfos.length) {
                const info = fileInfos[i]
                lines.push({ text: info.name, iconUrl: info.iconUrl, isDirectory: info.isDirectory, isSummary: false })
            }
        }
        const remaining = totalCount - firstNamesWhenTruncated
        lines.push({ text: `and ${String(remaining)} more`, iconUrl: undefined, isDirectory: false, isSummary: true })
    }

    return lines
}

/** Formats the action line text shown at the bottom of the overlay. */
export function formatActionLine(operation: DragOperation, targetName: string | null, canDrop: boolean): string {
    if (!canDrop) return "Can't drop here"

    const verb = operation === 'move' ? 'Move' : 'Copy'
    if (targetName) {
        return `${verb} to ${targetName}`
    }
    return verb
}
