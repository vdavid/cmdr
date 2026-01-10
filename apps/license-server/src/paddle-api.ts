import type { LicenseType } from './license'

/**
 * Response from /validate endpoint
 */
export interface ValidationResponse {
    status: 'active' | 'expired' | 'invalid'
    type: LicenseType | null
    organizationName: string | null
    expiresAt: string | null
}

/**
 * Configuration for Paddle API
 */
interface PaddleConfig {
    apiKey: string
    environment: 'sandbox' | 'live'
}

/**
 * Get subscription status from Paddle API.
 * Returns null if transaction not found or API error.
 */
export async function getSubscriptionStatus(
    transactionId: string,
    config: PaddleConfig,
): Promise<{
    status: 'active' | 'expired' | 'canceled'
    expiresAt: string | null
    customData: { organizationName?: string } | null
} | null> {
    const baseUrl =
        config.environment === 'sandbox'
            ? 'https://sandbox-api.paddle.com'
            : 'https://api.paddle.com'

    try {
        // First get the transaction to find subscription ID
        const txnResponse = await fetch(`${baseUrl}/transactions/${transactionId}`, {
            headers: {
                Authorization: `Bearer ${config.apiKey}`,
            },
        })

        if (!txnResponse.ok) {
            console.error('Failed to fetch transaction:', txnResponse.status)
            return null
        }

        const txnData = (await txnResponse.json()) as {
            data: {
                subscription_id?: string
                custom_data?: { organization_name?: string }
            }
        }

        const subscriptionId = txnData.data.subscription_id
        const customData = txnData.data.custom_data

        // If no subscription, it's a one-time purchase (perpetual or supporter)
        if (!subscriptionId) {
            return {
                status: 'active', // One-time purchases are always active
                expiresAt: null,
                customData: customData ? { organizationName: customData.organization_name } : null,
            }
        }

        // Get subscription status
        const subResponse = await fetch(`${baseUrl}/subscriptions/${subscriptionId}`, {
            headers: {
                Authorization: `Bearer ${config.apiKey}`,
            },
        })

        if (!subResponse.ok) {
            console.error('Failed to fetch subscription:', subResponse.status)
            return null
        }

        const subData = (await subResponse.json()) as {
            data: {
                status: string
                current_billing_period?: {
                    ends_at: string
                }
            }
        }

        const paddleStatus = subData.data.status
        let status: 'active' | 'expired' | 'canceled'

        if (paddleStatus === 'active' || paddleStatus === 'trialing') {
            status = 'active'
        } else if (paddleStatus === 'past_due') {
            // Give grace period for failed payments
            status = 'active'
        } else {
            status = paddleStatus === 'canceled' ? 'canceled' : 'expired'
        }

        return {
            status,
            expiresAt: subData.data.current_billing_period?.ends_at ?? null,
            customData: customData ? { organizationName: customData.organization_name } : null,
        }
    } catch (error) {
        console.error('Paddle API error:', error)
        return null
    }
}

/**
 * Determine license type from Paddle price ID.
 * This mapping needs to be updated when products are created in Paddle.
 */
export function getLicenseTypeFromPriceId(priceId: string, priceIds: PriceIdMapping): LicenseType | null {
    if (priceIds.supporter && priceId === priceIds.supporter) {
        return 'supporter'
    }
    if (priceIds.commercialSubscription && priceId === priceIds.commercialSubscription) {
        return 'commercial_subscription'
    }
    if (priceIds.commercialPerpetual && priceId === priceIds.commercialPerpetual) {
        return 'commercial_perpetual'
    }
    // Legacy: treat unknown price IDs as commercial subscription for backwards compat
    return 'commercial_subscription'
}

export interface PriceIdMapping {
    supporter?: string
    commercialSubscription?: string
    commercialPerpetual?: string
}
