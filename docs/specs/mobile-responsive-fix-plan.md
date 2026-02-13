# Mobile responsive fix plan

The website looks broken on mobile (< 768px). This spec covers all fixes needed to make it look great on phones.

## Problem summary

The website was designed desktop-first with some responsive classes sprinkled in, but several critical pieces are
missing or broken on mobile viewports (especially 375px–430px, the iPhone range).

## Issues and fixes

### 1. Header: no mobile menu (critical)

**Problem**: The header renders all 7 nav items (Newsletter, Features, Pricing, Changelog, Roadmap, GitHub, Download)
plus the logo in a single flex row with `gap-8`. On a 375px screen, this overflows badly — items either wrap, get
squished, or cause horizontal scrolling.

**Fix**: Add a hamburger menu for screens below `md` (768px).

- Hide nav links and download button below `md` with `hidden md:flex`.
- Add a hamburger button (3-line icon) visible only below `md`.
- On click, toggle a mobile menu overlay/dropdown that shows the nav links stacked vertically.
- Include the Download CTA as a prominent button at the bottom of the mobile menu.
- Include the Newsletter toggle in the mobile menu as well.
- Close on Escape, outside click, and navigation.
- Use `transition` for a clean slide-down or fade animation.
- Keep it simple — no heavy libraries, just a `<div>` with toggled visibility.

### 2. Hero illustration overflow

**Problem**: `.hero-base` has `width: 1600px` with complex 3D transforms. The mobile media query scales to `0.7x`
and adjusts margins, but the parent `.hero-illustration` only uses `margin-left: -40px` and `height: 600px`. The
combination can still cause horizontal overflow and the illustration may look odd.

**Fix**:
- Add `overflow: hidden` to `.hero-section` (already has `overflow-hidden` in Tailwind classes — verify this covers the
  illustration).
- Reduce `.hero-base` scale further on very small screens if needed, or hide the 3D illustration on very small screens
  and show a flat screenshot instead.
- Test and adjust margins/scale so the hero image fills the viewport width nicely without overflow.

### 3. Newsletter form: inline variant on narrow screens

**Problem**: The "inline" variant uses `flex-direction: row`, which means on a ~300px container the email input gets
squeezed to an unusable width alongside the "Sign up" button.

**Fix**:
- Add a media query or `flex-wrap: wrap` behavior so the inline form stacks on very narrow screens:
  ```css
  @media (max-width: 480px) {
      .newsletter-form--inline {
          flex-direction: column;
      }
      .newsletter-form--inline .newsletter-form__input {
          flex: 1 1 auto;
          width: 100%;
      }
      .newsletter-form--inline .newsletter-form__button {
          width: 100%;
      }
  }
  ```

### 4. Horizontal overflow protection

**Problem**: No global protection against horizontal overflow. The hero image transforms, wide elements, and possibly
the pricing grid could cause a horizontal scrollbar on mobile.

**Fix**:
- Add `overflow-x: hidden` to `html` or `body` in `global.css`.
- This is a safety net — individual components should also be fixed, but this prevents the page-level scrollbar.

### 5. Pricing page: card grid and badge

**Problem**: The pricing grid (`lg:grid-cols-4 md:grid-cols-2`) stacks to 1 column on mobile, which is fine. But:
- The "Most popular" badge (`absolute -top-3 left-1/2 -translate-x-1/2`) may clip against the top of its container
  when stacked since there's no extra margin-top on that card in single-column mode.
- The card padding could be slightly reduced on mobile for better use of space.

**Fix**:
- Add `mt-4 md:mt-0` (or similar) to the "Most popular" card so the badge has room when stacked.
- Consider `p-5` instead of `p-6` on mobile for a tighter look.

### 6. Footer: newsletter section width

**Problem**: The footer newsletter section uses `md:flex-row` and `md:min-w-[320px]`. On mobile it stacks properly
(`flex-col`), but the newsletter form container is `w-full` which is fine — just needs verification.

**Fix**: Verify only — this likely works already. The `w-full md:w-auto md:min-w-[320px]` pattern is correct.

### 7. Text sizing on small screens

**Problem**: Hero headline uses `text-4xl` (36px) on mobile. Section headings like Features and Download also use
`text-4xl`. These are large on a 375px screen.

**Fix**:
- Hero: Consider `text-3xl md:text-4xl lg:text-6xl` for better mobile scaling.
- Section headings: `text-3xl md:text-4xl lg:text-5xl`.
- Subheadlines: Verify `text-lg` is comfortable on mobile.

### 8. Roadmap page on mobile

**Problem**: The roadmap already has `@media (max-width: 640px)` handling. The `flex-direction: column` with
`margin-left: 1.75rem` for indent looks reasonable.

**Fix**: Verify only — likely fine but test with actual mobile viewport.

### 9. Newsletter on mobile

**Problem**: On desktop, clicking "Newsletter" in the header opens a slide-down panel. On mobile (where we're adding
a hamburger menu), this interaction needs rethinking. A slide-down panel inside a mobile menu is awkward UX.

**Options** (team to decide):
- A: Show the newsletter form inline in the mobile menu (below the nav links).
- B: Link to a dedicated `/newsletter` page from mobile.
- C: Open the same slide-down panel, but from the mobile menu context.

**Guidance**: Whatever feels most modern and natural. The newsletter signup should be easy to find but not intrusive.

### 10. Accessibility

All mobile changes must meet these requirements:
- WCAG 2.1 AA contrast ratios (4.5:1 for text, 3:1 for large text/UI).
- Hamburger button must have `aria-label`, `aria-expanded`, and `aria-controls`.
- Mobile menu must trap focus when open (or at least manage focus logically).
- All interactive elements must have visible focus indicators.
- Mobile menu must be navigable with keyboard (Tab, Escape to close).
- Touch targets must be at least 44x44px.
- `prefers-reduced-motion` must be respected for any new animations.
- Screen reader users must be able to navigate the mobile menu.

## Out of scope

- Dark/light mode toggle (currently dark only).
- Touch gesture support (swipe, etc.).
- Mobile-specific features like "Add to Home Screen" prompts.

## Testing

- Resize browser to 375px (iPhone SE), 390px (iPhone 14), 430px (iPhone 14 Pro Max).
- Check every page: homepage, pricing, changelog, roadmap, legal pages.
- Verify no horizontal scrollbar on any page.
- Verify the mobile menu opens/closes correctly, navigates properly.
- Playwright e2e tests can be used where helpful.
- Run `./scripts/check.sh --check website-prettier --check website-eslint --check stylelint --check website-typecheck`.
- Accessibility: verify with keyboard navigation, check contrast ratios, test with screen reader.
