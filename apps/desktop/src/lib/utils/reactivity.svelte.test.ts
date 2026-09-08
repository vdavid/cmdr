/**
 * `dependOn` has an empty body, so the only thing worth testing is the thing that isn't obvious from
 * reading it: passing a `$state` value as an argument really does subscribe the enclosing effect.
 * Each case runs inside an `$effect.root`, standing in for the component scope the helper lives in.
 */

import { describe, it, expect } from 'vitest'
import { flushSync } from 'svelte'
import { dependOn } from './reactivity'

describe('dependOn', () => {
  it('re-runs an effect that reads a value only through it', () => {
    const cleanup = $effect.root(() => {
      let counter = $state(0)
      let runs = 0

      $effect(() => {
        dependOn(counter)
        runs++
      })
      flushSync()
      expect(runs).toBe(1)

      counter++
      flushSync()
      expect(runs).toBe(2)
    })
    cleanup()
  })

  it('subscribes to every argument, not only the first', () => {
    const cleanup = $effect.root(() => {
      const first = $state('a')
      let second = $state('b')
      let runs = 0

      $effect(() => {
        dependOn(first, second)
        runs++
      })
      flushSync()

      second = 'c'
      flushSync()
      expect(runs).toBe(2)
    })
    cleanup()
  })

  it('leaves an untouched value out of the dependency set', () => {
    const cleanup = $effect.root(() => {
      const watched = $state(0)
      let ignored = $state(0)
      let runs = 0

      $effect(() => {
        dependOn(watched)
        runs++
      })
      flushSync()

      ignored++
      flushSync()
      expect(ignored).toBe(1)
      expect(runs).toBe(1)
    })
    cleanup()
  })

  it('subscribes to a nested read, so a property path works as the dependency', () => {
    const cleanup = $effect.root(() => {
      const state = $state({ messages: ['one'] })
      let runs = 0

      $effect(() => {
        dependOn(state.messages.length)
        runs++
      })
      flushSync()

      state.messages.push('two')
      flushSync()
      expect(runs).toBe(2)
    })
    cleanup()
  })
})
