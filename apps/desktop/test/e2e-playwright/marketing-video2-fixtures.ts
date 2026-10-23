/**
 * The second demo video's "anywhere your files live" fixtures, dressed to look like a real
 * person's setup: the repo's Docker SMB and S3 test servers refilled with family photos, a
 * restic repo called `Backups`, and the virtual MTP phone's backing dir. Staged by
 * `pnpm marketing:clips --video2` before the app launches (the phone's after, since the
 * app reseeds its backing dir at startup).
 *
 * ❗ The servers are the shared E2E fixtures (`test/smb-servers/`, `test/s3-servers/`). The
 * SMB share's seed files are deleted and replaced here; a container restart puts them
 * back, which is how the E2E suites get them again.
 */

import { spawnSync } from 'node:child_process'
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

/** The SMB fixture with a password, `test/smb-servers/` (`smb-consumer-auth`). */
export const SMB = {
  host: '127.0.0.1',
  port: 11481,
  share: 'private',
  username: 'testuser',
  password: 'testpass',
  /** What the Servers list and the volume picker call it. */
  name: 'Home NAS',
} as const

/** The VersityGW S3 fixture, `test/s3-servers/`. */
export const S3 = {
  endpoint: 'http://127.0.0.1:14480',
  region: 'us-east-1',
  accessKey: 'GK00000000000000000000c0de',
  secretKey: 'c0de'.repeat(16),
  bucket: 'family-photos',
} as const

/**
 * The `s3` beat's priced bucket: Cmdr's Cloudflare R2 test bucket (`test/s3-servers/live-env.sh`),
 * so the copy dialog shows a real cost estimate. B2, Wasabi, Hetzner, and Spaces price
 * every request at $0 and an upload has no egress, so their line always rounds to zero
 * and hides. R2 reads "About $0.01" from ~1,112 Class A requests, hence `THUMBNAILS`.
 *
 * ❗ The beat writes only under `prefix` (obviously a demo's), and the run deletes it.
 */
export const PRICED_S3 = {
  bucket: 'cmdr-s3-test',
  prefix: 'cmdr-clips-demo',
  album: 'family-photos',
  /** The account's name in Servers. The volume picker names the place by its bucket. */
  displayName: 'Family photos',
  /** The local folder the beat uploads, and how many tiny files it holds. */
  thumbnails: 'Thumbnails',
  count: 1_300,
} as const

/** The R2 account the beat signs in to. The secret stays in the instance's store. */
export interface PricedS3Account {
  accountId: string
  accessKeyId: string
}

/**
 * One value from David's sops store through the `secret` CLI, or `null` without it.
 * ❗ Captured, never printed: a failure message carries only the name.
 */
function readSecret(name: string): string | null {
  const res = spawnSync('secret', [name], { encoding: 'utf8' })
  if (res.error !== undefined || res.status !== 0) return null
  const value = res.stdout.trim()
  return value === '' ? null : value
}

/** The R2 account id, asked of Cloudflare's API with the account's token (as `live-env.sh` does). */
async function r2AccountId(): Promise<string | null> {
  const token = readSecret('CLOUDFLARE_API_TOKEN')
  if (token === null) return null
  const res = await fetch('https://api.cloudflare.com/client/v4/accounts', {
    headers: { Authorization: `Bearer ${token}` },
  })
  if (!res.ok) return null
  const body = (await res.json()) as { result?: { id?: string }[] }
  return body.result?.[0]?.id ?? null
}

function r2Endpoint(account: PricedS3Account): string {
  return `https://${account.accountId}.r2.cloudflarestorage.com`
}

/** Where the app's file-backed secret store keys the account (`S3ConnectionParams::credential_service`). */
function r2SecretKey(account: PricedS3Account): string {
  return `smb://s3+https://${account.accountId}.r2.cloudflarestorage.com:443/${account.accessKeyId}`
}

/** Runs the aws CLI against R2, the credentials in its env only. */
function r2Cli(account: PricedS3Account, secretKey: string, args: string[]): string {
  return run('aws', ['--endpoint-url', r2Endpoint(account), ...args], {
    AWS_ACCESS_KEY_ID: account.accessKeyId,
    AWS_SECRET_ACCESS_KEY: secretKey,
    AWS_DEFAULT_REGION: 'auto',
    AWS_EC2_METADATA_DISABLED: 'true',
  })
}

/**
 * Readies the priced bucket for the `s3` beat, before the app launches: writes the R2
 * secret into the clips instance's file-backed secret store (`CMDR_SECRET_STORE=file`,
 * `secrets.json`), so the scene connects WITHOUT a secret and nothing sensitive crosses
 * Playwright, MCP, or IPC; empties the demo prefix; and puts a README in the album so
 * the folder exists. `null` when the `secret` CLI can't supply the account.
 */
export async function seedPricedS3(dataDir: string): Promise<PricedS3Account | null> {
  const accountId = await r2AccountId()
  const accessKeyId = readSecret('R2_S3_TEST_ACCESS_KEY_ID')
  const secretKey = readSecret('R2_S3_TEST_SECRET_ACCESS_KEY')
  if (accountId === null || accessKeyId === null || secretKey === null) return null
  const account = { accountId, accessKeyId }

  const storePath = join(dataDir, 'secrets.json')
  const store = existsSync(storePath) ? (JSON.parse(readFileSync(storePath, 'utf8')) as Record<string, number[]>) : {}
  store[r2SecretKey(account)] = [...Buffer.from(`${accessKeyId}\u0000${secretKey}`, 'utf8')]
  writeFileSync(storePath, JSON.stringify(store), { mode: 0o600 })

  emptyPricedPrefix(account, secretKey)
  writeFileSync('/tmp/cmdr-clips-readme.txt', 'Family photo archive. Albums by year.\n')
  r2Cli(account, secretKey, [
    's3',
    'cp',
    '--quiet',
    '/tmp/cmdr-clips-readme.txt',
    `s3://${PRICED_S3.bucket}/${PRICED_S3.prefix}/${PRICED_S3.album}/README.txt`,
  ])
  return account
}

/** Deletes everything under the demo prefix, and returns how many objects a listing still finds there. */
function emptyPricedPrefix(account: PricedS3Account, secretKey: string): number {
  const prefix = `s3://${PRICED_S3.bucket}/${PRICED_S3.prefix}/`
  r2Cli(account, secretKey, ['s3', 'rm', '--quiet', '--recursive', prefix])
  const left = r2Cli(account, secretKey, [
    's3api',
    'list-objects-v2',
    '--bucket',
    PRICED_S3.bucket,
    '--prefix',
    `${PRICED_S3.prefix}/`,
    '--query',
    'KeyCount',
    '--output',
    'text',
  ])
  return Number(left.trim()) || 0
}

/**
 * After the run: deletes what the beat uploaded, confirms with a listing that the prefix
 * is empty, and takes the secret back out of the instance's store. Returns the
 * listing's count (0 when clean).
 */
export function cleanUpPricedS3(dataDir: string, account: PricedS3Account): number {
  const secretKey = readSecret('R2_S3_TEST_SECRET_ACCESS_KEY')
  const left = secretKey === null ? -1 : emptyPricedPrefix(account, secretKey)
  const storePath = join(dataDir, 'secrets.json')
  if (existsSync(storePath)) {
    const store = JSON.parse(readFileSync(storePath, 'utf8')) as Record<string, number[]>
    delete store[r2SecretKey(account)]
    writeFileSync(storePath, JSON.stringify(store), { mode: 0o600 })
  }
  return left
}

/** The restic repo's folder name in the demo tree, and its password. */
export const RESTIC = { folder: 'Backups', password: 'harbour-lights-42' } as const

/** The virtual phone's backing dir (`CMDR_VIRTUAL_MTP`) and its name (`CMDR_VIRTUAL_MTP_MODEL`). */
export const PHONE = { backingDir: '/tmp/cmdr-clips-phone', model: 'Pixel 8' } as const

function run(cmd: string, args: string[], env: Record<string, string> = {}): string {
  const res = spawnSync(cmd, args, { encoding: 'utf8', env: { ...process.env, ...env }, maxBuffer: 64 * 1024 * 1024 })
  if (res.error !== undefined || res.status !== 0) {
    throw new Error(`${cmd} ${args.slice(0, 3).join(' ')}… failed: ${res.stderr || String(res.error)}`)
  }
  return res.stdout
}

/** The running container behind the SMB fixture, or a message saying how to start it. */
function smbContainer(): string {
  const names = run('docker', ['ps', '--format', '{{.Names}}']).split('\n')
  const found = names.find((n) => n.includes('smb-consumer-auth'))
  if (found === undefined) {
    throw new Error('the SMB fixtures are not running: apps/desktop/test/smb-servers/start.sh minimal')
  }
  return found
}

/**
 * Refills the SMB share with a family's NAS: a `Photos` tree with one big year folder
 * (5,000 files, sparse, so it costs no disk) and one small real-sized trip folder for the
 * server-side copy, plus documents and videos.
 */
export function seedSmbShare(): void {
  const script = `
    set -e
    cd /shares/private
    rm -rf ./* ./.[!.]* 2>/dev/null || true
    mkdir -p "Photos/2024" "Photos/2025/Lisbon trip" "Backup" "Documents/Taxes" "Family videos"
    cd "Photos/2024"
    i=1; while [ $i -le 5000 ]; do n=$(printf '%05d' $i); truncate -s $(( 1800000 + (i * 7919) % 4200000 )) "IMG_$n.jpg"; i=$((i+1)); done
    cd "../2025/Lisbon trip"
    i=1; while [ $i -le 36 ]; do n=$(printf '%04d' $((i + 2210))); dd if=/dev/urandom of="IMG_$n.jpg" bs=1024 count=$(( 900 + (i * 131) % 1600 )) 2>/dev/null; i=$((i+1)); done
    cd /shares/private
    for m in 01 02 03 04 05 06; do truncate -s $(( 90000 + 1$m * 977 )) "Documents/Taxes/2025-$m statement.pdf"; done
    truncate -s 182000 "Documents/Insurance policy.pdf"
    truncate -s 64000 "Documents/Recipes.docx"
    truncate -s 1450000000 "Family videos/Summer 2025.mov"
    truncate -s 820000000 "Family videos/Grandma's 80th.mov"
    chmod -R 777 /shares/private
  `
  run('docker', ['exec', smbContainer(), 'sh', '-c', script])
}

const awsEnv = {
  AWS_ACCESS_KEY_ID: S3.accessKey,
  AWS_SECRET_ACCESS_KEY: S3.secretKey,
  AWS_DEFAULT_REGION: S3.region,
  AWS_EC2_METADATA_DISABLED: 'true',
}

/** Creates the `family-photos` bucket (if missing) and fills it with a few albums. */
export function seedS3Bucket(tripDir: string): void {
  const s3 = (args: string[]): string => run('aws', ['--endpoint-url', S3.endpoint, ...args], awsEnv)
  const buckets = s3(['s3api', 'list-buckets', '--query', 'Buckets[].Name', '--output', 'text'])
  if (!buckets.split(/\s+/).includes(S3.bucket)) s3(['s3api', 'create-bucket', '--bucket', S3.bucket])
  // An earlier take's uploads would otherwise be in the "before" shot.
  s3(['s3', 'rm', '--quiet', '--recursive', `s3://${S3.bucket}/`])
  const trip = tripDir
  const photos = readdirSync(trip)
    .filter((f) => f.endsWith('.jpg'))
    .slice(0, 12)
  for (const [i, photo] of photos.entries()) {
    const album = i < 6 ? '2024/Christmas' : '2025/Easter'
    s3(['s3', 'cp', '--quiet', join(trip, photo), `s3://${S3.bucket}/${album}/${photo}`])
  }
  writeFileSync('/tmp/cmdr-clips-readme.txt', 'Family photo archive. Albums by year.\n')
  s3(['s3', 'cp', '--quiet', '/tmp/cmdr-clips-readme.txt', `s3://${S3.bucket}/README.txt`])
}

/** A restic repo `Backups` in the demo tree, with two snapshots of the documents. */
export function seedResticRepo(demoRoot: string, sources: string[]): void {
  const repo = join(demoRoot, RESTIC.folder)
  const env = { RESTIC_PASSWORD: RESTIC.password, RESTIC_REPOSITORY: repo }
  if (!existsSync(join(repo, 'config'))) {
    rmSync(repo, { recursive: true, force: true })
    run('restic', ['init', '--quiet'], env)
    run('restic', ['backup', '--quiet', '--host', 'studio-mac', sources[0]], env)
    run('restic', ['backup', '--quiet', '--host', 'studio-mac', ...sources], env)
  }
}

/**
 * Puts realistic photos on the phone, after the app reseeded its backing dir at launch:
 * a `DCIM/Camera` of real-sized JPEGs from the demo tree, replacing the fixture's
 * placeholder files.
 */
export function seedPhone(tripDir: string): void {
  const dcim = join(PHONE.backingDir, 'internal', 'DCIM')
  rmSync(dcim, { recursive: true, force: true })
  const camera = join(dcim, 'Camera')
  mkdirSync(camera, { recursive: true })
  mkdirSync(join(dcim, 'Screenshots'), { recursive: true })
  const trip = tripDir
  const two = (n: number): string => String(n).padStart(2, '0')
  for (const [i, photo] of readdirSync(trip)
    .filter((f) => f.endsWith('.jpg'))
    .slice(0, 40)
    .entries()) {
    // Pixel camera names: PXL_<date>_<time with milliseconds>.
    const name = `PXL_202608${two(14 + Math.floor(i / 10))}_${two(9 + (i % 10))}${two((i * 7) % 60)}${two((i * 13) % 60)}${String((i * 97) % 1000).padStart(3, '0')}.jpg`
    copyFileSync(join(trip, photo), join(camera, name))
  }
  for (let i = 0; i < 6; i++)
    writeFileSync(
      join(dcim, 'Screenshots', `Screenshot_2026091${String(i)}-0930.png`),
      Buffer.alloc(240_000 + i * 9_000),
    )
}
