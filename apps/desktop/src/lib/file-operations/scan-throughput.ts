/**
 * Scan-phase throughput estimator.
 *
 * The backend EtaEstimator covers write phases but not the scan-preview pipeline,
 * so the FE computes a simple rolling rate from event-to-event tally deltas.
 * Keep the algorithm tiny — we just want a calm number for the user to read,
 * not a forecast.
 *
 * Returns null files/s and bytes/s until two samples have arrived (no point
 * estimating a rate from one data point) and discards stale samples beyond
 * `windowMs` so a long pause doesn't poison the average.
 */

const DEFAULT_WINDOW_MS = 2000

export interface ThroughputSample {
  timestampMs: number
  files: number
  bytes: number
}

export interface ThroughputReadout {
  filesPerSecond: number | null
  bytesPerSecond: number | null
}

export class ScanThroughput {
  private samples: ThroughputSample[] = []
  private readonly windowMs: number

  constructor(windowMs: number = DEFAULT_WINDOW_MS) {
    this.windowMs = windowMs
  }

  /** Reset between scans. */
  reset(): void {
    this.samples = []
  }

  /** Add a new tally sample. Returns the current rate readout. */
  push(sample: ThroughputSample): ThroughputReadout {
    this.samples.push(sample)
    this.dropStale(sample.timestampMs)
    return this.readout()
  }

  /** Latest readout without pushing a new sample. */
  readout(): ThroughputReadout {
    if (this.samples.length < 2) {
      return { filesPerSecond: null, bytesPerSecond: null }
    }
    const first = this.samples[0]
    const last = this.samples[this.samples.length - 1]
    const elapsedMs = last.timestampMs - first.timestampMs
    if (elapsedMs <= 0) {
      return { filesPerSecond: null, bytesPerSecond: null }
    }
    const elapsedSec = elapsedMs / 1000
    const fps = (last.files - first.files) / elapsedSec
    const bps = (last.bytes - first.bytes) / elapsedSec
    return {
      filesPerSecond: fps > 0 ? fps : 0,
      bytesPerSecond: bps > 0 ? bps : 0,
    }
  }

  private dropStale(nowMs: number): void {
    const cutoff = nowMs - this.windowMs
    // Always keep the most recent sample so a long pause still has a baseline.
    while (this.samples.length > 2 && this.samples[0].timestampMs < cutoff) {
      this.samples.shift()
    }
  }
}
