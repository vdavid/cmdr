/**
 * The SFTP and WebDAV fixture servers, as the Playwright server specs see them:
 * where the app dials each one, what a person types into the add sheet, and a
 * way to read and write the export WITHOUT going through the app.
 *
 * ❗ The side door is the point. A spec that copies a file to a server and then
 * reads it back through the same app proves only that the app agrees with
 * itself. So every server-side assertion here talks to the fixture over a
 * transport of its own: the `ssh` client for SFTP (the fixture user has a shell)
 * and `curl` for WebDAV.
 *
 * Where each lane finds the servers:
 * - **macOS** (`pnpm check desktop-e2e-playwright`): host ports on loopback,
 *   12480+ for SFTP and 13480+ for WebDAV, which the check runner pins through
 *   `SFTP_FIXTURE_*_PORT` / `WEBDAV_FIXTURE_*_PORT`.
 * - **Linux Docker** (`e2e-linux.sh`): the E2E container joins each stack's
 *   Docker network and dials the service by name on its container port, set
 *   through `SFTP_E2E_HOST` / `SFTP_E2E_PORT` and the WebDAV twins.
 *
 * The stacks themselves are leased by whoever runs the lane (`e2e` mode, one
 * server each); nothing here starts or stops a container, because other
 * worktrees lease the same ones.
 */

import { spawnSync } from 'child_process'
import crypto from 'crypto'
import fs from 'fs'
import os from 'os'
import path from 'path'

export type ServerProtocol = 'sftp' | 'webdav'

/** The account every fixture server runs as. Public on purpose: see the fixtures' READMEs. */
export const SERVER_USERNAME = 'test_user_1'
export const SERVER_PASSWORD = 'openthedoor'

/** One fixture server, from the spec's side. Paths are relative to the scratch-able export root. */
export interface ServerFixture {
  protocol: ServerProtocol
  /** How the spec names the suite in titles and logs. */
  label: string
  /** The host the APP dials, which is the host its volume id is minted from. */
  host: string
  port: number
  /** What a person types into the add sheet's address field to land in `dir`. */
  addressFor(dir: string): string
  /** Creates `rel` and every missing parent. */
  mkdir(rel: string): void
  /** Writes a file, creating its parents. */
  writeFile(rel: string, data: string | Buffer): void
  /** A file's bytes, or `null` when there is no file there. */
  readFile(rel: string): Buffer | null
  /** The names directly inside a directory, as the server stores them (NFC and NFD kept apart). */
  list(rel: string): string[]
  /** Removes a file or a whole tree. A missing one is fine. */
  remove(rel: string): void
}

/**
 * A directory name no other run can pick: two worktrees (or two lanes) share one
 * fixture server, and a Docker lane's pids start small, so the pid alone is not
 * unique. Never assume the export root is empty.
 */
export function uniqueScratchDir(protocol: ServerProtocol): string {
  return `cmdr-e2e-${protocol}-${crypto.randomBytes(6).toString('hex')}`
}

export function sftpFixture(): ServerFixture {
  const host = process.env.SFTP_E2E_HOST ?? '127.0.0.1'
  const port = Number(process.env.SFTP_E2E_PORT ?? process.env.SFTP_FIXTURE_OPENSSH_PORT ?? '12480')
  // The fixture exports this directory (`sftp-servers/image/entrypoint.sh`).
  const exportRoot = '/srv/data'
  const abs = (rel: string): string => path.posix.join(exportRoot, rel)

  function ssh(command: string, input?: Buffer | string): Buffer {
    const result = spawnSync(
      'ssh',
      [
        '-p',
        String(port),
        // Throwaway loopback servers whose host keys are minted per container:
        // there is nothing to verify and nowhere worth recording it.
        '-o',
        'StrictHostKeyChecking=no',
        '-o',
        'UserKnownHostsFile=/dev/null',
        '-o',
        'LogLevel=ERROR',
        // The developer's own keys and agent have no business here: the fixture
        // signs in on its public password.
        '-o',
        'PubkeyAuthentication=no',
        '-o',
        'PreferredAuthentications=password',
        '-o',
        'NumberOfPasswordPrompts=1',
        `${SERVER_USERNAME}@${host}`,
        command,
      ],
      {
        input: input ?? '',
        env: { ...process.env, SSH_ASKPASS: askpassScript(), SSH_ASKPASS_REQUIRE: 'force', DISPLAY: ':0' },
        maxBuffer: 64 * 1024 * 1024,
        timeout: 30_000,
      },
    )
    if (result.error) throw result.error
    if (result.status !== 0) {
      throw new ServerCommandError(
        `ssh ${host}:${String(port)} \`${command}\``,
        result.status,
        result.stderr.toString(),
      )
    }
    return result.stdout
  }

  return {
    protocol: 'sftp',
    label: 'SFTP',
    host,
    port,
    addressFor: (dir) => `sftp://${SERVER_USERNAME}@${host}:${String(port)}${abs(dir)}`,
    mkdir: (rel) => {
      ssh(`mkdir -p ${quote(abs(rel))}`)
    },
    writeFile: (rel, data) => {
      ssh(`mkdir -p ${quote(path.posix.dirname(abs(rel)))} && cat > ${quote(abs(rel))}`, data)
    },
    readFile: (rel) => {
      try {
        return ssh(`test -f ${quote(abs(rel))} || exit 3; cat ${quote(abs(rel))}`)
      } catch (e) {
        if (e instanceof ServerCommandError && e.status === 3) return null
        throw e
      }
    },
    list: (rel) =>
      ssh(`find ${quote(abs(rel))} -mindepth 1 -maxdepth 1 -print0`)
        .toString('utf-8')
        .split('\0')
        .filter((entry) => entry !== '')
        .map((entry) => path.posix.basename(entry))
        .sort(),
    remove: (rel) => {
      ssh(`rm -rf ${quote(abs(rel))}`)
    },
  }
}

export function webdavFixture(): ServerFixture {
  const host = process.env.WEBDAV_E2E_HOST ?? '127.0.0.1'
  const port = Number(process.env.WEBDAV_E2E_PORT ?? process.env.WEBDAV_FIXTURE_APACHE_PORT ?? '13480')
  // The collection the fixture's Apache serves (`webdav-servers/image/`).
  const base = `http://${host}:${String(port)}/dav/`
  const auth = `Basic ${Buffer.from(`${SERVER_USERNAME}:${SERVER_PASSWORD}`).toString('base64')}`
  const url = (rel: string): string =>
    base +
    rel
      .split('/')
      .filter((segment) => segment !== '')
      .map(encodeURIComponent)
      .join('/')

  /**
   * One request, synchronously. The helpers are called from hooks and from inside
   * `expect.poll`, and a sync shape keeps them interchangeable with the SFTP
   * ones; `curl` is on both lanes (the E2E image ships it, macOS always has it).
   */
  function dav(
    method: string,
    rel: string,
    options: { body?: Buffer | string; depth?: string; collection?: boolean } = {},
  ): DavAnswer {
    const args = ['-s', '-o', '-', '-w', '\n%{http_code}', '-X', method, '-H', `Authorization: ${auth}`]
    if (options.depth !== undefined) args.push('-H', `Depth: ${options.depth}`)
    if (options.body !== undefined) args.push('--data-binary', '@-')
    // ❗ A collection's URL ends in a slash: Apache answers the bare one with a
    // 301 to the slashed form, which no verb here should have to follow.
    args.push(options.collection === true ? `${url(rel)}/` : url(rel))
    const result = spawnSync('curl', args, { input: options.body ?? '', maxBuffer: 64 * 1024 * 1024, timeout: 30_000 })
    if (result.error) throw result.error
    const out = result.stdout
    const split = out.lastIndexOf(0x0a)
    return { status: Number(out.subarray(split + 1).toString()), body: out.subarray(0, split) }
  }

  function expectStatus(answer: DavAnswer, what: string, ok: number[]): void {
    if (!ok.includes(answer.status)) {
      throw new ServerCommandError(`${what} on ${base}`, answer.status, answer.body.toString())
    }
  }

  function mkdirs(rel: string): void {
    const segments = rel.split('/').filter((segment) => segment !== '')
    for (let i = 1; i <= segments.length; i++) {
      // 405 is "that collection already exists", which is what `-p` means.
      expectStatus(dav('MKCOL', segments.slice(0, i).join('/'), { collection: true }), 'MKCOL', [201, 405])
    }
  }

  return {
    protocol: 'webdav',
    label: 'WebDAV',
    host,
    port,
    addressFor: (dir) => url(dir) + '/',
    mkdir: mkdirs,
    writeFile: (rel, data) => {
      mkdirs(path.posix.dirname(rel))
      expectStatus(dav('PUT', rel, { body: data }), 'PUT', [201, 204])
    },
    readFile: (rel) => {
      const answer = dav('GET', rel)
      if (answer.status === 404) return null
      expectStatus(answer, 'GET', [200])
      return answer.body
    },
    list: (rel) => {
      const answer = dav('PROPFIND', rel, { depth: '1', collection: true })
      expectStatus(answer, 'PROPFIND', [207])
      const self = new URL(url(rel)).pathname.replace(/\/$/, '')
      const names: string[] = []
      for (const match of answer.body.toString('utf-8').matchAll(/<D:href>([^<]*)<\/D:href>/gi)) {
        const href = match[1].replace(/\/$/, '')
        if (href === self) continue
        names.push(decodeURIComponent(href.slice(href.lastIndexOf('/') + 1)))
      }
      return names.sort()
    },
    remove: (rel) => {
      // Not knowing whether `rel` is a file or a tree, ask for the file first: the
      // 301 is Apache saying it's a collection.
      let answer = dav('DELETE', rel)
      if (answer.status === 301) answer = dav('DELETE', rel, { collection: true })
      expectStatus(answer, 'DELETE', [204, 404])
    },
  }
}

export function serverFixture(protocol: ServerProtocol): ServerFixture {
  return protocol === 'sftp' ? sftpFixture() : webdavFixture()
}

interface DavAnswer {
  status: number
  body: Buffer
}

/** A side-door command the server refused, carrying what it said. */
export class ServerCommandError extends Error {
  constructor(
    what: string,
    readonly status: number | null,
    stderr: string,
  ) {
    super(`${what} answered ${String(status)}: ${stderr.trim()}`)
    this.name = 'ServerCommandError'
  }
}

/** POSIX single-quoting, for a path handed to the fixture's shell. */
function quote(value: string): string {
  return `'${value.replace(/'/g, `'\\''`)}'`
}

let askpassPath: string | undefined

/**
 * A script that answers ssh's password prompt, so the side door needs no
 * `sshpass` (the E2E image doesn't carry it) and no TTY. `SSH_ASKPASS_REQUIRE=force`
 * is what makes ssh ask it even with a terminal attached (OpenSSH 8.4+).
 */
function askpassScript(): string {
  if (askpassPath && fs.existsSync(askpassPath)) return askpassPath
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'cmdr-e2e-askpass-'))
  askpassPath = path.join(dir, 'askpass.sh')
  fs.writeFileSync(askpassPath, `#!/bin/sh\necho '${SERVER_PASSWORD}'\n`, { mode: 0o700 })
  return askpassPath
}
