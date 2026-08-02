/**
 * Shared types for composing friendly-error copy on the frontend.
 *
 * Error CLASSIFICATION lives in Rust (errno→reason mapping, category/retry
 * assignment, provider detection); the WORDS live here. The backend ships a
 * typed `reason` + structured params over IPC, and the FE factories turn that
 * into the user-facing title/explanation/suggestion.
 *
 * The `FriendlyErrorMessage` shape matches `transfer-error-messages.ts` so the
 * listing path and the write path can converge on one catalog later.
 */

export interface FriendlyErrorMessage {
  /** Short title for the error. Plain text (not markdown). */
  title: string
  /** Main explanation of what happened. Trusted markdown. */
  message: string
  /** Suggestion for what the user can do. Trusted markdown. */
  suggestion: string
}
