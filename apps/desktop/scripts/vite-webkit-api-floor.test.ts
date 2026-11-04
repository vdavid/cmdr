import { describe, expect, it } from 'vitest'
import { parseSync } from 'vite'
import { findApisBeyondSafari15 } from './vite-webkit-api-floor.ts'

const find = (code: string, chunk = 'chunk.js') =>
  findApisBeyondSafari15(parseSync('chunk.js', code, { sourceType: 'module' }).program, chunk).map((f) => f.api)

describe('findApisBeyondSafari15', () => {
  it('flags a static call, a prototype method, and a global constructor newer than Safari 15.4', () => {
    expect(find('let a=Object.groupBy(x,f),b=Promise.withResolvers();')).toEqual([
      'Object.groupBy',
      'Promise.withResolvers',
    ])
    expect(find('e.items.toSorted((a,b)=>a-b);new Set(a).union(b)')).toEqual(['toSorted', 'union'])
    expect(find('requestIdleCallback(t);let s=new CompressionStream(`gzip`)')).toEqual([
      'requestIdleCallback',
      'CompressionStream',
    ])
  })

  it('passes what Safari 15.4 has, and a mere read of a newer member', () => {
    expect(find('a.findLast(f);a.at(-1);Object.hasOwn(o,k);structuredClone(v);crypto.randomUUID()')).toEqual([])
    expect(find('let hasIt=typeof Object.groupBy;')).toEqual([])
  })

  it('passes a call inside a feature check that names it, in each shape the minifier emits', () => {
    // The shapes in the real bundle: `'x' in window ? x() : …`, `typeof x == 'function' ? () => x() : …`.
    expect(find('`requestIdleCallback`in window?requestIdleCallback(e):setTimeout(e,0)')).toEqual([])
    expect(
      find('var Lt=typeof requestIdleCallback==`function`?e=>{requestIdleCallback(()=>{e()})}:e=>{setTimeout(e,1)}'),
    ).toEqual([])
    expect(find('if(typeof OffscreenCanvas<`u`){t=new OffscreenCanvas(1,1)}')).toEqual([])
    expect(find('Object.groupBy&&Object.groupBy(a,f)')).toEqual([])
    expect(find('Object.groupBy?.(a,f);a.toSorted?.()')).toEqual([])
  })

  it('still flags a call in the branch the check rules out', () => {
    expect(find('typeof requestIdleCallback==`function`?setTimeout(e):requestIdleCallback(e)')).toEqual([
      'requestIdleCallback',
    ])
  })

  it('exempts a worker that is only ever started behind a check in the main bundle', () => {
    expect(
      find('self.onmessage=()=>{new OffscreenCanvas(1,1)}', '_app/immutable/workers/measure-worker-_Kr8.js'),
    ).toEqual([])
    expect(find('self.onmessage=()=>{new OffscreenCanvas(1,1)}', '_app/immutable/workers/other-x.js')).toEqual([
      'OffscreenCanvas',
    ])
  })
})
