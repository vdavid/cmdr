/**
 * The new-folder / new-file submit, from OK to how the create really ended
 * (ERR-AREUV): a slow volume is "still creating", never a failure, and an end
 * that arrives after the dialog closed speaks only when there's bad news.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { MutationWaitOptions } from '#lib/tauri-commands/index.js'
import { MutationFailure } from './mutation-error'
import { CreateSubmission } from './create-submission.svelte'
import type { NewEntryNameCheck } from './new-entry-name-check.svelte'

const addToast = vi.fn()
vi.mock('#lib/ui/toast/index.js', () => ({
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

/** The name check, as far as a tool's press reads it. */
function checkAnswering(errorMessage: string) {
  return { errorMessage, validate: vi.fn(() => Promise.resolve()) } as unknown as NewEntryNameCheck
}

describe('a press from the MCP `dialog_confirm`', () => {
  it('answers once the create landed', async () => {
    const { backend, onCreated, submission } = setup()
    const press = submission.pressFromTool('photos', checkAnswering(''))
    await vi.waitFor(() => {
      expect(backend.create).toHaveBeenCalled()
    })

    backend.land()

    expect(await press).toEqual({ kind: 'done' })
    expect(onCreated).toHaveBeenCalledExactlyOnceWith('photos')
  })

  it('answers while a slow volume is still on it, and the create carries on', async () => {
    const { backend, onCreated, submission } = setup()
    const press = submission.pressFromTool('photos', checkAnswering(''))
    await vi.waitFor(() => {
      expect(backend.create).toHaveBeenCalled()
    })

    backend.stillRunning()

    expect(await press).toEqual({ kind: 'stillRunning' })
    expect(submission.phase).toBe('stillCreating')
    backend.land()
    await vi.waitFor(() => {
      expect(onCreated).toHaveBeenCalled()
    })
  })

  it('refuses what the field would refuse, with its sentence, and creates nothing', async () => {
    const { backend, submission } = setup()
    const check = checkAnswering('There is already a folder by this name in this folder.')

    expect(await submission.pressFromTool('photos', check)).toEqual({
      kind: 'refused',
      reason: 'There is already a folder by this name in this folder.',
    })
    expect(backend.create).not.toHaveBeenCalled()
  })

  it('refuses an empty name', async () => {
    const { backend, submission } = setup()

    const end = await submission.pressFromTool('', checkAnswering(''))

    expect(end.kind).toBe('refused')
    expect(backend.create).not.toHaveBeenCalled()
  })

  it('carries a volume’s refusal, with the dialog back on editing', async () => {
    const { backend, onRefused, submission } = setup()
    const check = checkAnswering('')
    onRefused.mockImplementation((refusal: { message: string }) => {
      check.errorMessage = refusal.message
    })
    const press = submission.pressFromTool('photos', check)
    await vi.waitFor(() => {
      expect(backend.create).toHaveBeenCalled()
    })

    backend.refuse(new Error('no'))

    const end = await press
    expect(end.kind).toBe('refused')
    expect(submission.phase).toBe('editing')
  })
})

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
