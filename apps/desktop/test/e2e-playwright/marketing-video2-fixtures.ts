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
import { copyFileSync, existsSync, mkdirSync, readdirSync, rmSync, writeFileSync } from 'node:fs'
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
