/** Production worker. Override per-request with `WORKER_BASE_URL` (e.g. a local `wrangler dev`) for QA. */
const defaultWorkerBaseUrl = 'https://api.getcmdr.com'

/**
 * GETs a worker admin endpoint with the shared admin bearer token. Throws on a non-2xx response.
 * `baseUrl` defaults to production; pass the resolved `WORKER_BASE_URL` to point at a local worker.
 */
export async function fetchWorkerEndpoint<T>(adminToken: string, path: string, baseUrl?: string): Promise<T> {
  const response = await fetch(`${baseUrl || defaultWorkerBaseUrl}${path}`, {
    headers: { Authorization: `Bearer ${adminToken}` },
  })
  if (!response.ok) {
    const text = await response.text()
    throw new Error(`Worker ${path} returned ${String(response.status)}: ${text}`)
  }
  return (await response.json()) as T
}
