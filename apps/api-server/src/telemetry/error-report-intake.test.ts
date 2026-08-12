import { describe, expect, it } from 'vitest'
import {
  DAILY_INTAKE_BUDGET_BYTES,
  DAILY_NOTIFICATION_CAP,
  INTAKE_PAUSED_KEY,
  checkIntakeAllowed,
  claimBudgetAlert,
  claimNotificationSlot,
  dailyBytesKey,
  isIntakePaused,
  pauseIntake,
  recordIntakeBytes,
  resumeIntake,
} from './error-report-intake'

/** In-memory KV stub. `expirationTtl` is accepted and ignored; nothing here asserts on expiry. */
function createKv(seed: Record<string, string> = {}): KVNamespace {
  const store = new Map<string, string>(Object.entries(seed))
  return {
    get: (key: string) => Promise.resolve(store.get(key) ?? null),
    put: (key: string, value: string) => {
      store.set(key, value)
      return Promise.resolve()
    },
    delete: (key: string) => {
      store.delete(key)
      return Promise.resolve()
    },
  } as unknown as KVNamespace
}

const today = '2026-08-10'

describe('daily intake budget', () => {
  it('accepts an upload when the day is fresh', async () => {
    const kv = createKv()

    await expect(checkIntakeAllowed(kv, today)).resolves.toEqual({ accept: true })
  })

  it('accepts an upload while the day is still under budget', async () => {
    const kv = createKv({ [dailyBytesKey(today)]: String(DAILY_INTAKE_BUDGET_BYTES - 1) })

    await expect(checkIntakeAllowed(kv, today)).resolves.toEqual({ accept: true })
  })

  it('rejects once the day has spent its budget', async () => {
    const kv = createKv({ [dailyBytesKey(today)]: String(DAILY_INTAKE_BUDGET_BYTES) })

    await expect(checkIntakeAllowed(kv, today)).resolves.toEqual({ accept: false, reason: 'daily_budget' })
  })

  it('counts each day separately, so a flood yesterday does not block today', async () => {
    const kv = createKv({ [dailyBytesKey('2026-08-09')]: String(DAILY_INTAKE_BUDGET_BYTES * 4) })

    await expect(checkIntakeAllowed(kv, today)).resolves.toEqual({ accept: true })
  })

  it('accumulates recorded bytes within the day', async () => {
    const kv = createKv()

    await recordIntakeBytes(kv, today, 1_000)
    const total = await recordIntakeBytes(kv, today, 2_500)

    expect(total).toBe(3_500)
    expect(await kv.get(dailyBytesKey(today))).toBe('3500')
  })
})

describe('intake pause', () => {
  it('reports intake as running by default', async () => {
    const kv = createKv()

    await expect(isIntakePaused(kv)).resolves.toBe(false)
  })

  it('rejects uploads while intake is paused, ahead of any budget check', async () => {
    const kv = createKv()
    await pauseIntake(kv)

    expect(await isIntakePaused(kv)).toBe(true)
    await expect(checkIntakeAllowed(kv, today)).resolves.toEqual({ accept: false, reason: 'paused' })
  })

  it('accepts uploads again after a resume', async () => {
    const kv = createKv()
    await pauseIntake(kv)
    await resumeIntake(kv)

    expect(await kv.get(INTAKE_PAUSED_KEY)).toBeNull()
    await expect(checkIntakeAllowed(kv, today)).resolves.toEqual({ accept: true })
  })
})

describe('notification fan-out cap', () => {
  it('notifies normally up to the daily cap', async () => {
    const kv = createKv()

    for (let i = 0; i < DAILY_NOTIFICATION_CAP; i++) {
      await expect(claimNotificationSlot(kv, today)).resolves.toBe('notify')
    }
  })

  it('posts exactly one suppression notice when the cap is passed', async () => {
    const kv = createKv()
    for (let i = 0; i < DAILY_NOTIFICATION_CAP; i++) await claimNotificationSlot(kv, today)

    await expect(claimNotificationSlot(kv, today)).resolves.toBe('suppress-notice')
    await expect(claimNotificationSlot(kv, today)).resolves.toBe('silent')
    await expect(claimNotificationSlot(kv, today)).resolves.toBe('silent')
  })

  it('gives each day a fresh allowance', async () => {
    const kv = createKv()
    for (let i = 0; i < DAILY_NOTIFICATION_CAP + 5; i++) await claimNotificationSlot(kv, today)

    await expect(claimNotificationSlot(kv, '2026-08-11')).resolves.toBe('notify')
  })
})

describe('budget alert claim', () => {
  it('lets the first caller of the day claim the alert', async () => {
    const kv = createKv()

    await expect(claimBudgetAlert(kv, today)).resolves.toBe(true)
  })

  it('denies every later caller the same day, so a flood pings once', async () => {
    const kv = createKv()

    await claimBudgetAlert(kv, today)

    await expect(claimBudgetAlert(kv, today)).resolves.toBe(false)
    await expect(claimBudgetAlert(kv, today)).resolves.toBe(false)
  })

  it('lets the next day claim its own alert', async () => {
    const kv = createKv()
    await claimBudgetAlert(kv, today)

    await expect(claimBudgetAlert(kv, '2026-08-11')).resolves.toBe(true)
  })
})
