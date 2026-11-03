/**
 * The closed-tab history setting, as the i18n capture's staging needs it.
 *
 * The E2E suite can't import the settings registry (its type graph pulls in the app's
 * bundler-only modules), so the default is restated here and pinned to the registry by
 * `src/lib/settings/e2e-setting-mirrors.test.ts`.
 */
export const CLOSED_TAB_HISTORY_SIZE = 'fileExplorer.tabs.closedTabHistorySize'
export const CLOSED_TAB_HISTORY_DEFAULT = 10
