/**
 * Base-locale (en) parity net for the onboarding i18n migration.
 *
 * Every user-facing onboarding string moved from hardcoded English in the
 * `lib/onboarding/` components into the `onboarding.*` catalog (resolved through
 * `t()` / `<Trans>`). This is a behavior-preserving MOVE: every rendered en
 * string must be byte-identical to the pre-migration copy. The goldens below are
 * the literals that lived in the components before the move; a future copy edit
 * lands in the catalog AND here together, never silently.
 *
 * Rich-text (`<Trans>`) messages are asserted via `t()` returning the raw
 * array of strings + tag markers; we join the plain-string parts and check the
 * inline-tag positions, since the rendered text (with HTML whitespace collapsing)
 * matches the original sentence run for run.
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import { _setLocaleForTests } from '$lib/intl/locale'
import { t, tString, type TranslationParams } from '$lib/intl/messages.svelte'
import type { MessageKey } from '$lib/intl/keys.gen'

beforeAll(() => {
  _setLocaleForTests('en-US')
})
afterAll(() => {
  _setLocaleForTests(null)
})

/** Marker shape that `t()` emits for each `<tag>` when given a tag handler. */
interface Marker {
  __trans: true
  tag: string
  chunks: unknown[]
}

/**
 * Renders a rich-text message to a flat string the way the eye sees it: tag
 * handlers wrap their inner chunks in the literal tag, so `<strong>x</strong>`
 * round-trips to `<strong>x</strong>`. Lets us assert the full sentence including
 * where the inline components sit, in one golden string.
 */
function renderRich(key: MessageKey, tags: string[], params?: TranslationParams): string {
  const handlers: TranslationParams = { ...params }
  for (const tag of tags) {
    handlers[tag] = (chunks: unknown[]): Marker => ({ __trans: true, tag, chunks })
  }
  const result = t(key, handlers)
  const parts = Array.isArray(result) ? (result as unknown[]) : [result]
  return parts
    .map((part) => {
      if (typeof part === 'object' && part !== null && '__trans' in part) {
        const marker = part as Marker
        return `<${marker.tag}>${marker.chunks.join('')}</${marker.tag}>`
      }
      return String(part)
    })
    .join('')
}

const SYS_SETTINGS = 'System Settings'

describe('onboarding wizard chrome parity (en)', () => {
  it('resolves the sr-only title and progress labels', () => {
    expect(tString('onboarding.wizard.title')).toBe('Cmdr onboarding')
    expect(tString('onboarding.wizard.progressLabel')).toBe('Onboarding progress')
  })

  it('resolves the step-dot progress text, with and without the optional suffix', () => {
    expect(tString('onboarding.wizard.stepProgress', { step: 2, total: 4, isOptional: false })).toBe('Step 2 of 4')
    expect(tString('onboarding.wizard.stepProgress', { step: 4, total: 4, isOptional: true })).toBe(
      'Step 4 of 4 (optional)',
    )
  })

  it('resolves the footer button labels', () => {
    expect(tString('onboarding.wizard.back')).toBe('Back')
    expect(tString('onboarding.wizard.backAria')).toBe('Go to previous step')
    expect(tString('onboarding.wizard.next')).toBe('Next')
    expect(tString('onboarding.wizard.finish')).toBe('Finish')
    expect(tString('onboarding.wizard.restart')).toBe('Restart Cmdr')
  })
})

describe('onboarding step 1 (FDA) parity (en)', () => {
  it('resolves the granted-success copy', () => {
    expect(tString('onboarding.stepFda.granted.title')).toBe('You granted full disk access!')
    expect(tString('onboarding.stepFda.granted.body')).toBe(
      "Nice, that's all Cmdr needs. Restart it now to start using everything.",
    )
    expect(tString('onboarding.stepFda.granted.hint')).toBe(
      "Cmdr picks up the new permission on the next launch. Your spot in onboarding is saved, so you'll land right back here.",
    )
  })

  it('resolves the already-granted copy with the system-settings interpolation', () => {
    expect(tString('onboarding.stepFda.alreadyGranted.title')).toBe('Cmdr currently has full disk access')
    expect(tString('onboarding.stepFda.alreadyGranted.body', { systemSettings: SYS_SETTINGS })).toBe(
      'You can revoke it any time in System Settings.',
    )
  })

  it('resolves the welcome heading and the revoked-variant copy', () => {
    expect(tString('onboarding.stepFda.welcome.title')).toBe('Welcome to Cmdr!')
    expect(tString('onboarding.stepFda.revoked.intro')).toBe(
      'It looks like you accepted full disk access before but then revoked it.',
    )
    expect(tString('onboarding.stepFda.revoked.noAccess')).toBe('The app currently has no full disk access.')
    expect(renderRich('onboarding.stepFda.revoked.ifIntentional', ['deny'])).toBe(
      "If that was intentional, click <deny>Deny</deny> and the app won't bother you again.",
    )
    expect(renderRich('onboarding.stepFda.revoked.ifNot', ['em'])).toBe(
      "If it <em>wasn't</em> intentional, consider allowing full disk access again. Here are the pros and cons:",
    )
  })

  it('resolves the first-ask copy', () => {
    expect(renderRich('onboarding.stepFda.firstAsk.lede', ['strong'])).toBe(
      "<strong>You probably just want to start using the app.</strong> Sorry to bother you with this first, but it's needed.",
    )
    expect(tString('onboarding.stepFda.firstAsk.explain')).toBe(
      "You see, Cmdr is a file manager, and it needs to access your disk to see all your files. macOS doesn't automatically grant permission to this.",
    )
    expect(tString('onboarding.stepFda.firstAsk.askPermission')).toBe(
      "Would you like to give this app full disk access? Here's what that means:",
    )
  })

  it('resolves the pros/cons bullets', () => {
    expect(renderRich('onboarding.stepFda.pro', ['strong'])).toBe(
      '<strong>Pro:</strong> The app will access your entire disk without nagging you for permissions to each folder like Downloads, Documents, and Desktop.',
    )
    expect(renderRich('onboarding.stepFda.con', ['strong', 'sourceLink'])).toBe(
      '<strong>Con:</strong> Full disk access is pretty powerful. It lets the app read any file on your Mac. Only grant this if you trust Cmdr. Cmdr uses this right respectfully, and is <sourceLink>source-available</sourceLink> if you feel unsure.',
    )
  })

  it('resolves the allow-steps and the settings buttons', () => {
    expect(tString('onboarding.stepFda.ifAllow')).toBe('If you decide to allow:')
    expect(tString('onboarding.stepFda.openSettings', { systemSettings: SYS_SETTINGS })).toBe('Open System Settings')
    expect(renderRich('onboarding.stepFda.step1', ['strong'], { systemSettings: SYS_SETTINGS })).toBe(
      'Click <strong>Open System Settings</strong> below',
    )
    expect(renderRich('onboarding.stepFda.step2.ventura', ['strong'])).toBe(
      'Find <strong>Cmdr</strong> in the list and toggle it on',
    )
    expect(renderRich('onboarding.stepFda.step2.older', ['strong'])).toBe(
      'Find <strong>Cmdr</strong> at the end of the list and toggle it on',
    )
    expect(renderRich('onboarding.stepFda.step2.tip', ['strong'])).toBe(
      'Tip: Is Cmdr not in the list? Click the "+" button at the bottom, and choose <strong>Cmdr</strong> from your <strong>Applications</strong> folder.',
    )
    expect(renderRich('onboarding.stepFda.step3', ['strong'])).toBe('Confirm and click <strong>Quit & Reopen</strong>')
    expect(tString('onboarding.stepFda.deny')).toBe('Deny')
  })

  it('resolves the post-action restart hint', () => {
    expect(tString('onboarding.stepFda.postAction.intro')).toBe(
      'Cmdr needs to restart so the new permission takes effect.',
    )
    expect(renderRich('onboarding.stepFda.postAction.body', ['restart', 'deny'])).toBe(
      "When you're ready, click <restart>Restart Cmdr</restart> below. If you change your mind, click <deny>Deny</deny> above instead.",
    )
  })
})

describe('onboarding step 2 (AI) parity (en)', () => {
  it('resolves the three FDA-outcome banners', () => {
    expect(tString('onboarding.stepAi.bannerTitle.granted')).toBe('Full disk access granted')
    expect(tString('onboarding.stepAi.bannerBody.granted')).toBe(
      'Thanks for granting full disk access! Now, the app can access your disk. Great!',
    )
    expect(tString('onboarding.stepAi.bannerTitle.denied')).toBe('No full disk access')
    expect(tString('onboarding.stepAi.bannerBody.denied', { systemSettings: SYS_SETTINGS })).toBe(
      "You chose not to enable full disk access. We respect that. You'll then shortly get a few permission requests from macOS for Cmdr to access your Desktop, Downloads, and similar folders. Accept or reject these at will. You can change all of this later in your System Settings.",
    )
    expect(tString('onboarding.stepAi.bannerTitle.stuck')).toBe("Cmdr doesn't seem to have full disk access yet")
    expect(renderRich('onboarding.stepAi.bannerBody.stuck', ['settingsLink'], { systemSettings: SYS_SETTINGS })).toBe(
      'You said you wanted to enable full disk access, but Cmdr doesn\'t seem to have gotten it. You might need to restart the app (do it now, we\'ll continue from here!), or go to your <settingsLink>System Settings > Privacy & Security > Full Disk Access</settingsLink> and find Cmdr, or manually add it with the little "+" button at the bottom.',
    )
  })

  it('resolves the headings, intro, and comparison table', () => {
    expect(tString('onboarding.stepAi.welcomeLinux.title')).toBe('Welcome to Cmdr!')
    expect(tString('onboarding.stepAi.welcomeLinux.subtitle')).toBe("Let's set up AI.")
    expect(tString('onboarding.stepAi.title')).toBe("Now, let's talk AI")
    expect(renderRich('onboarding.stepAi.intro', ['em'])).toBe(
      'Cmdr has a bunch of AI features that you <em>may</em> want and may not want. AI is a controversial topic these days.',
    )
    expect(tString('onboarding.stepAi.comparisonIntro')).toBe('Here is how you do common actions with and without AI:')
    expect(tString('onboarding.stepAi.table.colFeature')).toBe('Feature')
    expect(tString('onboarding.stepAi.table.colWithout')).toBe('Without AI')
    expect(tString('onboarding.stepAi.table.colWith')).toBe('With AI')
    expect(tString('onboarding.stepAi.table.rowSearch')).toBe('Search')
    expect(renderRich('onboarding.stepAi.table.searchWithout', ['code'])).toBe(
      'You type something like <code>*fish*.ppt</code>, and select the "after 1st of this month" filter.',
    )
    expect(tString('onboarding.stepAi.table.searchWith')).toBe(
      'You say "my recent fish-related presentations", agent sets your filters.',
    )
    expect(tString('onboarding.stepAi.table.rowRename')).toBe('Mass-rename')
    expect(tString('onboarding.stepAi.table.renameWithout')).toBe(
      'You use the batch rename UI to manually set the rename pattern, review and apply.',
    )
    expect(tString('onboarding.stepAi.table.renameWith')).toBe(
      'You say "add ISO date prefix", agent sets your rename pattern, you review and apply at will.',
    )
    expect(tString('onboarding.stepAi.table.rowSelect')).toBe('Select')
    expect(renderRich('onboarding.stepAi.table.selectWithoutBound', ['chip', 'code'])).toBe(
      'You press the <chip></chip> key and type something like <code>*.jpg,*.png,*.gif,*.heic,*.webp,*.jpeg</code>, review and apply.',
    )
    expect(renderRich('onboarding.stepAi.table.selectWithoutUnbound', ['code'])).toBe(
      'You open "Select files…" and type something like <code>*.jpg,*.png,*.gif,*.heic,*.webp,*.jpeg</code>, review and apply.',
    )
    expect(tString('onboarding.stepAi.table.selectWith')).toBe(
      'You say "select all image files", agent suggests a selection, you review and apply at will.',
    )
  })

  it('resolves the resume cue, legend, and the three choices', () => {
    expect(tString('onboarding.stepAi.resumeCue')).toBe('You picked this last time. Confirm or change below.')
    expect(tString('onboarding.stepAi.choiceLegend')).toBe('Based on this, do you want AI or not?')
    expect(tString('onboarding.stepAi.choiceGroupAria')).toBe('AI choice')
    expect(tString('onboarding.stepAi.cloud.label')).toBe('Yes, I want AI')
    expect(tString('onboarding.stepAi.cloud.recommended')).toBe('(recommended)')
    expect(tString('onboarding.stepAi.cloud.help')).toBe(
      'Use any cloud provider with your own API key. Fast, high-quality models. Pick a provider below.',
    )
    expect(tString('onboarding.stepAi.cloud.pickerTitle')).toBe('Select a provider')
    expect(tString('onboarding.stepAi.local.label')).toBe('Yes, I want AI, but I want to be super private')
    expect(tString('onboarding.stepAi.local.help')).toBe(
      'A bit dumber model that takes up about 2 GB of space and a bit of CPU at every use. Still an okay solution. No data leaves your machine. Cmdr tries to deliver updates for the best small local model available.',
    )
    expect(tString('onboarding.stepAi.local.note')).toBe(
      'Started downloading the local model in the background. You can finish onboarding now; the toast in the corner will keep you posted.',
    )
    expect(tString('onboarding.stepAi.localTooltip')).toBe('Local LLM requires Apple Silicon. Cloud works on Intel.')
    expect(tString('onboarding.stepAi.off.label')).toBe('Thanks but no thanks, no AI for me')
    expect(tString('onboarding.stepAi.off.help')).toBe(
      'Cmdr works fully without AI. You can turn it on later in Settings.',
    )
  })
})

describe('onboarding step 3 (open beta) parity (en)', () => {
  it('resolves the footer buttons', () => {
    expect(tString('onboarding.stepBeta.footer.start')).toBe('Start using Cmdr!')
    expect(tString('onboarding.stepBeta.footer.continue')).toBe('One more optional setup step')
  })

  it('resolves the personal intro and feedback channels', () => {
    expect(tString('onboarding.stepBeta.title')).toBe('Help improve Cmdr!')
    expect(renderRich('onboarding.stepBeta.greeting', ['david'])).toBe(
      "Hi, I'm <david>David</david>! I build Cmdr, and you're one of the very first people using it. Thanks for your trust! ❤️",
    )
    expect(renderRich('onboarding.stepBeta.openBeta', ['alpha'])).toBe(
      "Cmdr is in open beta, which means it's overall solid and usable, but some parts are still rough. See any <alpha></alpha> badges marking the most work-in-progress areas.",
    )
    expect(tString('onboarding.stepBeta.feedbackIntro')).toBe(
      'Your feedback helps me spot bugs and prioritize features. Here is how you can engage:',
    )
    expect(renderRich('onboarding.stepBeta.feedback.inAppBound', ['strong', 'chip'])).toBe(
      '<strong>In-app:</strong> See <strong>Help > Send feedback…</strong> in the menu, or find it in the command palette with <chip></chip>.',
    )
    expect(renderRich('onboarding.stepBeta.feedback.inAppUnbound', ['strong'])).toBe(
      '<strong>In-app:</strong> See <strong>Help > Send feedback…</strong> in the menu, or find it in the command palette.',
    )
    expect(renderRich('onboarding.stepBeta.feedback.github', ['github'])).toBe(
      '<github>GitHub</github>: Add issues, vote on issues.',
    )
    expect(renderRich('onboarding.stepBeta.feedback.discord', ['discord'])).toBe(
      '<discord>Discord</discord>: Click the link, hop on to the server, meet me and others.',
    )
    expect(renderRich('onboarding.stepBeta.feedback.call', ['call'])).toBe(
      "<call>Schedule a call with me</call>: I won't be doing this for very long, but while Cmdr is an open beta, I'd love to talk to you about your files!",
    )
    expect(renderRich('onboarding.stepBeta.star', ['github', 'code'])).toBe(
      'And one more very important way you can help in one minute: star the repo <github>here on GitHub</github>. Once it hits 225 stars, Homebrew lets me enable <code>brew install cmdr</code>.',
    )
  })

  it('resolves the analytics and email blocks', () => {
    expect(tString('onboarding.stepBeta.analyticsLede')).toBe(
      "To learn what's working and what isn't, during the open beta Cmdr sends anonymous usage stats: which features get used and how often, never anything from your files. It's on now, and you can turn it off anytime.",
    )
    expect(tString('onboarding.stepBeta.analyticsTitle')).toBe('Send anonymous usage stats')
    expect(tString('onboarding.stepBeta.analyticsCaption')).toBe(
      "Note that it's ON by default to encourage people to send me data during the Beta. You can change this any time in Settings.",
    )
    expect(tString('onboarding.stepBeta.emailTitle')).toBe('Stay in touch (optional)')
    expect(tString('onboarding.stepBeta.emailPlaceholder')).toBe('you@example.com')
    expect(tString('onboarding.stepBeta.signup.success')).toBe(
      'Check your inbox to confirm your email. Thanks for helping out!',
    )
    expect(tString('onboarding.stepBeta.signup.failure')).toBe("Sorry, we couldn't sign you up right now. Try again?")
    expect(tString('onboarding.stepBeta.emailNote')).toBe(
      "Drop your email and I'll reach out with the occasional question or update. The email address you enter here is stored only on your Mac and it's never connected to your usage stats, the two are intentionally two separate subsystems.",
    )
  })
})

describe('onboarding step 4 (optional setup) parity (en)', () => {
  it('resolves the heading, lede, footer, and shared caption', () => {
    expect(tString('onboarding.stepOptional.footer.start')).toBe('Start using Cmdr')
    expect(tString('onboarding.stepOptional.title')).toBe("You're almost ready")
    expect(tString('onboarding.stepOptional.lede')).toBe(
      "You chose to walk through a detailed setup, so here are a few easy choices. If you don't care too much, just click the button below. These are all options, and the defaults are picked for your benefit.",
    )
    expect(tString('onboarding.stepOptional.recommendedOn')).toBe(
      'Recommended: on. You can change this any time in Settings.',
    )
  })

  it('resolves the four toggle blocks', () => {
    expect(tString('onboarding.stepOptional.networking.title')).toBe('Networking')
    expect(renderRich('onboarding.stepOptional.networking.desc', ['em'])).toBe(
      'Having this <em>on</em> means you can connect to SMB servers like company network shares, a home NAS, and the like. The only cost is a macOS permission dialog that pops up and asks you to allow "Local network access", and one for "Accepting incoming connections". Both dialogs are harmless, but if you don\'t know what these are, they might be scary or annoying.',
    )
    expect(tString('onboarding.stepOptional.indexing.title')).toBe('Drive indexing')
    expect(tString('onboarding.stepOptional.indexing.descIntro')).toBe(
      'Drive indexing is totally cool! Gives you two main things:',
    )
    expect(tString('onboarding.stepOptional.indexing.benefit1')).toBe(
      'Instant search of your whole drive. Think Spotlight, but even faster.',
    )
    expect(tString('onboarding.stepOptional.indexing.benefit2')).toBe(
      'Real-time folder sizes for your whole drive. You always know how much stuff you have in each folder.',
    )
    expect(renderRich('onboarding.stepOptional.indexing.descCost', ['code'])).toBe(
      "If you turn this off, you only get <code>&lt;DIR&gt;</code> for the sizes. The cost is a 300 MB index on your drive, but no extra CPU or memory use after the first 2&ndash;3 minutes of you first starting the app, or starting it after a long time. It's a cheap feature considering the benefits.",
    )
    expect(tString('onboarding.stepOptional.updates.title')).toBe('Automatic updates')
    expect(tString('onboarding.stepOptional.updates.desc')).toBe(
      "If you enable this, Cmdr makes a tiny network request to a central license server at each app start plus once every 24 hours, and you always get the latest updates. If disabled, you'll keep your current version, and zero automated network requests (except for periodic license checks, if you have a commercial license).",
    )
    expect(tString('onboarding.stepOptional.mtp.title')).toBe('MTP (Android phones, Kindles, cameras)')
    expect(renderRich('onboarding.stepOptional.mtp.desc', ['strong', 'em'])).toBe(
      "If you enable this, Cmdr can <strong>connect to Android phones, Kindles, cameras</strong>, some music players, and any other device that supports the protocols called MTP or PTP. The cost is that macOS <em>also</em> wants to connect to these (and it usually fails, which is why you can't just use Finder to copy photos from Android phones), so Cmdr has to suppress that macOS process while it's running. When you quit Cmdr, this is politely restored. But it's a bit of a cost, so:",
    )
  })
})

describe('onboarding cloud provider picker/setup parity (en)', () => {
  it('resolves the picker list label', () => {
    expect(tString('onboarding.cloudPicker.listAria')).toBe('Cloud AI providers')
  })

  it('resolves the setup title and steps', () => {
    expect(tString('onboarding.cloudSetup.title', { provider: 'OpenAI' })).toBe('Set up OpenAI')
    expect(renderRich('onboarding.cloudSetup.step.signup', ['signupLink'], { provider: 'OpenAI' })).toBe(
      "Sign up at <signupLink>OpenAI</signupLink> (if you don't have an account)",
    )
    expect(renderRich('onboarding.cloudSetup.step.createKey', ['keyLink'])).toBe(
      'Create an API key <keyLink>here</keyLink>',
    )
    expect(tString('onboarding.cloudSetup.step.endpoint')).toBe('Endpoint URL')
    expect(tString('onboarding.cloudSetup.step.endpointPlaceholder')).toBe('Example: https://api.example.com/v1')
    expect(tString('onboarding.cloudSetup.step.pasteKey')).toBe('Paste your API key')
    expect(tString('onboarding.cloudSetup.apiKeyAria')).toBe('API key')
    expect(tString('onboarding.cloudSetup.step.pickModel')).toBe('Pick a model')
    expect(tString('onboarding.cloudSetup.modelAria')).toBe('Model')
  })

  it('resolves the API-key placeholders', () => {
    expect(tString('onboarding.cloudSetup.apiKeyPlaceholder.openai')).toBe('Example: sk-abc123...')
    expect(tString('onboarding.cloudSetup.apiKeyPlaceholder.anthropic')).toBe('Example: sk-ant-abc123...')
    expect(tString('onboarding.cloudSetup.apiKeyPlaceholder.generic')).toBe('API key')
  })

  it('resolves the model placeholders', () => {
    expect(tString('onboarding.cloudSetup.modelPlaceholderExample', { model: 'gpt-4o' })).toBe('Example: gpt-4o')
    expect(tString('onboarding.cloudSetup.modelPlaceholder')).toBe('Model name')
  })

  it('resolves the connection status strings', () => {
    expect(tString('onboarding.cloudSetup.status.checking')).toBe('Checking your key…')
    expect(tString('onboarding.cloudSetup.status.authError')).toBe("That key didn't work")
    expect(tString('onboarding.cloudSetup.status.connectionError')).toBe("Can't reach the service right now")
    expect(tString('onboarding.cloudSetup.status.genericError')).toBe('Something went wrong')
    expect(tString('onboarding.cloudSetup.status.connected')).toBe('Connected!')
  })
})
