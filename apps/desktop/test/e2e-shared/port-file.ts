/**
 * Read the MCP / tauri-MCP port files written by the Cmdr backend and the wrapper.
 *
 * Shape mirrors the Rust side in `apps/desktop/src-tauri/src/mcp/port_file.rs`:
 *   - File contents are ASCII decimal port + `\n`, written via tempfile + fsync + rename
 *     (POSIX-atomic on same-filesystem). A zero-byte read is impossible: the file either
 *     hasn't been renamed yet (NOENT) or contains the full content.
 *   - Callers that need to wait for the file to appear poll externally; this helper exposes
 *     both a one-shot read and a polling read.
 *
 * External readers of the port file are CLI tools and tests; the in-process FE keeps using
 * the `get_mcp_port` IPC (it reads the same `MCP_ACTUAL_PORT` atomic without disk I/O).
 *
 * Read precedence used by `resolveMcpPort()`:
 *   1. `CMDR_MCP_PORT` env (manual pin) → return immediately.
 *   2. `<data_dir>/mcp.port` (ephemeral discovery) → poll up to 5 s.
 *   3. Throw `PortDiscoveryError`. Never silently fall back to the legacy 19224 / 19225;
 *      that would hide bugs (P2/P3 design).
 */

import { readFileSync } from 'fs'
import { join } from 'path'

export class PortDiscoveryError extends Error {
  constructor(
    message: string,
    public readonly kind: 'not_found' | 'invalid_content' | 'io',
  ) {
    super(message)
    this.name = 'PortDiscoveryError'
  }
}

/** Canonical port-file path: `<dir>/<name>`. */
export function portFilePath(dir: string, name: string): string {
  return join(dir, name)
}

/** Synchronous one-shot read. Throws `PortDiscoveryError` on missing / unparseable file. */
export function readPortFile(dir: string, name: string): number {
  let raw: string
  try {
    raw = readFileSync(portFilePath(dir, name), 'utf8')
  } catch (err) {
    if (err instanceof Error && (err as NodeJS.ErrnoException).code === 'ENOENT') {
      throw new PortDiscoveryError(`port file not found at ${portFilePath(dir, name)}`, 'not_found')
    }
    throw new PortDiscoveryError(
      `port file IO error at ${portFilePath(dir, name)}: ${err instanceof Error ? err.message : String(err)}`,
      'io',
    )
  }
  const trimmed = raw.trim()
  const port = Number.parseInt(trimmed, 10)
  if (!Number.isInteger(port) || port < 0 || port > 65535 || String(port) !== trimmed) {
    throw new PortDiscoveryError(`port file content not a valid u16: ${JSON.stringify(trimmed)}`, 'invalid_content')
  }
  return port
}

/**
 * Poll for the port file to appear and return its parsed value. 50 ms cadence by default
 * (matches the Rust-side recommendation in port_file.rs), 5 s deadline by default. The
 * cadence and deadline are exposed so tests can tighten them.
 *
 * Throws `PortDiscoveryError('not_found')` on deadline.
 */
export async function pollPortFile(
  dir: string,
  name: string,
  opts?: { intervalMs?: number; deadlineMs?: number },
): Promise<number> {
  const intervalMs = opts?.intervalMs ?? 50
  const deadlineMs = opts?.deadlineMs ?? 5000
  const start = Date.now()
  for (;;) {
    try {
      return readPortFile(dir, name)
    } catch (err) {
      if (!(err instanceof PortDiscoveryError) || err.kind !== 'not_found') {
        throw err
      }
      if (Date.now() - start >= deadlineMs) {
        throw new PortDiscoveryError(
          `port file ${portFilePath(dir, name)} did not appear within ${String(deadlineMs)} ms`,
          'not_found',
        )
      }
      await new Promise((resolve) => setTimeout(resolve, intervalMs))
    }
  }
}

/**
 * Resolve the MCP server port for an out-of-process reader (CLI, fixture, agent helper).
 * Precedence: `CMDR_MCP_PORT` env → `<dataDir>/mcp.port` file → throw.
 *
 * Pass the data dir explicitly so callers can compose it from `CMDR_DATA_DIR` or from
 * `CMDR_INSTANCE_ID` via instance-id.js's `computeAppDataDir` rules (see the wrapper).
 */
export async function resolveMcpPort(
  dataDir: string,
  opts?: { intervalMs?: number; deadlineMs?: number },
): Promise<number> {
  const envPort = process.env.CMDR_MCP_PORT
  if (envPort !== undefined && envPort.length > 0) {
    const parsed = Number.parseInt(envPort, 10)
    if (!Number.isInteger(parsed) || parsed < 0 || parsed > 65535) {
      throw new PortDiscoveryError(`CMDR_MCP_PORT is not a valid u16: ${JSON.stringify(envPort)}`, 'invalid_content')
    }
    return parsed
  }
  return pollPortFile(dataDir, 'mcp.port', opts)
}
