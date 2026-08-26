/**
 * Admission control for `POST /error-report`: the global ceiling that the per-IP rate limiter
 * cannot provide.
 *
 * Two gates, both backed by `ERROR_REPORT_META` (KV):
 *
 * - **Daily byte budget** (`bytes_today:{yyyy-mm-dd}`): a running total of accepted bundle bytes
 *   for the UTC day. Past {@link DAILY_INTAKE_BUDGET_BYTES}, intake returns 503 for the rest of
 *   the day and pings Discord once. This bounds a distributed flood, which `ERROR_REPORT_LIMITER`
 *   cannot: Cloudflare counts rate limits per data center, not globally (see `enforceIpRateLimit`).
 * - **Intake pause** (`intake_paused`): a manual/automatic kill switch. Eviction sets it when the
 *   bucket is over its high watermark and nothing is old enough to evict, so the answer to a full
 *   bucket is "stop accepting", never "delete reports that are still fresh evidence"
 *   (`error-report-eviction.ts`).
 *
 * Both counters are read-then-write with no atomic increment (KV offers none), so a burst of
 * concurrent uploads can overshoot the budget by roughly the in-flight byte count. That is
 * deliberate: this is a coarse circuit breaker, and the bundle cap keeps a single overshoot small.
 * The same approximation runs the `total_bytes` counter.
 */

/**
 * Accepted bundle bytes allowed per UTC day. Legitimate traffic is orders of magnitude below this
 * (a handful of reports per user per day, each a few MB), so hitting it means something is wrong;
 * the Discord ping is the point as much as the rejection. It is also low enough that filling the
 * 8 GB eviction high watermark takes days of sustained flooding, each day alerting.
 */
export const DAILY_INTAKE_BUDGET_BYTES = 2 * 1024 ** 3 // 2 GB

/** KV key holding the intake kill switch. Present (any value) = paused. */
export const INTAKE_PAUSED_KEY = 'intake_paused'

/** KV key prefix for the per-day accepted-bytes counter. */
export const DAILY_BYTES_PREFIX = 'bytes_today:'

/** KV key prefix for the once-per-day "budget exhausted" alert claim. */
const BUDGET_ALERT_PREFIX = 'budget_alert:'

/** KV key prefix for the per-day count of error-report Discord notifications. */
const NOTIFY_COUNT_PREFIX = 'notify_count:'

/**
 * KV key prefix for the per-day count of error-report notification EMAILS. Deliberately its own
 * prefix: the two channels have wildly different budgets, and sharing a counter would let Discord
 * traffic silence the inbox.
 */
const EMAIL_COUNT_PREFIX = 'error_email_count:'

/**
 * Error-report Discord pings allowed per UTC day, after which the channel gets one "suppressing
 * the rest" notice and nothing more.
 *
 * A Discord webhook accepts 30 messages/min, and this endpoint posts one embed per accepted upload.
 * Without a cap, a burst that clears the per-IP limiter and the byte budget still drowns the
 * channel, and the notification that mattered scrolls away. Nothing is lost when this trips: every
 * bundle is still in R2 and listed by `GET /admin/error-reports`.
 */
export const DAILY_NOTIFICATION_CAP = 50

/**
 * Error-report notification emails allowed per UTC day, after which the inbox gets one "suppressing
 * the rest" notice and nothing more.
 *
 * Only hand-written (`kind: 'user'`) reports are mailed, and those run about four per 60 days, so
 * 10 in a single day is already ~150x the normal rate: a genuinely bad day (a broken release where
 * several people all write in at once) still arrives in full. The cap exists because `kind` comes
 * from the client's manifest, so a buggy or hostile build can label auto-sends as hand-written; the
 * ceiling bounds what that costs at 10 emails plus one notice. Nothing is lost when it trips: every
 * bundle is still in R2, pinged to Discord, and listed by `GET /admin/error-reports`.
 */
export const DAILY_ERROR_REPORT_EMAIL_CAP = 10

/**
 * Day-scoped keys outlive their day by a margin so a late-arriving request still lands on the
 * right counter, then expire on their own. No cleanup job needed.
 */
const DAY_KEY_TTL_SECONDS = 48 * 60 * 60

/** KV key for the accepted-bytes counter of one UTC day (`yyyy-mm-dd`). */
export function dailyBytesKey(date: string): string {
  return `${DAILY_BYTES_PREFIX}${date}`
}

/**
 * Why intake turned a request away. Carried as an explicit tag rather than inferred from a message:
 * the route maps it to a status and the log line reads it.
 */
export type IntakeRejection = 'paused' | 'daily_budget'

export type IntakeDecision = { accept: true } | { accept: false; reason: IntakeRejection }

/** True when the intake kill switch is set. */
export async function isIntakePaused(kv: KVNamespace): Promise<boolean> {
  return (await kv.get(INTAKE_PAUSED_KEY)) !== null
}

/**
 * Stop accepting new bundles until {@link resumeIntake}. Set by eviction when the bucket is full
 * of bundles too young to evict; also settable by hand for an incident.
 */
export async function pauseIntake(kv: KVNamespace): Promise<void> {
  await kv.put(INTAKE_PAUSED_KEY, '1')
}

/** Clear the kill switch. The daily cron does this once the bucket is back under its low watermark. */
export async function resumeIntake(kv: KVNamespace): Promise<void> {
  await kv.delete(INTAKE_PAUSED_KEY)
}

/**
 * Decide whether to accept one upload. Call before reading the body: a rejected request should
 * cost no parsing and no storage.
 */
export async function checkIntakeAllowed(kv: KVNamespace, date: string): Promise<IntakeDecision> {
  if (await isIntakePaused(kv)) return { accept: false, reason: 'paused' }

  const spent = parseInt((await kv.get(dailyBytesKey(date))) ?? '0', 10)
  if (spent >= DAILY_INTAKE_BUDGET_BYTES) return { accept: false, reason: 'daily_budget' }

  return { accept: true }
}

/** Add an accepted upload's size to the day's total. Returns the new total. */
export async function recordIntakeBytes(kv: KVNamespace, date: string, bytes: number): Promise<number> {
  const key = dailyBytesKey(date)
  const next = parseInt((await kv.get(key)) ?? '0', 10) + bytes
  await kv.put(key, String(next), { expirationTtl: DAY_KEY_TTL_SECONDS })
  return next
}

/**
 * What to do with the Discord ping for one accepted upload:
 * - `notify`: post the embed as usual.
 * - `suppress-notice`: post one line saying the rest of today's pings are suppressed.
 * - `silent`: post nothing.
 */
export type NotificationDecision = 'notify' | 'suppress-notice' | 'silent'

/**
 * Take one slot from a day's allowance on `key`. Exactly one caller per day gets `suppress-notice`,
 * so the channel learns it stopped hearing about uploads instead of going quiet without
 * explanation. Racy like the other KV counters, which at worst shifts the cutoff by a message or
 * two.
 */
async function claimDailySlot(kv: KVNamespace, key: string, cap: number): Promise<NotificationDecision> {
  const count = parseInt((await kv.get(key)) ?? '0', 10) + 1
  await kv.put(key, String(count), { expirationTtl: DAY_KEY_TTL_SECONDS })

  if (count <= cap) return 'notify'
  if (count === cap + 1) return 'suppress-notice'
  return 'silent'
}

/** Take one slot from the day's Discord-ping allowance ({@link DAILY_NOTIFICATION_CAP}). */
export async function claimNotificationSlot(kv: KVNamespace, date: string): Promise<NotificationDecision> {
  return claimDailySlot(kv, `${NOTIFY_COUNT_PREFIX}${date}`, DAILY_NOTIFICATION_CAP)
}

/** KV key for the notification-email count of one UTC day (`yyyy-mm-dd`). */
export function errorReportEmailCountKey(date: string): string {
  return `${EMAIL_COUNT_PREFIX}${date}`
}

/**
 * Take one slot from the day's notification-email allowance
 * ({@link DAILY_ERROR_REPORT_EMAIL_CAP}). Counted separately from the Discord allowance, so a
 * Discord burst can never suppress the inbox or the other way round.
 */
export async function claimErrorReportEmailSlot(kv: KVNamespace, date: string): Promise<NotificationDecision> {
  return claimDailySlot(kv, errorReportEmailCountKey(date), DAILY_ERROR_REPORT_EMAIL_CAP)
}

/**
 * Claim the right to send today's "budget exhausted" Discord ping. Returns true to exactly one
 * caller per day under normal conditions, so a sustained flood produces one alert rather than one
 * per rejected request. Racy like every KV counter here: a tie can produce two pings, which is a
 * far better failure than a silent drop or a webhook flood.
 */
export async function claimBudgetAlert(kv: KVNamespace, date: string): Promise<boolean> {
  const key = `${BUDGET_ALERT_PREFIX}${date}`
  if ((await kv.get(key)) !== null) return false
  await kv.put(key, '1', { expirationTtl: DAY_KEY_TTL_SECONDS })
  return true
}
