# Newsletter dismiss & subscribe state plan

## Goal

Respect users who've already subscribed or dismissed the newsletter by hiding CTAs site-wide,
while keeping the footer signup as a low-friction fallback.

## Two states

- **Subscribed**: User successfully submitted the form. `localStorage('newsletter-subscribed', 'true')`
- **Dismissed**: User clicked "Not interested". `localStorage('newsletter-dismissed', 'true')`

## Behavior per location

| Location                    | Subscribed               | Dismissed | Default |
|-----------------------------|--------------------------|-----------|---------|
| Header nav (desktop panel)  | Hidden                   | Hidden    | Visible |
| Header mobile menu          | Hidden                   | Hidden    | Visible |
| Blog index CTA              | Hidden                   | Hidden    | Visible |
| Blog post CTA               | Hidden                   | Hidden    | Visible |
| Download section (homepage) | "You're on the list"     | Hidden    | Visible |
| Roadmap page                | "You're on the list"     | Hidden    | Visible |
| Changelog page              | "You're on the list"     | Hidden    | Visible |
| Footer                      | Visible (unchanged)      | Visible   | Visible |

## Dismiss UX

- **BlogNewsletterCta**: Replace the current X button with a "Not interested" text link (bottom-right or
  below the form). Less accidental than an X, requires intentional reading.
- **Inline page CTAs** (download, roadmap, changelog): Add a "Not interested" text link below the form.
- **Header**: No dismiss UI of its own — reacts to state set elsewhere.
- **Footer**: No dismiss UI, always visible.

## After dismiss (inline confirmation)

Replace the CTA with a one-liner:
> "Hidden across the site. Changed your mind? There's a signup in the footer."

This fades out after a few seconds or disappears on next navigation. Respect `prefers-reduced-motion`.

## After subscribe

- On pages with inline CTAs (download, roadmap, changelog): replace the form with "You're on the list" + checkmark.
- On blog CTAs and header: just hide entirely (no replacement text needed — these are interruptive placements).

## Implementation

### Shared utility

Create a small module (or inline in a shared `<script>`) for reading newsletter state:

```ts
const STORAGE_KEYS = { subscribed: 'newsletter-subscribed', dismissed: 'newsletter-dismissed' }
function getNewsletterState(): 'subscribed' | 'dismissed' | null
function setNewsletterSubscribed(): void
function setNewsletterDismissed(): void
```

### NewsletterForm.astro changes

- On successful POST response, call `setNewsletterSubscribed()`.
- The form itself doesn't need to check state on load — the *parent* wrapper/page handles visibility.

### BlogNewsletterCta.astro changes

- Replace X button with "Not interested" text link.
- On dismiss, write to the shared `newsletter-dismissed` key (not the old `newsletter-cta-dismissed` key).
- Show inline confirmation message after dismiss.
- On page load, check both `newsletter-subscribed` and `newsletter-dismissed` — hide if either is true.
- Migrate: also check legacy `newsletter-cta-dismissed` key for existing dismissed users.

### Header.astro changes

- Wrap the newsletter nav button (desktop + mobile) in an element with `data-newsletter-nav`.
- On `astro:page-load`, check state and hide if subscribed or dismissed.

### Page-level inline CTAs (Download.astro, roadmap.astro, changelog.astro)

- Wrap each CTA section in an element with `data-newsletter-inline`.
- Add "Not interested" text link below the form.
- On page load: if subscribed, replace with "You're on the list"; if dismissed, hide entirely.
- On dismiss click, same inline confirmation behavior as BlogNewsletterCta.

### E2E tests

- Update `newsletter.spec.ts` to test dismiss behavior on header, download section.
- Update `blog.spec.ts` to test new dismiss link (replaces X button tests).
- Test that dismissing on one page hides CTAs on other pages.
- Test that successful subscription hides CTAs on other pages.
- Test that footer remains visible in both states.
