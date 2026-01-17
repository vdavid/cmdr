// Character measurement using Canvas API

import { getAppLogger } from '$lib/logger'

const log = getAppLogger('fontMetrics')

/**
 * Measures character widths for a given font configuration.
 * Covers BMP (Basic Multilingual Plane) + common emoji ranges.
 */
export function measureCharWidths(fontFamily: string, fontSize: number, fontWeight: number): Record<number, number> {
    // Create offscreen canvas for measurement
    const canvas = new OffscreenCanvas(1, 1)
    const ctx = canvas.getContext('2d')
    if (!ctx) {
        throw new Error('Failed to get canvas context')
    }

    // Set font
    ctx.font = `${fontWeight.toString()} ${fontSize.toString()}px ${fontFamily}`

    const widths: Record<number, number> = {}

    // BMP: U+0020 to U+FFFF (excluding control chars and private use areas)
    // We measure printable ranges only for efficiency
    const ranges: [number, number][] = [
        [0x0020, 0x007e], // Basic Latin
        [0x00a0, 0x00ff], // Latin-1 Supplement
        [0x0100, 0x017f], // Latin Extended-A
        [0x0180, 0x024f], // Latin Extended-B
        [0x0250, 0x02af], // IPA Extensions
        [0x02b0, 0x02ff], // Spacing Modifier Letters
        [0x0300, 0x036f], // Combining Diacritical Marks
        [0x0370, 0x03ff], // Greek and Coptic
        [0x0400, 0x04ff], // Cyrillic
        [0x0500, 0x052f], // Cyrillic Supplement
        [0x0530, 0x058f], // Armenian
        [0x0590, 0x05ff], // Hebrew
        [0x0600, 0x06ff], // Arabic
        [0x0700, 0x074f], // Syriac
        [0x0780, 0x07bf], // Thaana
        [0x0900, 0x097f], // Devanagari
        [0x0980, 0x09ff], // Bengali
        [0x0a00, 0x0a7f], // Gurmukhi
        [0x0a80, 0x0aff], // Gujarati
        [0x0b00, 0x0b7f], // Oriya
        [0x0b80, 0x0bff], // Tamil
        [0x0c00, 0x0c7f], // Telugu
        [0x0c80, 0x0cff], // Kannada
        [0x0d00, 0x0d7f], // Malayalam
        [0x0d80, 0x0dff], // Sinhala
        [0x0e00, 0x0e7f], // Thai
        [0x0e80, 0x0eff], // Lao
        [0x0f00, 0x0fff], // Tibetan
        [0x1000, 0x109f], // Myanmar
        [0x10a0, 0x10ff], // Georgian
        [0x1100, 0x11ff], // Hangul Jamo
        [0x1200, 0x137f], // Ethiopic
        [0x13a0, 0x13ff], // Cherokee
        [0x1400, 0x167f], // Unified Canadian Aboriginal Syllabics
        [0x1680, 0x169f], // Ogham
        [0x16a0, 0x16ff], // Runic
        [0x1700, 0x171f], // Tagalog
        [0x1720, 0x173f], // Hanunoo
        [0x1740, 0x175f], // Buhid
        [0x1760, 0x177f], // Tagbanwa
        [0x1780, 0x17ff], // Khmer
        [0x1800, 0x18af], // Mongolian
        [0x1900, 0x194f], // Limbu
        [0x1950, 0x197f], // Tai Le
        [0x1980, 0x19df], // New Tai Lue
        [0x19e0, 0x19ff], // Khmer Symbols
        [0x1a00, 0x1a1f], // Buginese
        [0x1b00, 0x1b7f], // Balinese
        [0x1d00, 0x1d7f], // Phonetic Extensions
        [0x1d80, 0x1dbf], // Phonetic Extensions Supplement
        [0x1e00, 0x1eff], // Latin Extended Additional
        [0x1f00, 0x1fff], // Greek Extended
        [0x2000, 0x206f], // General Punctuation
        [0x2070, 0x209f], // Superscripts and Subscripts
        [0x20a0, 0x20cf], // Currency Symbols
        [0x20d0, 0x20ff], // Combining Diacritical Marks for Symbols
        [0x2100, 0x214f], // Letterlike Symbols
        [0x2150, 0x218f], // Number Forms
        [0x2190, 0x21ff], // Arrows
        [0x2200, 0x22ff], // Mathematical Operators
        [0x2300, 0x23ff], // Miscellaneous Technical
        [0x2400, 0x243f], // Control Pictures
        [0x2440, 0x245f], // Optical Character Recognition
        [0x2460, 0x24ff], // Enclosed Alphanumerics
        [0x2500, 0x257f], // Box Drawing
        [0x2580, 0x259f], // Block Elements
        [0x25a0, 0x25ff], // Geometric Shapes
        [0x2600, 0x26ff], // Miscellaneous Symbols
        [0x2700, 0x27bf], // Dingbats
        [0x27c0, 0x27ef], // Miscellaneous Mathematical Symbols-A
        [0x27f0, 0x27ff], // Supplemental Arrows-A
        [0x2800, 0x28ff], // Braille Patterns
        [0x2900, 0x297f], // Supplemental Arrows-B
        [0x2980, 0x29ff], // Miscellaneous Mathematical Symbols-B
        [0x2a00, 0x2aff], // Supplemental Mathematical Operators
        [0x2b00, 0x2bff], // Miscellaneous Symbols and Arrows
        [0x3000, 0x303f], // CJK Symbols and Punctuation
        [0x3040, 0x309f], // Hiragana
        [0x30a0, 0x30ff], // Katakana
        [0x3100, 0x312f], // Bopomofo
        [0x3130, 0x318f], // Hangul Compatibility Jamo
        [0x3190, 0x319f], // Kanbun
        [0x31a0, 0x31bf], // Bopomofo Extended
        [0x31f0, 0x31ff], // Katakana Phonetic Extensions
        [0x3200, 0x32ff], // Enclosed CJK Letters and Months
        [0x3300, 0x33ff], // CJK Compatibility
        [0x3400, 0x4dbf], // CJK Unified Ideographs Extension A
        [0x4dc0, 0x4dff], // Yijing Hexagram Symbols
        [0x4e00, 0x9fff], // CJK Unified Ideographs
        [0xa000, 0xa48f], // Yi Syllables
        [0xa490, 0xa4cf], // Yi Radicals
        [0xa960, 0xa97f], // Hangul Jamo Extended-A
        [0xac00, 0xd7af], // Hangul Syllables
        [0xd7b0, 0xd7ff], // Hangul Jamo Extended-B
        [0xe000, 0xf8ff], // Private Use Area (skip - no standard glyphs)
        [0xf900, 0xfaff], // CJK Compatibility Ideographs
        [0xfb00, 0xfb4f], // Alphabetic Presentation Forms
        [0xfb50, 0xfdff], // Arabic Presentation Forms-A
        [0xfe00, 0xfe0f], // Variation Selectors
        [0xfe20, 0xfe2f], // Combining Half Marks
        [0xfe30, 0xfe4f], // CJK Compatibility Forms
        [0xfe50, 0xfe6f], // Small Form Variants
        [0xfe70, 0xfeff], // Arabic Presentation Forms-B
        [0xff00, 0xffef], // Halfwidth and Fullwidth Forms
        [0xfff0, 0xffff], // Specials
        // Common emoji ranges (outside BMP)
        [0x1f300, 0x1f5ff], // Miscellaneous Symbols and Pictographs
        [0x1f600, 0x1f64f], // Emoticons
        [0x1f680, 0x1f6ff], // Transport and Map Symbols
        [0x1f900, 0x1f9ff], // Supplemental Symbols and Pictographs
        [0x1fa00, 0x1fa6f], // Chess Symbols
        [0x1fa70, 0x1faff], // Symbols and Pictographs Extended-A
    ]

    // Skip Private Use Area (E000-F8FF) as mentioned in the list
    const skipRanges = new Set<number>()
    for (let i = 0xe000; i <= 0xf8ff; i++) {
        skipRanges.add(i)
    }

    let totalChars = 0
    for (const [start, end] of ranges) {
        for (let codePoint = start; codePoint <= end; codePoint++) {
            if (skipRanges.has(codePoint)) continue

            const char = String.fromCodePoint(codePoint)
            const metrics = ctx.measureText(char)
            widths[codePoint] = metrics.width
            totalChars++
        }
    }

    log.debug('Measured {totalChars} characters for {fontFamily} {fontWeight} {fontSize}px', {
        totalChars,
        fontFamily,
        fontWeight,
        fontSize,
    })
    return widths
}
