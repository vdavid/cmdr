// Which code points get measured up front, and which wait to be asked for.

/**
 * Unicode blocks measured eagerly, the moment a font size is first used.
 *
 * The set is deliberately small: it covers what filenames on a Latin-script
 * system actually contain, plus the symbol and emoji blocks a file manager
 * renders in its own chrome. Everything else (CJK, Hangul, Ethiopic, Myanmar,
 * the Indic and South-East Asian blocks, Braille, presentation forms, …) is
 * measured **on demand** instead: the backend reports the code points it
 * couldn't find, the frontend measures exactly those, and the widths are
 * correct from the next repaint on. See `DETAILS.md` § On-demand fill-in.
 *
 * Keep this list short. Every code point here is measured on the first use of
 * every text size, before anything else can happen; a code point left out
 * costs one background round-trip the first time a filename contains it.
 */
const EAGER_RANGES: readonly (readonly [number, number])[] = [
  [0x0020, 0x007e], // Basic Latin
  [0x00a0, 0x00ff], // Latin-1 Supplement
  [0x0100, 0x017f], // Latin Extended-A
  [0x0180, 0x024f], // Latin Extended-B
  [0x0250, 0x02af], // IPA Extensions
  [0x02b0, 0x02ff], // Spacing Modifier Letters
  [0x0300, 0x036f], // Combining Diacritical Marks (macOS stores filenames NFD, so these are everywhere)
  [0x0370, 0x03ff], // Greek and Coptic
  [0x0400, 0x04ff], // Cyrillic
  [0x0500, 0x052f], // Cyrillic Supplement
  [0x0590, 0x05ff], // Hebrew
  [0x0600, 0x06ff], // Arabic
  [0x1e00, 0x1eff], // Latin Extended Additional
  [0x2000, 0x206f], // General Punctuation
  [0x2070, 0x209f], // Superscripts and Subscripts
  [0x20a0, 0x20cf], // Currency Symbols
  [0x2100, 0x214f], // Letterlike Symbols
  [0x2150, 0x218f], // Number Forms
  [0x2190, 0x21ff], // Arrows
  [0x2200, 0x22ff], // Mathematical Operators
  [0x2300, 0x23ff], // Miscellaneous Technical
  [0x2500, 0x257f], // Box Drawing
  [0x2580, 0x259f], // Block Elements
  [0x25a0, 0x25ff], // Geometric Shapes
  [0x2600, 0x26ff], // Miscellaneous Symbols
  [0x2700, 0x27bf], // Dingbats
  [0xfe00, 0xfe0f], // Variation Selectors (emoji presentation)
  [0xff00, 0xffef], // Halfwidth and Fullwidth Forms
  [0x1f300, 0x1f5ff], // Miscellaneous Symbols and Pictographs
  [0x1f600, 0x1f64f], // Emoticons
  [0x1f680, 0x1f6ff], // Transport and Map Symbols
  [0x1f900, 0x1f9ff], // Supplemental Symbols and Pictographs
  [0x1fa70, 0x1faff], // Symbols and Pictographs Extended-A
]

/**
 * Surrogate halves (U+D800–U+DFFF). Never valid on their own, and
 * `String.fromCodePoint` throws on them, so any code point the backend reports
 * from a lone surrogate must be dropped rather than measured.
 */
const SURROGATE_START = 0xd800
const SURROGATE_END = 0xdfff

/** Highest code point Unicode defines. */
const MAX_CODE_POINT = 0x10ffff

/**
 * True when a code point can be turned into a string and measured. Filters the
 * backend's fill-in requests, which come from arbitrary filename bytes.
 */
export function isMeasurable(codePoint: number): boolean {
  return (
    Number.isInteger(codePoint) &&
    codePoint >= 0 &&
    codePoint <= MAX_CODE_POINT &&
    !(codePoint >= SURROGATE_START && codePoint <= SURROGATE_END)
  )
}

/**
 * Expands `EAGER_RANGES` into a flat, ascending list of code points.
 *
 * Built fresh on each call; the one caller memoizes the measured result per
 * font ID, so this runs once per font size at most.
 */
export function eagerCodePoints(): Uint32Array {
  let total = 0
  for (const [start, end] of EAGER_RANGES) {
    total += end - start + 1
  }

  const out = new Uint32Array(total)
  let i = 0
  for (const [start, end] of EAGER_RANGES) {
    for (let codePoint = start; codePoint <= end; codePoint++) {
      if (!isMeasurable(codePoint)) continue
      out[i++] = codePoint
    }
  }
  return i === out.length ? out : out.subarray(0, i)
}
