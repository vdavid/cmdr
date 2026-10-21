import { describe, it, expect, vi } from 'vitest'
import { createMountScope } from './mount-scope'

describe('createMountScope', () => {
  it('runs registered teardowns once on dispose, newest first', () => {
    const scope = createMountScope()
    const order: string[] = []
    expect(scope.add(() => order.push('first'))).toBe(true)
    expect(scope.add(() => order.push('second'))).toBe(true)

    scope.dispose()
    scope.dispose()

    expect(order).toEqual(['second', 'first'])
    expect(scope.isDisposed()).toBe(true)
  })

  it('runs a teardown added after dispose at once and reports the scope dead', () => {
    const scope = createMountScope()
    scope.dispose()
    const teardown = vi.fn()

    expect(scope.add(teardown)).toBe(false)
    expect(teardown).toHaveBeenCalledOnce()
  })
})
