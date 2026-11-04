import { describe, expect, it } from 'vitest'
import { formatMoney, renderSaleNotice, type SaleNotice } from './sale'

/**
 * The celebration email David gets for every sale. He reads the subject in a notification banner,
 * so it has to say the amount and the place on its own; the body carries everything needed to
 * thank the buyer or look the sale up.
 */
const notice: SaleNotice = {
  saleNumber: 2,
  transactionId: 'txn_01m5f98hga6h76qgcek3kd45vj',
  paddleEnvironment: 'live',
  billedAt: '2026-10-21T13:12:03.620649Z',
  currencyCode: 'USD',
  grandTotalMinor: 5900,
  earningsMinor: 5555,
  productName: 'Cmdr',
  priceName: 'Commercial license',
  quantity: 1,
  countryCode: 'DE',
  buyerName: 'Robin Doe',
  buyerEmail: 'robin@example.com',
  businessName: null,
}

describe('formatMoney', () => {
  it('reads Paddle amounts as cents', () => {
    expect(formatMoney(7021, 'USD')).toBe('$70.21')
  })

  it("respects a currency that has no cents, since Paddle's smallest unit for yen is one yen", () => {
    expect(formatMoney(5900, 'JPY')).toBe('¥5,900')
  })

  it('drops the cents on a whole amount when asked to, for the subject line', () => {
    expect(formatMoney(5900, 'USD', { dropWholeCents: true })).toBe('$59')
    expect(formatMoney(7021, 'USD', { dropWholeCents: true })).toBe('$70.21')
  })
})

describe('renderSaleNotice', () => {
  it('names the amount and the country in the subject', () => {
    expect(renderSaleNotice(notice).subject).toBe('A new Cmdr sale: $59 from Germany')
  })

  it('leaves the place out of the subject when Paddle has no address', () => {
    expect(renderSaleNotice({ ...notice, countryCode: null }).subject).toBe('A new Cmdr sale: $59')
  })

  it('carries the running count, the money, the buyer, and a link to the sale in Paddle', () => {
    const { html, text } = renderSaleNotice(notice)

    for (const body of [html, text]) {
      expect(body).toContain('sale #2')
      expect(body).toContain('$59.00 USD')
      expect(body).toContain('$55.55')
      expect(body).toContain('Cmdr, Commercial license')
      expect(body).toContain('Germany')
      expect(body).toContain('Robin Doe')
      expect(body).toContain('robin@example.com')
      expect(body).toContain('txn_01m5f98hga6h76qgcek3kd45vj')
      expect(body).toContain('https://vendors.paddle.com/transactions-v2/txn_01m5f98hga6h76qgcek3kd45vj')
      expect(body).toContain('2026-10-21')
    }
  })

  it('links a sandbox sale to the sandbox dashboard', () => {
    const { html } = renderSaleNotice({ ...notice, paddleEnvironment: 'sandbox' })

    expect(html).toContain('https://sandbox-vendors.paddle.com/transactions-v2/')
  })

  it('says so when the buyer gave no name, and shows a company and seat count when there is one', () => {
    const { html, text } = renderSaleNotice({ ...notice, buyerName: null, businessName: 'Acme Inc', quantity: 3 })

    expect(text).toContain('No name given')
    expect(text).toContain('Acme Inc')
    expect(text).toContain('3 seats')
    expect(html).toContain('Acme Inc')
  })

  it('escapes what the buyer typed', () => {
    const { html } = renderSaleNotice({ ...notice, buyerName: '<script>alert(1)</script>' })

    expect(html).not.toContain('<script>')
    expect(html).toContain('&lt;script&gt;')
  })
})
