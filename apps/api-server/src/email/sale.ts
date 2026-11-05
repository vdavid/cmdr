/**
 * The sale notice: one email to David for every completed purchase, so a sale is a moment to
 * celebrate rather than a row he finds in an analytics report weeks later.
 *
 * Plain and friendly, in the ops-alert chrome. Only David receives it, but it's copy he reads, so
 * the wording is pinned by `sale.test.ts` rather than reviewed by eye.
 */

import { sendEmail } from './send'
import { CELL_STYLE, TABLE_STYLE, bodyStyle, documentShell, escapeHtml, signoffParagraph } from './layout'

/** Everything the notice says about one sale. Amounts are in the currency's smallest unit, as Paddle sends them. */
export interface SaleNotice {
  /** The running count: this is sale number N. */
  saleNumber: number
  transactionId: string
  paddleEnvironment: 'sandbox' | 'live'
  /** ISO 8601, when Paddle billed it. */
  billedAt: string | null
  currencyCode: string
  grandTotalMinor: number
  earningsMinor: number | null
  productName: string | null
  priceName: string | null
  quantity: number
  /** ISO 3166-1 alpha-2. */
  countryCode: string | null
  buyerName: string | null
  buyerEmail: string | null
  businessName: string | null
}

/** A transaction's page in the Paddle dashboard, per environment. */
export function paddleTransactionUrl(transactionId: string, environment: 'sandbox' | 'live'): string {
  const domain = environment === 'sandbox' ? 'sandbox-vendors.paddle.com' : 'vendors.paddle.com'
  return `https://${domain}/transactions-v2/${encodeURIComponent(transactionId)}`
}

/**
 * An amount in a currency's smallest unit, formatted for a person: 7021 USD is "$70.21", 5900 JPY
 * is "¥5,900". The currency's own decimal count decides the divisor, since Paddle's smallest unit
 * for yen is one yen. `dropWholeCents` turns "$59.00" into "$59", for the subject line.
 */
export function formatMoney(amountMinor: number, currencyCode: string, options?: { dropWholeCents?: boolean }): string {
  let format: Intl.NumberFormat
  try {
    format = new Intl.NumberFormat('en-US', { style: 'currency', currency: currencyCode })
  } catch {
    return `${(amountMinor / 100).toFixed(2)} ${currencyCode}`
  }
  const digits = format.resolvedOptions().maximumFractionDigits ?? 2
  const amount = amountMinor / 10 ** digits
  if (options?.dropWholeCents && Number.isInteger(amount)) {
    return new Intl.NumberFormat('en-US', {
      style: 'currency',
      currency: currencyCode,
      minimumFractionDigits: 0,
      maximumFractionDigits: 0,
    }).format(amount)
  }
  return format.format(amount)
}

/** "Germany" for `DE`, or the code itself if the runtime can't name it. */
function countryName(countryCode: string): string {
  try {
    return new Intl.DisplayNames(['en'], { type: 'region' }).of(countryCode) ?? countryCode
  } catch {
    return countryCode
  }
}

function productLine(notice: SaleNotice): string {
  const names = [notice.productName, notice.priceName].filter((name): name is string => name !== null)
  return names.length > 0 ? names.join(', ') : 'Cmdr'
}

/** The facts, in order, as label and plain value. The HTML and the text part render the same list. */
function factRows(notice: SaleNotice): Array<{ label: string; value: string; href?: string }> {
  const rows: Array<{ label: string; value: string; href?: string }> = [
    { label: 'Paid', value: `${formatMoney(notice.grandTotalMinor, notice.currencyCode)} ${notice.currencyCode}` },
  ]
  if (notice.earningsMinor !== null) {
    rows.push({ label: 'After fees and tax', value: formatMoney(notice.earningsMinor, notice.currencyCode) })
  }
  rows.push({ label: 'Product', value: productLine(notice) })
  if (notice.quantity > 1) rows.push({ label: 'Seats', value: `${String(notice.quantity)} seats` })
  if (notice.countryCode) rows.push({ label: 'Country', value: countryName(notice.countryCode) })
  rows.push({ label: 'Buyer', value: notice.buyerName ?? 'No name given' })
  if (notice.buyerEmail) rows.push({ label: 'Email', value: notice.buyerEmail, href: `mailto:${notice.buyerEmail}` })
  if (notice.businessName) rows.push({ label: 'Company', value: notice.businessName })
  if (notice.billedAt) rows.push({ label: 'Date', value: notice.billedAt.slice(0, 10) })
  rows.push({
    label: 'Transaction',
    value: notice.transactionId,
    href: paddleTransactionUrl(notice.transactionId, notice.paddleEnvironment),
  })
  return rows
}

const SIGNOFF = 'The Cmdr API server sends one of these for every completed sale.'

/** Subject, HTML, and plain text for one sale. Pure, so the copy is testable. */
export function renderSaleNotice(notice: SaleNotice): { subject: string; html: string; text: string } {
  const amount = formatMoney(notice.grandTotalMinor, notice.currencyCode, { dropWholeCents: true })
  const place = notice.countryCode ? countryName(notice.countryCode) : null
  const subject = place ? `A new Cmdr sale: ${amount} from ${place}` : `A new Cmdr sale: ${amount}`
  const heading = `That's sale #${String(notice.saleNumber)}!`
  const intro = place
    ? `Someone in ${place} bought Cmdr. Time to celebrate!`
    : 'Someone bought Cmdr. Time to celebrate!'
  const rows = factRows(notice)
  const labelCell = `${CELL_STYLE} font-weight: bold; width: 160px;`

  const tableRows = rows
    .map((row) => {
      const value = row.href
        ? `<a href="${escapeHtml(row.href)}" style="color: #2563eb;">${escapeHtml(row.value)}</a>`
        : escapeHtml(row.value)
      return `        <tr>
            <td style="${labelCell}">${escapeHtml(row.label)}</td>
            <td style="${CELL_STYLE}">${value}</td>
        </tr>`
    })
    .join('\n')

  const html = documentShell(
    `    <h2 style="color: #059669;">${escapeHtml(heading)}</h2>

    <p>${escapeHtml(intro)}</p>

    <table style="${TABLE_STYLE}">
${tableRows}
    </table>

    ${signoffParagraph(SIGNOFF)}`,
    bodyStyle({ maxWidthPx: 600, color: '#333' }),
  )

  const text = [
    heading,
    '',
    intro,
    '',
    ...rows.map(
      (row) => `${row.label}: ${row.value}${row.href && !row.href.startsWith('mailto:') ? ` (${row.href})` : ''}`,
    ),
    '',
    SIGNOFF,
  ].join('\n')

  return { subject, html, text }
}

/** Mail one sale notice. Throws through `sendEmail` when SMTP2Go refuses it. */
export async function sendSaleNotice(params: { notice: SaleNotice; to: string; smtp2goApiKey: string }): Promise<void> {
  const { subject, html, text } = renderSaleNotice(params.notice)
  await sendEmail(
    params.smtp2goApiKey,
    { from: 'Cmdr Sales <noreply@getcmdr.com>', to: params.to, subject, html, text },
    'sale notice',
  )
}
