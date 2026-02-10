// Canvas-based drag image renderer for outbound drag operations.
// Produces a retina-aware PNG with dark semi-transparent card, file names,
// middle-truncation, fading edges, and count badge.

/** Configuration for the drag image renderer */
const cardBg = 'rgba(30, 30, 30, 0.85)'
const textColor = 'rgba(255, 255, 255, 0.92)'
const mutedTextColor = 'rgba(255, 255, 255, 0.55)'
const countBadgeColor = 'rgba(255, 255, 255, 0.45)'
const cornerRadius = 5
const lineHeight = 11
const fontSize = 7
const paddingX = 8
const paddingY = 6
const maxVisibleNames = 12
const firstNamesWhenTruncated = 8
const fadeSize = 8
const maxNameChars = 36
const ellipsis = '\u2026'

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

/**
 * Renders a drag image onto a canvas and returns it.
 * The canvas is retina-aware (2x devicePixelRatio).
 */
export function renderDragImage(names: string[], isDirectoryFlags: boolean[]): HTMLCanvasElement {
    const dpr = typeof window !== 'undefined' ? window.devicePixelRatio || 1 : 2
    const lines = buildDisplayLines(names, isDirectoryFlags)

    // Calculate dimensions
    const logicalWidth = paddingX * 2 + maxNameChars * 4 // approximate max width
    const logicalHeight = paddingY * 2 + lines.length * lineHeight

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

    // Draw text lines
    ctx.font = `${String(fontSize)}px -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif`
    ctx.textBaseline = 'middle'

    for (let i = 0; i < lines.length; i++) {
        const line = lines[i]
        ctx.fillStyle = line.isMuted ? mutedTextColor : textColor
        // Last line (count badge) gets special styling
        if (i === lines.length - 1) {
            ctx.fillStyle = countBadgeColor
        }
        const y = paddingY + i * lineHeight + lineHeight / 2
        ctx.fillText(line.text, paddingX, y)
    }

    // Apply fading edges using destination-out compositing
    applyEdgeFade(ctx, logicalWidth, logicalHeight, fadeSize)

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

/** Applies edge-fade using gradient masks composited with destination-out. */
function applyEdgeFade(ctx: CanvasRenderingContext2D, w: number, h: number, fade: number) {
    ctx.save()
    ctx.globalCompositeOperation = 'destination-out'

    // Top edge
    const topGrad = ctx.createLinearGradient(0, 0, 0, fade)
    topGrad.addColorStop(0, 'rgba(0,0,0,1)')
    topGrad.addColorStop(1, 'rgba(0,0,0,0)')
    ctx.fillStyle = topGrad
    ctx.fillRect(0, 0, w, fade)

    // Bottom edge
    const bottomGrad = ctx.createLinearGradient(0, h - fade, 0, h)
    bottomGrad.addColorStop(0, 'rgba(0,0,0,0)')
    bottomGrad.addColorStop(1, 'rgba(0,0,0,1)')
    ctx.fillStyle = bottomGrad
    ctx.fillRect(0, h - fade, w, fade)

    // Left edge
    const leftGrad = ctx.createLinearGradient(0, 0, fade, 0)
    leftGrad.addColorStop(0, 'rgba(0,0,0,1)')
    leftGrad.addColorStop(1, 'rgba(0,0,0,0)')
    ctx.fillStyle = leftGrad
    ctx.fillRect(0, 0, fade, h)

    // Right edge
    const rightGrad = ctx.createLinearGradient(w - fade, 0, w, 0)
    rightGrad.addColorStop(0, 'rgba(0,0,0,0)')
    rightGrad.addColorStop(1, 'rgba(0,0,0,1)')
    ctx.fillStyle = rightGrad
    ctx.fillRect(w - fade, 0, fade, h)

    ctx.restore()
}
