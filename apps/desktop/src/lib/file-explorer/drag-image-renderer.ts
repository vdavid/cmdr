// Canvas-based drag image renderer for outbound drag operations.
// Produces a retina-aware PNG with dark semi-transparent card, file icons,
// middle-truncation, asymmetric edge fading, and count badge.

import type { DragFileInfo } from './drag-drop'
import { getCachedIcon } from '$lib/icon-cache'

/** Configuration for the drag image renderer */
const cardBg = 'rgba(20, 20, 20, 0.75)'
const cardBorderColor = 'rgba(255, 255, 255, 0.08)'
const textColor = 'rgba(255, 255, 255, 0.92)'
const mutedTextColor = 'rgba(255, 255, 255, 0.50)'
const countBadgeColor = 'rgba(255, 255, 255, 0.40)'
const separatorColor = 'rgba(255, 255, 255, 0.10)'
const topGlowColor = 'rgba(255, 255, 255, 0.04)'
const cornerRadius = 5
const lineHeight = 13
const fontSize = 7
const paddingX = 8
const paddingY = 7
const iconSize = 10
const iconTextGap = 4
const maxVisibleNames = 12
const firstNamesWhenTruncated = 8
const maxNameChars = 36
const ellipsis = '\u2026'

/** Asymmetric fade sizes */
const fadeTop = 3
const fadeBottom = 14
const fadeRight = 6

/** Fallback icon colors */
const fallbackFileColor = 'rgba(255, 255, 255, 0.35)'
const fallbackFolderColor = 'rgba(255, 255, 255, 0.40)'

/** Returns an emoji prefix for a file name (folder vs file heuristic). */
export function getEntryEmoji(name: string, isDirectory: boolean): string {
    return isDirectory ? '\uD83D\uDCC1 ' : '\uD83D\uDCC4 '
}

/**
 * Middle-truncates a file name, preserving the extension.
 * For example, "very-long-filename.txt" becomes "very-long\u2026me.txt".
 */
export function truncateMiddle(name: string, maxLength: number): string {
    if (name.length <= maxLength) return name

    const dotIndex = name.lastIndexOf('.')
    const hasExtension = dotIndex > 0 && dotIndex < name.length - 1
    const extension = hasExtension ? name.slice(dotIndex) : ''
    const baseName = hasExtension ? name.slice(0, dotIndex) : name

    // Reserve space for the extension and the ellipsis character
    const availableForBase = maxLength - extension.length - 1
    if (availableForBase <= 2) {
        // Not enough room to split base meaningfully
        return name.slice(0, maxLength - 1) + ellipsis
    }

    const frontLen = Math.ceil(availableForBase / 2)
    const backLen = availableForBase - frontLen

    return baseName.slice(0, frontLen) + ellipsis + baseName.slice(-backLen) + extension
}

/** Formats the count line, for example "3 files" or "1 file". */
export function formatFileCount(count: number): string {
    return `${String(count)} ${count === 1 ? 'file' : 'files'}`
}

/** A display line in the drag image. */
interface DisplayLine {
    text: string
    isMuted: boolean
    /** Index into the DragFileInfo array, or -1 for summary/count lines. */
    fileIndex: number
    /** Whether this is the "and N more" summary line (rendered in lighter weight). */
    isSummary: boolean
    /** Whether this is the count badge (rendered below a separator). */
    isCountBadge: boolean
}

/** Builds the list of display lines for the drag image. */
export function buildDisplayLines(names: string[], isDirectoryFlags: boolean[]): { text: string; isMuted: boolean }[] {
    const lines: { text: string; isMuted: boolean }[] = []
    const total = names.length

    if (total <= maxVisibleNames) {
        for (let i = 0; i < total; i++) {
            const emoji = getEntryEmoji(names[i], isDirectoryFlags[i] ?? false)
            lines.push({ text: emoji + truncateMiddle(names[i], maxNameChars), isMuted: false })
        }
    } else {
        for (let i = 0; i < firstNamesWhenTruncated; i++) {
            const emoji = getEntryEmoji(names[i], isDirectoryFlags[i] ?? false)
            lines.push({ text: emoji + truncateMiddle(names[i], maxNameChars), isMuted: false })
        }
        const remaining = total - firstNamesWhenTruncated
        lines.push({ text: `and ${String(remaining)} more`, isMuted: true })
    }

    // Count badge at the bottom
    lines.push({ text: formatFileCount(total), isMuted: true })

    return lines
}

/** Builds rich display lines with file index references for icon rendering. */
function buildRichDisplayLines(fileInfos: DragFileInfo[]): DisplayLine[] {
    const lines: DisplayLine[] = []
    const total = fileInfos.length

    if (total <= maxVisibleNames) {
        for (let i = 0; i < total; i++) {
            lines.push({
                text: truncateMiddle(fileInfos[i].name, maxNameChars),
                isMuted: false,
                fileIndex: i,
                isSummary: false,
                isCountBadge: false,
            })
        }
    } else {
        for (let i = 0; i < firstNamesWhenTruncated; i++) {
            lines.push({
                text: truncateMiddle(fileInfos[i].name, maxNameChars),
                isMuted: false,
                fileIndex: i,
                isSummary: false,
                isCountBadge: false,
            })
        }
        const remaining = total - firstNamesWhenTruncated
        lines.push({
            text: `and ${String(remaining)} more`,
            isMuted: true,
            fileIndex: -1,
            isSummary: true,
            isCountBadge: false,
        })
    }

    // Count badge at the bottom
    lines.push({
        text: formatFileCount(total),
        isMuted: true,
        fileIndex: -1,
        isSummary: false,
        isCountBadge: true,
    })

    return lines
}

/** Loads an image from a base64 data URL. Returns null if loading fails. */
function loadImage(dataUrl: string): Promise<HTMLImageElement | null> {
    return new Promise((resolve) => {
        const img = new Image()
        img.onload = () => {
            resolve(img)
        }
        img.onerror = () => {
            resolve(null)
        }
        img.src = dataUrl
    })
}

/** Preloads icons for all file entries, returning a map of file index to loaded image. */
async function preloadIcons(fileInfos: DragFileInfo[], lines: DisplayLine[]): Promise<Map<number, HTMLImageElement>> {
    const iconMap = new Map<number, HTMLImageElement>()
    const loadPromises: Promise<void>[] = []

    for (const line of lines) {
        if (line.fileIndex < 0) continue
        const info = fileInfos[line.fileIndex]
        const dataUrl = getCachedIcon(info.iconId)
        if (!dataUrl) continue

        loadPromises.push(
            loadImage(dataUrl).then((img) => {
                if (img) iconMap.set(line.fileIndex, img)
            }),
        )
    }

    await Promise.all(loadPromises)
    return iconMap
}

/** Draws a small filled rectangle as a file fallback icon. */
function drawFallbackFileIcon(ctx: CanvasRenderingContext2D, x: number, y: number, size: number): void {
    const inset = size * 0.15
    const w = size - inset * 2
    const h = size - inset * 2
    ctx.fillStyle = fallbackFileColor
    ctx.fillRect(x + inset, y + inset, w, h)
}

/** Draws a small open rectangle as a folder fallback icon. */
function drawFallbackFolderIcon(ctx: CanvasRenderingContext2D, x: number, y: number, size: number): void {
    const inset = size * 0.1
    const w = size - inset * 2
    const h = size - inset * 2
    const tabWidth = w * 0.4
    const tabHeight = h * 0.2

    ctx.fillStyle = fallbackFolderColor
    // Folder tab
    ctx.fillRect(x + inset, y + inset, tabWidth, tabHeight)
    // Folder body
    ctx.fillRect(x + inset, y + inset + tabHeight, w, h - tabHeight)
}

/**
 * Renders a drag image onto a canvas and returns it.
 * Loads actual file icons from the icon cache, falling back to geometric shapes.
 * The canvas is retina-aware (2x devicePixelRatio).
 */
export async function renderDragImage(fileInfos: DragFileInfo[]): Promise<HTMLCanvasElement> {
    const dpr = typeof window !== 'undefined' ? window.devicePixelRatio || 1 : 2
    const lines = buildRichDisplayLines(fileInfos)

    // Preload icons in parallel
    const iconMap = await preloadIcons(fileInfos, lines)

    // Separator line takes a small extra vertical space
    const separatorExtra = 5

    // Calculate dimensions
    const textLeft = paddingX + iconSize + iconTextGap
    const logicalWidth = textLeft + maxNameChars * 4 + paddingX
    const logicalHeight = paddingY * 2 + lines.length * lineHeight + separatorExtra

    const canvas = document.createElement('canvas')
    canvas.width = Math.round(logicalWidth * dpr)
    canvas.height = Math.round(logicalHeight * dpr)
    canvas.style.width = `${String(logicalWidth)}px`
    canvas.style.height = `${String(logicalHeight)}px`

    const ctx = canvas.getContext('2d')
    if (!ctx) return canvas

    ctx.scale(dpr, dpr)

    // Draw rounded rectangle background
    drawRoundedRect(ctx, 0, 0, logicalWidth, logicalHeight, cornerRadius, cardBg)

    // Draw subtle 1px border
    drawRoundedRectStroke(ctx, 0.5, 0.5, logicalWidth - 1, logicalHeight - 1, cornerRadius, cardBorderColor)

    // Draw subtle top inner glow (gradient from top to ~8px down)
    drawTopGlow(ctx, logicalWidth, cornerRadius)

    // Draw text and icon lines
    const font = `${String(fontSize)}px -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif`
    ctx.textBaseline = 'middle'

    let yOffset = 0

    for (let i = 0; i < lines.length; i++) {
        const line = lines[i]
        const y = paddingY + yOffset + lineHeight / 2

        if (line.isCountBadge) {
            // Draw a subtle separator line above the count badge
            const sepY = paddingY + yOffset - separatorExtra / 2
            ctx.strokeStyle = separatorColor
            ctx.lineWidth = 0.5
            ctx.beginPath()
            ctx.moveTo(paddingX, sepY)
            ctx.lineTo(logicalWidth - paddingX, sepY)
            ctx.stroke()

            ctx.font = font
            ctx.fillStyle = countBadgeColor
            ctx.fillText(line.text, textLeft, y)
        } else if (line.isSummary) {
            // "and N more" line: lighter color, slightly smaller for visual distinction
            const smallerFont = `${String(fontSize - 0.5)}px -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif`
            ctx.font = smallerFont
            ctx.fillStyle = mutedTextColor
            ctx.fillText(line.text, textLeft, y)
        } else {
            // Regular file line with icon
            const info = fileInfos[line.fileIndex]
            const loadedIcon = iconMap.get(line.fileIndex)
            const iconY = y - iconSize / 2

            if (loadedIcon) {
                ctx.drawImage(loadedIcon, paddingX, iconY, iconSize, iconSize)
            } else if (info.isDirectory) {
                drawFallbackFolderIcon(ctx, paddingX, iconY, iconSize)
            } else {
                drawFallbackFileIcon(ctx, paddingX, iconY, iconSize)
            }

            ctx.font = font
            ctx.fillStyle = textColor
            ctx.fillText(line.text, textLeft, y)
        }

        // Add extra space before the count badge
        yOffset += lineHeight
        if (i < lines.length - 1 && lines[i + 1].isCountBadge) {
            yOffset += separatorExtra
        }
    }

    // Apply asymmetric edge fading
    applyEdgeFade(ctx, logicalWidth, logicalHeight)

    return canvas
}

/** Draws a filled rounded rectangle. */
function drawRoundedRect(
    ctx: CanvasRenderingContext2D,
    x: number,
    y: number,
    w: number,
    h: number,
    r: number,
    fillColor: string,
) {
    ctx.beginPath()
    ctx.moveTo(x + r, y)
    ctx.lineTo(x + w - r, y)
    ctx.quadraticCurveTo(x + w, y, x + w, y + r)
    ctx.lineTo(x + w, y + h - r)
    ctx.quadraticCurveTo(x + w, y + h, x + w - r, y + h)
    ctx.lineTo(x + r, y + h)
    ctx.quadraticCurveTo(x, y + h, x, y + h - r)
    ctx.lineTo(x, y + r)
    ctx.quadraticCurveTo(x, y, x + r, y)
    ctx.closePath()
    ctx.fillStyle = fillColor
    ctx.fill()
}

/** Draws a stroked rounded rectangle for the card border. */
function drawRoundedRectStroke(
    ctx: CanvasRenderingContext2D,
    x: number,
    y: number,
    w: number,
    h: number,
    r: number,
    strokeColor: string,
) {
    ctx.beginPath()
    ctx.moveTo(x + r, y)
    ctx.lineTo(x + w - r, y)
    ctx.quadraticCurveTo(x + w, y, x + w, y + r)
    ctx.lineTo(x + w, y + h - r)
    ctx.quadraticCurveTo(x + w, y + h, x + w - r, y + h)
    ctx.lineTo(x + r, y + h)
    ctx.quadraticCurveTo(x, y + h, x, y + h - r)
    ctx.lineTo(x, y + r)
    ctx.quadraticCurveTo(x, y, x + r, y)
    ctx.closePath()
    ctx.strokeStyle = strokeColor
    ctx.lineWidth = 1
    ctx.stroke()
}

/** Draws a subtle top inner glow across the card. */
function drawTopGlow(ctx: CanvasRenderingContext2D, w: number, r: number): void {
    const glowHeight = 8
    ctx.save()

    // Clip to rounded top area
    ctx.beginPath()
    ctx.moveTo(r, 0)
    ctx.lineTo(w - r, 0)
    ctx.quadraticCurveTo(w, 0, w, r)
    ctx.lineTo(w, glowHeight)
    ctx.lineTo(0, glowHeight)
    ctx.lineTo(0, r)
    ctx.quadraticCurveTo(0, 0, r, 0)
    ctx.closePath()
    ctx.clip()

    const grad = ctx.createLinearGradient(0, 0, 0, glowHeight)
    grad.addColorStop(0, topGlowColor)
    grad.addColorStop(1, 'rgba(255, 255, 255, 0)')
    ctx.fillStyle = grad
    ctx.fillRect(0, 0, w, glowHeight)

    ctx.restore()
}

/**
 * Applies asymmetric edge-fade using gradient masks composited with destination-out.
 * Top: very subtle (3px). Bottom: strong fade-out (14px). Left: none. Right: subtle (6px).
 */
function applyEdgeFade(ctx: CanvasRenderingContext2D, w: number, h: number) {
    ctx.save()
    ctx.globalCompositeOperation = 'destination-out'

    // Top edge: very subtle
    const topGrad = ctx.createLinearGradient(0, 0, 0, fadeTop)
    topGrad.addColorStop(0, 'rgba(0,0,0,0.6)')
    topGrad.addColorStop(1, 'rgba(0,0,0,0)')
    ctx.fillStyle = topGrad
    ctx.fillRect(0, 0, w, fadeTop)

    // Bottom edge: strong fade-out
    const bottomGrad = ctx.createLinearGradient(0, h - fadeBottom, 0, h)
    bottomGrad.addColorStop(0, 'rgba(0,0,0,0)')
    bottomGrad.addColorStop(1, 'rgba(0,0,0,1)')
    ctx.fillStyle = bottomGrad
    ctx.fillRect(0, h - fadeBottom, w, fadeBottom)

    // Right edge: subtle fade for truncated names
    const rightGrad = ctx.createLinearGradient(w - fadeRight, 0, w, 0)
    rightGrad.addColorStop(0, 'rgba(0,0,0,0)')
    rightGrad.addColorStop(1, 'rgba(0,0,0,0.7)')
    ctx.fillStyle = rightGrad
    ctx.fillRect(w - fadeRight, 0, fadeRight, h)

    // No left edge fade

    ctx.restore()
}
