const workerBaseUrl = 'https://api.getcmdr.com'

/** GETs a worker admin endpoint with the shared admin bearer token. Throws on a non-2xx response. */
export async function fetchWorkerEndpoint<T>(adminToken: string, path: string): Promise<T> {
  const response = await fetch(`${workerBaseUrl}${path}`, {
    headers: { Authorization: `Bearer ${adminToken}` },
  })
  if (!response.ok) {
    const text = await response.text()
    throw new Error(`Worker ${path} returned ${String(response.status)}: ${text}`)
  }
  return (await response.json()) as T
}
