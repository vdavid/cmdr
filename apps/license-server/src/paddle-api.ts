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

/** Result from subscription status check */
interface SubscriptionResult {
    status: 'active' | 'expired' | 'canceled'
    expiresAt: string | null
    customData: { organizationName?: string } | null
}

/**
 * Get subscription status from Paddle API.
 * Returns null if transaction not found or API error.
 */
export async function getSubscriptionStatus(
    transactionId: string,
    config: PaddleConfig,
): Promise<SubscriptionResult | null> {
    const baseUrl = config.environment === 'sandbox' ? 'https://sandbox-api.paddle.com' : 'https://api.paddle.com'

    try {
        const txnResult = await fetchTransaction(baseUrl, transactionId, config.apiKey)
        if (!txnResult) return null

        // If no subscription, it's a one-time purchase (always active)
        if (!txnResult.subscriptionId) {
            return {
                status: 'active',
                expiresAt: null,
                customData: txnResult.customData?.organizationName
                    ? { organizationName: txnResult.customData.organizationName }
                    : null,
            }
        }

        const subResult = await fetchSubscription(baseUrl, txnResult.subscriptionId, config.apiKey)
        if (!subResult) return null

        return {
            status: subResult.status,
            expiresAt: subResult.expiresAt ?? null,
            customData: txnResult.customData?.organizationName
                ? { organizationName: txnResult.customData.organizationName }
                : null,
        }
    } catch (error) {
        console.error('Paddle API error:', error)
        return null
    }
}

/** Fetch transaction from Paddle API */
async function fetchTransaction(
    baseUrl: string,
    transactionId: string,
    apiKey: string,
): Promise<{ subscriptionId: string | undefined; customData: { organizationName?: string } | undefined } | null> {
    const response = await fetch(`${baseUrl}/transactions/${transactionId}`, {
        headers: { Authorization: `Bearer ${apiKey}` },
    })

    if (!response.ok) {
        console.error('Failed to fetch transaction:', response.status)
        return null
    }

    const json: unknown = await response.json()
    return extractTransactionData(json)
}

/** Fetch subscription from Paddle API */
async function fetchSubscription(
    baseUrl: string,
    subscriptionId: string,
    apiKey: string,
): Promise<{ status: 'active' | 'expired' | 'canceled'; expiresAt: string | undefined } | null> {
    const response = await fetch(`${baseUrl}/subscriptions/${subscriptionId}`, {
        headers: { Authorization: `Bearer ${apiKey}` },
    })

    if (!response.ok) {
        console.error('Failed to fetch subscription:', response.status)
        return null
    }

    const json: unknown = await response.json()
    return extractSubscriptionData(json)
}

/** Extract transaction data from Paddle API response */
function extractTransactionData(json: unknown): {
    subscriptionId: string | undefined
    customData: { organizationName?: string } | undefined
} | null {
    if (!json || typeof json !== 'object') return null
    const obj = json as Record<string, unknown>

    if (!obj.data || typeof obj.data !== 'object') return null
    const data = obj.data as Record<string, unknown>

    const subscriptionId = typeof data.subscription_id === 'string' ? data.subscription_id : undefined

    let customData: { organizationName?: string } | undefined
    if (data.custom_data && typeof data.custom_data === 'object') {
        const cd = data.custom_data as Record<string, unknown>
        customData = {
            organizationName: typeof cd.organization_name === 'string' ? cd.organization_name : undefined,
        }
    }

    return { subscriptionId, customData }
}

/** Extract subscription data from Paddle API response */
function extractSubscriptionData(json: unknown): {
    status: 'active' | 'expired' | 'canceled'
    expiresAt: string | undefined
} | null {
    if (!json || typeof json !== 'object') return null
    const obj = json as Record<string, unknown>

    if (!obj.data || typeof obj.data !== 'object') return null
    const data = obj.data as Record<string, unknown>

    const paddleStatus = typeof data.status === 'string' ? data.status : 'unknown'
    const status = mapPaddleStatus(paddleStatus)

    let expiresAt: string | undefined
    if (data.current_billing_period && typeof data.current_billing_period === 'object') {
        const period = data.current_billing_period as Record<string, unknown>
        expiresAt = typeof period.ends_at === 'string' ? period.ends_at : undefined
    }

    return { status, expiresAt }
}

/** Map Paddle subscription status to our status */
function mapPaddleStatus(paddleStatus: string): 'active' | 'expired' | 'canceled' {
    if (paddleStatus === 'active' || paddleStatus === 'trialing' || paddleStatus === 'past_due') {
        return 'active'
    }
    if (paddleStatus === 'canceled') {
        return 'canceled'
    }
    return 'expired'
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

/** Customer details from Paddle API */
export interface CustomerDetails {
    email: string
    name: string | null
    businessName: string | null
}

/**
 * Fetch customer details from Paddle API using customer ID.
 * Returns null if customer not found or API error.
 */
export async function getCustomerDetails(customerId: string, config: PaddleConfig): Promise<CustomerDetails | null> {
    const baseUrl = config.environment === 'sandbox' ? 'https://sandbox-api.paddle.com' : 'https://api.paddle.com'

    try {
        const response = await fetch(`${baseUrl}/customers/${customerId}`, {
            headers: { Authorization: `Bearer ${config.apiKey}` },
        })

        if (!response.ok) {
            console.error('Failed to fetch customer:', response.status)
            return null
        }

        const json: unknown = await response.json()
        return extractCustomerData(json)
    } catch (error) {
        console.error('Paddle API error fetching customer:', error)
        return null
    }
}

/** Extract customer data from Paddle API response */
function extractCustomerData(json: unknown): CustomerDetails | null {
    if (!json || typeof json !== 'object') return null
    const obj = json as Record<string, unknown>

    if (!obj.data || typeof obj.data !== 'object') return null
    const data = obj.data as Record<string, unknown>

    const email = typeof data.email === 'string' ? data.email : null
    if (!email) return null

    const name = typeof data.name === 'string' ? data.name : null

    // Extract business name from the business object (when customer adds business details)
    let businessName: string | null = null
    if (data.business && typeof data.business === 'object') {
        const business = data.business as Record<string, unknown>
        businessName = typeof business.name === 'string' ? business.name : null
    }

    return { email, name, businessName }
}
