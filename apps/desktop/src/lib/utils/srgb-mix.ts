/**
 * Tiny sRGB color helpers. The whole point: derive accent-family tokens
 * (`--color-accent-hover`, `--color-accent-subtle`, etc.) and volume tints in
 * JS so the values are concrete hex/rgba strings by the time the cascade
 * resolves them.
 *
 * Why not lean on CSS `color-mix()` for those? Tauri's WKWebView tracks the
 * system Safari, and macOS 12 Monterey ships with Safari 15.x out of the box.
 * `color-mix()` arrived in Safari 16.2, `color-mix(in oklch, …)` in 16.4. On
 * older WebKit, any `color-mix()` declaration is invalid → the variable goes
 * unset → buttons hover to black, the file-list cursor row disappears, etc.
 * Computing the result in JS sidesteps the whole class of failure for
 * runtime-derived tokens.
 *
 * The math is plain sRGB linear-interpolation. Cmdr's design tokens were
 * authored with `color-mix(in srgb, …)` for most cases anyway; the few `oklch`
 * sites are visual niceties (`--color-accent-hover` is a tiny lightening) where
 * the sRGB approximation is indistinguishable in the UI.
 */

/** Parses `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`. Returns 0–255 channels + 0–1 alpha. */
export function parseHex(hex: string): { r: number; g: number; b: number; a: number } {
  const s = hex.trim().replace(/^#/, '')
  const expand = (c: string) => c + c
  let r: string,
    g: string,
    b: string,
    a = 'ff'
  if (s.length === 3) {
    ;[r, g, b] = [expand(s[0]), expand(s[1]), expand(s[2])]
  } else if (s.length === 4) {
    ;[r, g, b, a] = [expand(s[0]), expand(s[1]), expand(s[2]), expand(s[3])]
  } else if (s.length === 6) {
    ;[r, g, b] = [s.slice(0, 2), s.slice(2, 4), s.slice(4, 6)]
  } else if (s.length === 8) {
    ;[r, g, b, a] = [s.slice(0, 2), s.slice(2, 4), s.slice(4, 6), s.slice(6, 8)]
  } else {
    throw new Error(`Invalid hex color: ${hex}`)
  }
  return {
    r: parseInt(r, 16),
    g: parseInt(g, 16),
    b: parseInt(b, 16),
    a: parseInt(a, 16) / 255,
  }
}

/** Formats `{r,g,b}` as a `#rrggbb` string. */
export function toHex(r: number, g: number, b: number): string {
  const clamp = (n: number) => Math.max(0, Math.min(255, Math.round(n)))
  const h = (n: number) => clamp(n).toString(16).padStart(2, '0')
  return `#${h(r)}${h(g)}${h(b)}`
}

/**
 * Linearly interpolates two sRGB hex colors. `t` is the share of `b` (0..1):
 * `t=0` returns `a`, `t=1` returns `b`. Mirrors CSS `color-mix(in srgb, a, b t%)`.
 */
export function mixSrgb(a: string, b: string, t: number): string {
  const ca = parseHex(a)
  const cb = parseHex(b)
  return toHex(ca.r * (1 - t) + cb.r * t, ca.g * (1 - t) + cb.g * t, ca.b * (1 - t) + cb.b * t)
}

/**
 * Returns the given color with an explicit alpha as `rgba(r, g, b, a)`.
 * Mirrors CSS `color-mix(in srgb, <color>, transparent <100*(1-alpha)>%)` for
 * a single solid input.
 */
export function withAlpha(hex: string, alpha: number): string {
  const { r, g, b } = parseHex(hex)
  return `rgba(${String(Math.round(r))}, ${String(Math.round(g))}, ${String(Math.round(b))}, ${String(alpha)})`
}

/**
 * WCAG 2.2 relative luminance of an opaque sRGB color. Returns 0..1.
 *
 * The check-side analog lives in `scripts/check-a11y-contrast/contrast.go`;
 * keep the two in sync (this is a hand-rolled mirror, not shared code,
 * because the Go check has no JS dependency).
 */
export function relativeLuminance(hex: string): number {
  const { r, g, b } = parseHex(hex)
  const channel = (c: number) => {
    const v = c / 255
    return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4)
  }
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
}

/**
 * WCAG contrast ratio between two opaque sRGB hex colors, in [1, 21].
 */
export function contrastRatio(a: string, b: string): number {
  const la = relativeLuminance(a)
  const lb = relativeLuminance(b)
  const [light, dark] = la >= lb ? [la, lb] : [lb, la]
  return (light + 0.05) / (dark + 0.05)
}

/**
 * Picks black or white as the readable foreground for text on a given bg.
 *
 * Returns whichever of `#000000` / `#ffffff` gives the higher contrast.
 * Concrete tradeoffs:
 *   - Apple Blue (#087aff): black gives 5.24:1, white 4.01:1 → black.
 *   - Apple Purple (#a54fa7): black gives 4.28:1, white 4.91:1 → white.
 *   - Cmdr gold (#d4a006) / yellow / orange / green / pink / red / graphite:
 *     black is the clear winner.
 *
 * Apple Purple is the only system accent today where white reads better.
 * Without this auto-pick we'd ship a fixed dark `#1a1a1a` that fails AA on
 * Apple Purple AND Apple Blue (the macOS default), which is what shipped
 * before the contrast-check accent matrix surfaced it.
 */
export function readableFgOn(bgHex: string): '#000000' | '#ffffff' {
  return contrastRatio('#000000', bgHex) >= contrastRatio('#ffffff', bgHex) ? '#000000' : '#ffffff'
}

/** WCAG 1.4.11 for the fill against its field, and 1.4.3 for the text drawn on it. */
const SELECTION_VS_FIELD = 3
const SELECTION_VS_TEXT = 4.5
/** Aim this far inside the window, so the Go checker's mirror (a hair different threshold) agrees. */
const SELECTION_MARGIN = 0.0003

/**
 * The opaque text-selection fill for an accent: the accent shaded toward black or white until it
 * clears 3:1 against the field and 4.5:1 under the field's text. Mirrored in
 * `scripts/check-a11y-contrast/accent_matrix.go`, which gates both ratios for every accent.
 *
 * The two ratios bound the fill's luminance from both sides. Dark text on a light field leaves a
 * roomy window; light text on a dark field leaves one narrower than one 8-bit step of green, so
 * the rounded hex is searched for around the mix rather than trusted. When no shade clears both,
 * the text wins: a selection you can read beats one you can see.
 */
export function textSelectionFill(accentHex: string, on: { field: string; text: string }): string {
  const field = relativeLuminance(on.field)
  const text = relativeLuminance(on.text)
  const lightField = field >= text
  // Luminance bounds from each ratio. On a light field the fill sits below the field and above the
  // text; on a dark field, the other way round.
  const fromField = lightField ? (field + 0.05) / SELECTION_VS_FIELD - 0.05 : SELECTION_VS_FIELD * (field + 0.05) - 0.05
  const fromText = lightField ? SELECTION_VS_TEXT * (text + 0.05) - 0.05 : (text + 0.05) / SELECTION_VS_TEXT - 0.05
  let min = Math.min(fromField, fromText) + SELECTION_MARGIN
  let max = Math.max(fromField, fromText) - SELECTION_MARGIN
  const reachable = lightField ? fromText <= fromField : fromField <= fromText
  if (!reachable || min > max) {
    // Only the text's bound, from its own side.
    ;[min, max] = lightField ? [fromText + SELECTION_MARGIN, 1] : [0, fromText - SELECTION_MARGIN]
  }
  const target = (min + max) / 2
  const shaded = shadeToLuminance(accentHex, target)
  return nearestInWindow(shaded, min, max, target)
}

/** The accent mixed toward black or white until its luminance is `target`. */
function shadeToLuminance(hex: string, target: number): { r: number; g: number; b: number } {
  const c = parseHex(hex)
  const towardWhite = relativeLuminance(hex) < target
  const end = towardWhite ? 255 : 0
  const at = (t: number) => ({ r: c.r + (end - c.r) * t, g: c.g + (end - c.g) * t, b: c.b + (end - c.b) * t })
  let lo = 0
  let hi = 1
  for (let i = 0; i < 40; i++) {
    const mid = (lo + hi) / 2
    const { r, g, b } = at(mid)
    const overshot = towardWhite
      ? relativeLuminance(toHex(r, g, b)) >= target
      : relativeLuminance(toHex(r, g, b)) <= target
    if (overshot) hi = mid
    else lo = mid
  }
  return at(hi)
}

/** The rounded color nearest `c` whose luminance is in `[min, max]`, closest to `target` on ties. */
function nearestInWindow(c: { r: number; g: number; b: number }, min: number, max: number, target: number): string {
  const rounded = toHex(c.r, c.g, c.b)
  const l = relativeLuminance(rounded)
  if (l >= min && l <= max) return rounded
  let best = rounded
  let bestScore = Infinity
  for (let dr = -3; dr <= 3; dr++) {
    for (let dg = -3; dg <= 3; dg++) {
      for (let db = -3; db <= 3; db++) {
        const candidate = toHex(c.r + dr, c.g + dg, c.b + db)
        const cl = relativeLuminance(candidate)
        if (cl < min || cl > max) continue
        // Smallest nudge first (keeps the hue), then the luminance nearest the window's middle.
        const score = dr * dr + dg * dg + db * db + Math.abs(cl - target)
        if (score < bestScore) {
          best = candidate
          bestScore = score
        }
      }
    }
  }
  return best
}
