import { Hono } from 'hono'
import { etag } from 'hono/etag'
import type { Bindings } from '../types'
import prices from './s3-prices.json'

// The S3 list-price table the desktop app prices a planned operation with. The app bundles the same
// file as its fallback and fetches this one, so a price change ships with a Worker deploy, not an
// app release. `s3-prices.test.ts` holds this copy byte-identical to the crate's.

const s3Prices = new Hono<{ Bindings: Bindings }>()

/** Prices move a few times a year; an hour of edge and client caching costs nothing. */
const cacheControl = 'public, max-age=3600'

/** Serialized once per isolate: the table is static for the life of a deploy. */
const body = JSON.stringify(prices)

s3Prices.get('/s3-prices/v1', etag(), (c) => {
  c.header('Cache-Control', cacheControl)
  c.header('Content-Type', 'application/json; charset=UTF-8')
  return c.body(body)
})

export { s3Prices }
