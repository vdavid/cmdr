import { describe, expect, it } from 'vitest'
import { findCssBeyondSafari15, supportsNotFallbacks } from './vite-webkit-css-floor.ts'

const find = (css: string, fallbacks?: Set<string>) => findCssBeyondSafari15(css, fallbacks).map((f) => f.feature)

describe('findCssBeyondSafari15', () => {
  it('flags each kind Safari 15.4 drops: property, keyword, function, selector, at-rule, media feature', () => {
    expect(find('.a{zoom:1;translate:1px;hanging-punctuation:first}.b{field-sizing:content}')).toEqual(['field-sizing'])
    expect(find('.a{display:flex;align-items:anchor-center}')).toEqual(['align-items: anchor-center'])
    expect(find('.a{color:light-dark(red,blue)}')).toEqual(['color: light-dark()'])
    expect(find('.a:user-invalid{color:red}')).toEqual([':user-invalid'])
    expect(find('@container (width > 1px){.a{color:red}}')).toEqual(['@container'])
    expect(find('@media (prefers-reduced-transparency: reduce){.a{color:red}}')).toEqual([
      '(prefers-reduced-transparency)',
    ])
  })

  it('passes what Safari 15.4 has, including `:has()` and `oklch()` (its own floor features)', () => {
    expect(find('.a:has(.b){color:oklch(50% 0.1 200);outline:1px solid;display:grid;gap:1px}')).toEqual([])
  })

  it('passes what the cascade covers: @supports, a supported -webkit- twin, an earlier fallback', () => {
    expect(find('@supports (field-sizing:content){.a{field-sizing:content}}')).toEqual([])
    expect(find('.a{-webkit-backdrop-filter:blur(2px);backdrop-filter:blur(2px)}')).toEqual([])
    expect(find('.a{color:red;color:light-dark(red,blue)}')).toEqual([])
  })

  it('passes a feature an `@supports not` block names, as the app writes its color-mix() fallbacks', () => {
    const fallbacks = supportsNotFallbacks('@supports not (color: color-mix(in oklch, red, blue)){.a{color:red}}')
    expect([...fallbacks].sort()).toEqual(['color', 'color-mix'])
    expect(find('.a{background:color-mix(in srgb,red,blue)}', fallbacks)).toEqual([])
    expect(find('.a{background:color-mix(in srgb,red,blue)}')).toEqual(['background: color-mix()'])
  })

  it('passes a keyword an `@supports not (property: keyword)` block names, and only that pair', () => {
    const fallbacks = supportsNotFallbacks('@supports not (contain: inline-size){.a{width:0;min-width:100%}}')
    expect(fallbacks.has('contain: inline-size')).toBe(true)
    expect(find('.a{contain:inline-size}', fallbacks)).toEqual([])
    expect(find('.a{overflow:clip}', fallbacks)).toEqual(['overflow: clip'])
  })

  it('reads `rect()` by its property: the legacy clip syntax is old, so it rests on the reviewed list', () => {
    // Regression anchor: `clip: rect(0,0,0,0)` in the visually-hidden utility once read as the Safari 17.2 shape.
    expect(find('.sr{clip:rect(0,0,0,0)}')).toEqual([])
    expect(find('.a{clip-path:rect(0 0 0 0)}')).toEqual(['clip-path: rect()'])
  })

  it('takes a partial implementation BCD later replaced as support, and BCD silence about a prefix as no finding', () => {
    // `outline` before Safari 16.4 didn't follow `border-radius`: partial, "removed" at 16.4, still there at 15.4.
    expect(find('.a{outline:2px solid red}')).toEqual([])
    expect(find('.a{-webkit-text-decoration:underline}')).toEqual([])
  })
})
