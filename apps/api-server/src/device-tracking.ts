export interface DeviceSet {
    devices: Record<string, string> // deviceHash → ISO timestamp (last seen)
    lastAlertedAt?: string // ISO timestamp, for alert suppression
}

/** Remove device entries older than `maxAgeDays` from the current time. */
export function pruneStaleDevices(devices: Record<string, string>, maxAgeDays: number): Record<string, string> {
    const cutoff = Date.now() - maxAgeDays * 24 * 60 * 60 * 1000
    const result: Record<string, string> = {}
    for (const [hash, timestamp] of Object.entries(devices)) {
        if (new Date(timestamp).getTime() >= cutoff) {
            result[hash] = timestamp
        }
    }
    return result
}

/** Whether an alert should fire based on device count, threshold, and last alert time. */
export function shouldAlert(deviceCount: number, lastAlertedAt: string | undefined, threshold: number): boolean {
    if (deviceCount < threshold) return false
    if (!lastAlertedAt) return true
    const daysSinceLastAlert = (Date.now() - new Date(lastAlertedAt).getTime()) / (24 * 60 * 60 * 1000)
    return daysSinceLastAlert > 30
}
