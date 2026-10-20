/**
 * A REAL SQLite behind the slice of the D1 API the cron jobs use (`prepare` / `bind` / `all` /
 * `first` / `run`), seeded with every migration in order.
 *
 * The statement-shape mock in `cron-test-helpers.ts` proves which SQL ran; this proves what the SQL
 * DOES, which is the whole risk of an outbox: a row that a WHERE clause quietly skips is a report
 * nobody hears about. `node:sqlite` ships with Node, so this costs no dependency. Test-only: nothing
 * in the Worker imports it.
 */
/* eslint-disable-next-line no-restricted-imports -- Node-side test helper; reads the real migration files. */
import { readdirSync, readFileSync } from 'node:fs'
/* eslint-disable-next-line no-restricted-imports -- Node-side test helper; never reaches the Worker bundle. */
import { DatabaseSync, type SQLInputValue } from 'node:sqlite'

const migrationsDir = new URL('../migrations/', import.meta.url)

/**
 * The migrations in filename order, the way `wrangler d1 migrations apply` runs them. `before` stops
 * short of the named one, so a test can seed the state a migration meets in production; `from`
 * starts at it, so the test can then bring the schema the rest of the way.
 */
function migrations(range: { before?: string; from?: string } = {}): string {
  const { before, from } = range
  return readdirSync(migrationsDir)
    .filter((file) => file.endsWith('.sql'))
    .sort()
    .filter((file) => before === undefined || file < before)
    .filter((file) => from === undefined || file >= from)
    .map((file) => readFileSync(new URL(file, migrationsDir), 'utf8'))
    .join('\n')
}

/**
 * Run every migration from `from` (a filename prefix, like `0022`) onward against an engine built
 * with `before`, leaving it at the production schema.
 */
export function applyMigrationsFrom(sqlite: DatabaseSync, from: string): void {
  sqlite.exec(migrations({ from }))
}

export interface SqliteD1 {
  /** The D1 face the code under test receives. */
  d1: D1Database
  /** The raw engine, for seeding rows and reading results back. */
  sqlite: DatabaseSync
}

/** A fresh in-memory database with the production schema, or the schema as it was `before` a migration. */
export function createSqliteD1(options: { before?: string } = {}): SqliteD1 {
  const sqlite = new DatabaseSync(':memory:')
  sqlite.exec(migrations({ before: options.before }))

  const statement = (sql: string, args: SQLInputValue[] = []) => ({
    bind: (...bound: unknown[]) => statement(sql, bound as SQLInputValue[]),
    all: () => Promise.resolve({ results: sqlite.prepare(sql).all(...args), success: true }),
    first: () => Promise.resolve(sqlite.prepare(sql).get(...args) ?? null),
    run: () => {
      const result = sqlite.prepare(sql).run(...args)
      return Promise.resolve({
        success: true,
        meta: { changes: Number(result.changes), last_row_id: Number(result.lastInsertRowid), size_after: 0 },
      })
    },
  })

  const d1 = { prepare: (sql: string) => statement(sql) } as unknown as D1Database
  return { d1, sqlite }
}
