import { describe, expect, it, vi } from 'vitest'
import { reloadWindow } from './reload-window'

describe('reloadWindow', () => {
  it('reloads the location it was handed', () => {
    const reload = vi.fn()
    reloadWindow({ reload })
    expect(reload).toHaveBeenCalledOnce()
  })
})
