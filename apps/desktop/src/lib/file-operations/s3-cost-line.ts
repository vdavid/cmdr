/**
 * The pure half of `S3CostLine.svelte`: when a dialog asks for an S3 cost
 * estimate, and which estimates it shows.
 *
 * The backend prices the operation from the dialog's settled scan preview and
 * answers `[]` when nothing S3 is priced, so a dialog asks unconditionally once
 * its scan settles. Rationale and the surfaces: `DETAILS.md` § "S3 cost line".
 */

import type { ClashPlan, CostEstimate, CostEstimateRequest } from '$lib/tauri-commands'
import { formatMoney, getNumberFormatter } from '$lib/intl/number-format'
import type { TransferOperationType } from './write-operation-types'

/** One shown line: the provider and its formatted, rounded amount. */
export interface CostLine {
  providerLabel: string
  amountText: string
}

/**
 * Whether `amount` shows as zero once rounded to the currency's minor unit
 * (under half a cent for USD), read off the same formatter `formatMoney` uses.
 */
export function roundsToZero(amount: number, currency: string): boolean {
  const digits = getNumberFormatter({ style: 'currency', currency }).resolvedOptions().maximumFractionDigits ?? 0
  return Math.abs(amount) < 0.5 * 10 ** -digits
}

/** The estimates worth a line, formatted: one that rounds to zero says nothing useful. */
export function visibleCostLines(estimates: CostEstimate[]): CostLine[] {
  return estimates
    .filter((estimate) => !roundsToZero(estimate.amount, estimate.currency))
    .map((estimate) => ({
      providerLabel: estimate.providerLabel,
      amountText: formatMoney(estimate.amount, estimate.currency),
    }))
}

/**
 * The request a dialog hands `S3CostLine`, or `null` for "don't ask (yet)":
 * the scan is still running, the preview id isn't known, or the operation is one
 * the backend doesn't price (compress, trash). `clashes` (the Move and Copy
 * dialogs' conflict check plus the chosen policy) prices the overwrites; the
 * backend decides which clashes the policy overwrites.
 */
export function costRequestFor(input: {
  operation: TransferOperationType
  scanComplete: boolean
  previewId: string | null
  sourceVolumeId: string
  destinationVolumeId: string | null
  clashes?: ClashPlan | null
}): CostEstimateRequest | null {
  const { operation, scanComplete, previewId, sourceVolumeId, destinationVolumeId, clashes } = input
  if (operation !== 'copy' && operation !== 'move' && operation !== 'delete') return null
  if (!scanComplete || previewId === null) return null
  const request: CostEstimateRequest = { operation, previewId, sourceVolumeId, destinationVolumeId }
  return clashes ? { ...request, clashes } : request
}
