// The tier 2 fixture for i18n key folding: every shape the feature handles, plus the ones it deliberately leaves
// alone. Keys resolve against `apps/desktop/src/lib/intl/messages/en/sandbox.json` in this same fixture project.

export function dialogCopy() {
  return {
    title: tString('crashReporter.dialog.title'),
    body: t('crashReporter.dialog.body'),
    progress: getMessage('transfer.progress.copying'),
  }
}

export const providerSetting = {
  labelKey: 'settings.ai.provider.label',
  descriptionKey: 'settings.ai.provider.description',
}

// Left alone on purpose: a key the catalog doesn't have, a bare string that isn't a key site, and a key built by
// template, which is the accepted miss.
export function notFolded(provider: string) {
  return [tString('crashReporter.dialog.renamedAway'), 'crashReporter.dialog.title', getMessage(`errors.${provider}`)]
}
