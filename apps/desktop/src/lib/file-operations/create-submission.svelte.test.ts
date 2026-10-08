/**
 * The new-folder / new-file submit, from OK to how the create really ended
 * (ERR-AREUV): a slow volume is "still creating", never a failure, and an end
 * that arrives after the dialog closed speaks only when there's bad news.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { MutationWaitOptions } from '$lib/tauri-commands'
import { MutationFailure } from './mutation-error'
import { CreateSubmission } from './create-submission.svelte'

const addToast = vi.fn()
vi.mock('$lib/ui/toast', () => ({
  addToast: (...args: unknown[]) => {
    addToast(...args)
  },
}))

/** A create the test ends by hand, and says "still running" for on demand. */
function controllableCreate() {
  let wait: MutationWaitOptions = {}
  let land!: () => void
  let refuse!: (e: unknown) => void
  const create = vi.fn((_name: string, options: MutationWaitOptions) => {
    wait = options
    return new Promise<void>((resolve, reject) => {
      land = resolve
      refuse = reject
    })
  })
  return {
    create,
    stillRunning: () => wait.onStillRunning?.(),
    land: () => {
      land()
    },
    refuse: (e: unknown) => {
      refuse(e)
    },
  }
}

function setup() {
  const backend = controllableCreate()
  const onCreated = vi.fn()
  const onRefused = vi.fn()
  const submission = new CreateSubmission({ kind: 'folder', create: backend.create, onCreated, onRefused })
  return { backend, onCreated, onRefused, submission }
}

describe('CreateSubmission', () => {
  beforeEach(() => {
    addToast.mockClear()
  })

  it('goes busy at once and reports the landing', async () => {
    const { backend, onCreated, submission } = setup()
    const done = submission.submit('photos')
    expect(submission.phase).toBe('creating')

    backend.land()
    await done
    expect(onCreated).toHaveBeenCalledExactlyOnceWith('photos')
  })

  it('says it is still creating past the deadline, then lands like a normal create', async () => {
    const { backend, onCreated, submission } = setup()
    const done = submission.submit('photos')
    backend.stillRunning()
    expect(submission.phase).toBe('stillCreating')
    expect(onCreated).not.toHaveBeenCalled()

    backend.land()
    await done
    expect(onCreated).toHaveBeenCalledExactlyOnceWith('photos')
  })

  it('shows a late refusal inline and lets the person try again', async () => {
    const { backend, onRefused, submission } = setup()
    const done = submission.submit('photos')
    backend.stillRunning()
    backend.refuse(new MutationFailure({ type: 'alreadyExists', name: 'photos' }))
    await done

    expect(submission.phase).toBe('editing')
    expect(onRefused).toHaveBeenCalledOnce()
    expect(addToast).not.toHaveBeenCalled()
  })

  it('ignores a second submit while one is in flight', async () => {
    const { backend, submission } = setup()
    const first = submission.submit('photos')
    void submission.submit('photos')
    expect(backend.create).toHaveBeenCalledOnce()
    backend.land()
    await first
  })

  it("doesn't steer anything when the create lands after the dialog closed", async () => {
    const { backend, onCreated, submission } = setup()
    const done = submission.submit('photos')
    backend.stillRunning()
    submission.close()

    backend.land()
    await done
    expect(onCreated).not.toHaveBeenCalled()
    expect(addToast).not.toHaveBeenCalled()
  })

  it('says so in a toast when the create is refused after the dialog closed', async () => {
    const { backend, onRefused, submission } = setup()
    const done = submission.submit('photos')
    backend.stillRunning()
    submission.close()

    backend.refuse(new MutationFailure({ type: 'parentNotWritable', path: '/docs' }))
    await done
    expect(onRefused).not.toHaveBeenCalled()
    expect(addToast).toHaveBeenCalledOnce()
    expect(addToast.mock.calls[0][1]).toMatchObject({ level: 'error', dismissal: 'persistent' })
  })
})
