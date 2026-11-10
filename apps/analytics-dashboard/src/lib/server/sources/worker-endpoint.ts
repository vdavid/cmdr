/** Production worker. Override per-request with `WORKER_BASE_URL` (e.g. a local `wrangler dev`) for QA. */
const defaultWorkerBaseUrl = 'https://api.getcmdr.com'

/**
 * GETs a worker admin endpoint with the shared admin bearer token. Throws on a non-2xx response.
 * `baseUrl` defaults to production; pass the resolved `WORKER_BASE_URL` to point at a local worker.
 */
export async function fetchWorkerEndpoint<T>(adminToken: string, path: string, baseUrl?: string): Promise<T> {
  return (await fetchWorkerResponse<T>(adminToken, path, baseUrl)).body
}

/** `fetchWorkerEndpoint` plus the response headers, for an endpoint that reports something in them. */
// eslint-disable-next-line @typescript-eslint/no-unnecessary-type-parameters -- `T` names the JSON body's shape for the caller, as `fetchWorkerEndpoint`'s does
export async function fetchWorkerResponse<T>(
  adminToken: string,
  path: string,
  baseUrl?: string,
): Promise<{ body: T; headers: Headers }> {
  const response = await fetch(`${baseUrl || defaultWorkerBaseUrl}${path}`, {
    headers: { Authorization: `Bearer ${adminToken}` },
  })
  if (!response.ok) {
    const text = await response.text()
    throw new Error(`Worker ${path} returned ${String(response.status)}: ${text}`)
  }
  return { body: (await response.json()) as T, headers: response.headers }
}
