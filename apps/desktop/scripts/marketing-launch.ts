/**
 * The launch layer both marketing captures share: `marketing-shots.ts` (the stills in
 * `brand/screenshots/`) and `marketing-clips.ts` (the website's looping clips).
 *
 * Each capture runs a prod-looking app on a persistent data dir of its own, with its warm
 * drive index, its suppressions, and an en-US locale, then drives it from a Playwright
 * shard of its own. Everything that makes that launch safe lives here once.
 *
 * Why it differs from `i18n-capture.ts`:
 *
 * - **`CMDR_E2E_MODE` stays unset.** That variable paints the blue `E2E MODE` title
 *   bar AND sets `ActivationPolicy::Prohibited`, which makes the app permanently
 *   unable to become the key window. A marketing master needs both the prod-looking
 *   title bar and the key-window shadow, so an E2E-mode run could not produce one even
 *   in principle. The Playwright plugin is gated on the cargo FEATURE, not the mode
 *   (`src-tauri/src/lib.rs:285-292`), so the socket still works.
 * - **`CMDR_E2E_START_PATH` stays unset**, and the shards skip the fixture machinery.
 *   The suite's post-test guard deletes anything not in the fixture manifest, and these
 *   runs show REAL folders. See `apps/desktop/test/e2e-playwright/DETAILS.md`
 *   § "Real data comes from the data dir".
 * - **The data dir persists.** Its index stays warm, so the whole-drive reconcile that
 *   makes every size cell an hourglass is a one-time cost rather than a 20-minute tax
 *   on every round.
 */

import { spawn, spawnSync } from 'node:child_process'
import type { ChildProcess, SpawnSyncOptions } from 'node:child_process'
import { existsSync, mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs'
import { homedir } from 'node:os'
import { backup, DatabaseSync } from 'node:sqlite'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { reserveFreePort, waitForSocket } from './capture-runtime.ts'
import { EN_US_LOCALE_ARGS, OVERLAY_SCROLLBAR_ARGS, pinUiLanguage } from '../test/e2e-shared/pin-locale.ts'

const here = dirname(fileURLToPath(import.meta.url))
const desktopDir = join(here, '..')

/** The two capture instances. Each has its own data and cache dir, so neither can disturb the other. */
export type CaptureInstanceName = 'shots' | 'clips'

export interface CaptureInstance {
  name: CaptureInstanceName
  /**
   * The instance's own data dir, deliberately OUTSIDE the repo and deliberately not
   * throwaway.
   *
   * Its own path so it can never collide with prod, plain dev, a `--worktree` dev
   * instance, or an E2E shard; and persistent so its file index stays warm. A cold dir
   * costs one whole-drive reconcile, during which every size cell shows an hourglass and
   * every folder size reads `≥` — which is what a whole round of unusable masters looks
   * like.
   */
  dataDir: string
  /**
   * The instance's cache dir, passed as `CMDR_CACHE_DIR`. Its `drive-index/` holds the
   * warm file index, out of Time Machine like production's (`config.rs` `drive_index_dir`).
   */
  cacheDir: string
}

export function captureInstance(name: CaptureInstanceName): CaptureInstance {
  const id = `com.veszelovszki.cmdr-${name}`
  return {
    name,
    dataDir: join(homedir(), 'Library', 'Application Support', id),
    cacheDir: join(homedir(), 'Library', 'Caches', id),
  }
}

export function run(cmd: string, args: string[], opts: SpawnSyncOptions = {}): void {
  const res = spawnSync(cmd, args, { cwd: desktopDir, stdio: 'inherit', ...opts })
  if (res.status !== 0) throw new Error(`${cmd} ${args.join(' ')} exited ${String(res.status)}`)
}

/**
 * Writes the instance's settings on first run, and leaves them alone after.
 *
 * Every entry here is something a prod-mode launch would otherwise put ON TOP of a
 * master, or send somewhere it shouldn't. `isE2eRun()` is false by design in this run
 * (see the file header), and these are the suppressions that come back with it:
 *
 * - `analytics.enabled` off, because a release build points at the PRODUCTION analytics
 *   server and a fresh data dir mints a new install id, i.e. a phantom user.
 * - `updates.autoCheck` off, because its poll loop can raise a "Restart to update"
 *   toast over a shot.
 * - `whatsNew.showOnUpdate` off and the upgrade nudge marked shown: on a fresh data dir
 *   both fire once, which would be precisely the run that proves the pipeline.
 * - The cosmetics are the look the masters are supposed to have, set explicitly rather
 *   than inherited from whatever a previous round left behind.
 *
 * Not written after the first run, so David can adjust the instance by hand (pane
 * paths, favorites, tabs) and keep it.
 *
 * ❗ Which is why anything a master is JUDGED on (view modes, size colors and units,
 * theme) is staged by the spec instead, on every run: a look seeded here can't be
 * changed later without deleting the instance, and a data dir cloned from the
 * production app arrives with its owner's choices already in it.
 */
function seedSettingsIfNew(instance: CaptureInstance, logPrefix: string): void {
  const settingsPath = join(instance.dataDir, 'settings.json')
  if (existsSync(settingsPath)) return
  console.log(`${logPrefix} first run: seeding ${settingsPath}`)
  writeFileSync(
    settingsPath,
    `${JSON.stringify(
      {
        isOnboarded: true,
        'analytics.enabled': false,
        'updates.autoCheck': false,
        'whatsNew.showOnUpdate': false,
        'onboarding.upgradeNudgeShown': true,
        'appearance.appColor': 'cmdr-gold',
        'appearance.showFunctionKeyBar': true,
        'mediaIndex.enabled': false,
      },
      null,
      2,
    )}\n`,
  )
}

/**
 * Settings the app must launch with on every run, merged over whatever the instance holds.
 *
 * Applied on every run because the instance persists: a suppression added to
 * `seedSettingsIfNew` would never reach an instance that already exists. Each capture
 * names its own; `askCmdr.proactive` is off for both, because an accepted consent plus
 * the `CMDR_E2E_ASK_CMDR_FAKE` provider would otherwise let a wake start a thread of its
 * own mid-run, or put its spinner in the status corner of a master.
 *
 * `whatsNew.lastSeenVersion` is this build's version for both: the "What's new" popup
 * shows a security advisory fixed since the last version seen even with
 * `whatsNew.showOnUpdate` off, and a build newer than the instance's last run opened on
 * it, blocking every take (2026-11-03).
 */
function pinRunSettings(instance: CaptureInstance, runSettings: Record<string, unknown>): void {
  const settingsPath = join(instance.dataDir, 'settings.json')
  const settings = existsSync(settingsPath)
    ? (JSON.parse(readFileSync(settingsPath, 'utf8')) as Record<string, unknown>)
    : {}
  const appVersion = (JSON.parse(readFileSync(join(here, '..', 'package.json'), 'utf8')) as { version: string }).version
  Object.assign(settings, { 'askCmdr.proactive': false, 'whatsNew.lastSeenVersion': appVersion }, runSettings)
  writeFileSync(settingsPath, `${JSON.stringify(settings, null, 2)}\n`)
}

/** Where the installed production app keeps its warm drive index. */
const PROD_DRIVE_INDEX_DIR = join(homedir(), 'Library', 'Caches', 'com.veszelovszki.cmdr', 'drive-index')
/** Where a production build older than the move to the cache dir still keeps it. */
const PROD_LEGACY_INDEX_DIR = join(homedir(), 'Library', 'Application Support', 'com.veszelovszki.cmdr')

/**
 * The boot disk's index file. `root` is the literal volume id the boot volume always
 * resolves to (`cmdr-fs/src/volume/ids.rs`), and volume ids are derived from the
 * machine's own UUIDs, so the same filename means the same drive in every instance.
 */
const ROOT_INDEX_DB = 'index-root.db'

/**
 * The FSEvents id an index database has caught up to, 0 when it can't say: the number
 * the app's startup gate compares against the system's to decide whether to rebuild.
 */
function storedEventId(path: string): number {
  try {
    const db = new DatabaseSync(path, { readOnly: true })
    try {
      const row = db.prepare("SELECT value FROM meta WHERE key='last_event_id'").get()
      return Number(row?.value) || 0
    } finally {
      db.close()
    }
  } catch {
    return 0
  }
}

/**
 * Copies production's warm drive index into the capture instance, so folder sizes are
 * real numbers from the first frame instead of hourglasses.
 *
 * Why this is safe rather than a hack: nothing in the index encodes a data dir, an
 * instance id, or a bundle id, and the startup freshness check compares FSEvents event
 * ids against a MACHINE-GLOBAL counter. So the copied index is judged by exactly the
 * rule production's own restart would use. If it is too far behind (a gap over ten
 * million events) or was written by a different schema version, the app throws it away
 * and does a full scan, which is the fallback either way.
 *
 * ❗ SQLite's backup API in ONE step, not `cp`. The index runs in WAL mode, so a plain
 * copy of the `.db` while production has it open is a torn read — the repo says so at
 * `index-size-probe.rs` and in `docs/tooling/index-query.md`. One step copies every page
 * under a single read lock, so it is a consistent snapshot; measured at 1.9 s for 1.1 GB
 * against a running production app (2026-10-26).
 * ❌ Not the `sqlite3` CLI's `.backup`: it copies 100 pages per step, and the backup
 * RESTARTS whenever another connection writes the source between steps. A running
 * production Cmdr writes its index every few seconds, so that copy looped for 18 minutes
 * at full CPU. ❌ Not `VACUUM INTO` either: it rebuilds the indexes, which need Cmdr's
 * custom `platform_case` collation, so it fails outright.
 * The `-wal` and `-shm` sidecars are deliberately NOT carried across: the destination
 * recreates them, and a stale pair would contradict the snapshot.
 *
 * ❗ The copy skips the wait only when production is current. Startup compares the
 * index's stored FSEvents `last_event_id` against the system's current one and rebuilds
 * when the gap passes ten million (`JOURNAL_GAP_THRESHOLD`). On a machine that compiles
 * all day that gap is about an hour (14 million in 90 minutes with two E2E builds,
 * 2026-10-26), so the instance's own index is usually too old by the next run, while a
 * RUNNING production app keeps its own seconds behind. Copy that one in and the launch
 * replays the journal instead of rebuilding. A production app that was quit hours ago
 * still leaves a reconcile, and then the copy buys CONTENT: the reconcile walks a
 * populated index instead of building one from nothing. Either way the wait is the
 * spec's job (`waitForIndexedSizes`), which is what guarantees no hourglass reaches a
 * master.
 *
 * ❌ Don't "fix" this by writing the current event id into the copy. That claims the
 * index has seen changes it hasn't, and the app would then trust stale sizes forever
 * instead of for one reconcile.
 *
 * Best-effort by design. No production install or no readable index both fall through
 * to a normal scan with a line saying so.
 */
async function cloneProdIndex(instance: CaptureInstance, logPrefix: string): Promise<void> {
  const driveIndexDir = join(instance.cacheDir, 'drive-index')
  const source =
    [PROD_DRIVE_INDEX_DIR, PROD_LEGACY_INDEX_DIR]
      .map((dir) => join(dir, ROOT_INDEX_DB))
      .find((path) => existsSync(path)) ?? join(PROD_DRIVE_INDEX_DIR, ROOT_INDEX_DB)
  const target = join(driveIndexDir, ROOT_INDEX_DB)
  mkdirSync(driveIndexDir, { recursive: true })
  if (!existsSync(source)) {
    console.log(`${logPrefix} no production index at ${source}; the instance will scan the drive itself (~1 min).`)
    return
  }
  // Never trade a fresher index for an older one, judged by the same number the app's
  // startup gate reads. ❌ Not `scan_completed_at`: a running production Cmdr keeps its
  // `last_event_id` current through its live watcher without ever finishing a new scan,
  // so comparing scan times kept a capture index 14 M events behind (a five-minute
  // rebuild) over a production one that was seconds behind (2026-10-26).
  const sourceEventId = storedEventId(source)
  const targetEventId = existsSync(target) ? storedEventId(target) : 0
  if (targetEventId >= sourceEventId) {
    console.log(`${logPrefix} the instance's index is at least as fresh as production's; keeping it.`)
    return
  }

  const incoming = `${target}.incoming`
  rmSync(incoming, { force: true })
  try {
    const db = new DatabaseSync(source, { readOnly: true })
    try {
      // A page budget above any real index makes it ONE step (the ❗ above says why).
      await backup(db, incoming, { rate: 2 ** 31 - 1 })
    } finally {
      db.close()
    }
  } catch (err) {
    rmSync(incoming, { force: true })
    console.warn(`${logPrefix} couldn't copy the production index (${(err as Error).message}); scanning.`)
    return
  }

  renameSync(incoming, target)
  rmSync(`${target}-wal`, { force: true })
  rmSync(`${target}-shm`, { force: true })
  console.log(`${logPrefix} copied production's drive index in; sizes should be real from the first frame.`)
}

/**
 * Readies the instance's data dir for a launch: seeds it on first run, pins the UI
 * language and the run settings, and copies a warm index in.
 */
export async function prepareCaptureInstance(
  instance: CaptureInstance,
  logPrefix: string,
  runSettings: Record<string, unknown>,
): Promise<void> {
  mkdirSync(instance.dataDir, { recursive: true })
  seedSettingsIfNew(instance, logPrefix)
  // Every run, not just the first: the language is something a master is JUDGED on,
  // and the app would otherwise resolve it from whatever the machine's preference
  // list says. The masters are English. Merges, so a hand-adjusted instance keeps
  // its pane paths, tabs, and favorites.
  pinUiLanguage(instance.dataDir)
  pinRunSettings(instance, runSettings)
  // Before the launch: the app opens its index at startup, so a copy afterwards is a copy
  // the running instance never reads.
  await cloneProdIndex(instance, logPrefix)
}

/** A launched capture app, and the env its Playwright shard needs to reach it. */
export interface LaunchedCaptureApp {
  pid: number
  /** `CMDR_PLAYWRIGHT_SOCKET` and `CMDR_MCP_PORT` / `CMDR_MCP_ENABLED`, plus the caller's extras. */
  sharedEnv: Record<string, string>
  /** Stops ONLY this app. Safe to call twice. */
  stop: () => void
}

/**
 * Launches the Playwright-enabled binary on `instance` and waits for its socket.
 *
 * `extraEnv` reaches both the app and, through `sharedEnv`, the shard.
 */
export async function launchCaptureApp(options: {
  binary: string
  instance: CaptureInstance
  logPrefix: string
  extraEnv: Record<string, string>
}): Promise<LaunchedCaptureApp> {
  const { binary, instance, logPrefix, extraEnv } = options
  const socket = `/tmp/tauri-playwright-${instance.name}-${String(process.pid)}.sock`
  const mcpPort = await reserveFreePort()
  const sharedEnv = {
    CMDR_PLAYWRIGHT_SOCKET: socket,
    CMDR_MCP_PORT: String(mcpPort),
    CMDR_MCP_ENABLED: '1',
    ...extraEnv,
  }

  console.log(`${logPrefix} launching the app; data dir ${instance.dataDir}, MCP port ${String(mcpPort)}…`)
  // en-US for this process alone, so a master never shows the developer's own
  // region's dates and number grouping. Overlay scrollbars too, so an awake mouse can't
  // put classic scrollbars in a master. See `test/e2e-shared/pin-locale.ts`.
  let appProc: ChildProcess | null = spawn(binary, [...EN_US_LOCALE_ARGS, ...OVERLAY_SCROLLBAR_ARGS], {
    cwd: desktopDir,
    stdio: 'inherit',
    env: {
      ...process.env,
      ...sharedEnv,
      CMDR_DATA_DIR: instance.dataDir,
      CMDR_CACHE_DIR: instance.cacheDir,
      // Keeps a Keychain approval dialog from landing over a shot. Without E2E mode
      // the app would otherwise talk to the REAL macOS Keychain
      // (`secrets/mod.rs:103-120`), and this override is checked before that branch.
      CMDR_SECRET_STORE: 'file',
      // The chat shot runs off a seeded thread, so no provider is ever called. This
      // keeps the composer from rendering its "provider off" hint over the shot.
      CMDR_E2E_ASK_CMDR_FAKE: '1',
      // No analytics hook is needed here: `CMDR_DATA_DIR` above IS the contract.
      // `analytics::suppression_reason` reads an isolated data dir as proof this isn't a
      // real install, and `every_tooling_launcher_is_recognized` pins this launcher's
      // exact env. The seeded `analytics.enabled: false` is the belt to that suspenders.
      // The same list gates the macOS update check, so this launcher stays out of
      // `update_checks` too, whether or not it seeds `updates.autoCheck: false`.
      // ❗ Deliberately absent: CMDR_E2E_MODE (would make the window permanently
      // unfocusable and paint the blue title bar) and CMDR_E2E_START_PATH (would arm
      // the fixture guard against real folders). See this file's header.
    },
  })
  appProc.on('exit', (code) => {
    if (code != null && code !== 0) console.warn(`${logPrefix} app exited with code ${String(code)}`)
  })

  /**
   * Stops ONLY the app this launched.
   *
   * ❌ Never `pkill -f 'target.*Cmdr'`: that pattern matches every worktree's Cmdr, and
   * it has killed a parallel session's app mid-shoot twice. We spawned the process, so
   * its pid is the exact thing to signal.
   */
  const stop = (): void => {
    const pid = appProc?.pid
    appProc = null
    if (pid === undefined) return
    try {
      process.kill(pid, 'SIGTERM')
    } catch {
      // Already gone; ESRCH is the normal way this ends.
    }
  }

  try {
    await waitForSocket(socket, 60000)
  } catch (e) {
    stop()
    throw e
  }
  return { pid: appProc.pid ?? 0, sharedEnv, stop }
}

/**
 * Runs one capture shard of the Playwright config against a launched app.
 *
 * No `--project`: Playwright treats a positional as a project filter when `--project`
 * is set, and the shard's own `testMatch` already restricts the run.
 */
export function runCaptureShard(shardKind: string, env: Record<string, string>, extraArgs: string[] = []): void {
  run('npx', ['playwright', 'test', '--config', 'test/e2e-playwright/playwright.config.ts', ...extraArgs], {
    env: { ...process.env, ...env, CMDR_E2E_SHARD_KIND: shardKind },
  })
}
