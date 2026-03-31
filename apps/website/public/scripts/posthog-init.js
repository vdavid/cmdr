/* PostHog session replay, heatmaps, click tracking — loaded from CDN to avoid bundling dompurify. */
/* eslint-disable */
// noinspection CommaExpressionJS,EqualityComparisonWithCoercionJS,JSReferencingMutableVariableFromClosure,ES6ConvertVarToLetConst,JSValidateTypes

!(function (t, e) {
  var o, n, p, r
  e.__SV ||
    ((window.posthog = e),
    (e._i = []),
    (e.init = function (i, s, a) {
      function g(t, e) {
        var o = e.split('.')
        ;(2 == o.length && ((t = t[o[0]]), (e = o[1])),
          (t[e] = function () {
            t.push([e].concat(Array.prototype.slice.call(arguments, 0)))
          }))
      }
      ;(((p = t.createElement('script')).type = 'text/javascript'),
        (p.crossOrigin = 'anonymous'),
        (p.async = !0),
        (p.src = s.api_host.replace('.i.posthog.com', '-assets.i.posthog.com') + '/static/array.js'),
        (r = t.getElementsByTagName('script')[0]).parentNode.insertBefore(p, r))
      var u = e
      for (
        void 0 !== a ? (u = e[a] = []) : (a = 'posthog'),
          u.people = u.people || [],
          u.toString = function (t) {
            var e = 'posthog'
            return ('posthog' !== a && (e += '.' + a), t || (e += ' (stub)'), e)
          },
          u.people.toString = function () {
            return u.toString(1) + '.people (stub)'
          },
          o =
            'init capture register register_once register_for_session unregister opt_out_capturing has_opted_out_capturing opt_in_capturing reset isFeatureEnabled getFeatureFlag getFeatureFlagPayload reloadFeatureFlags group updateEarlyAccessFeatureEnrollment getEarlyAccessFeatures on onFeatureFlags onSessionId getSurveys getActiveMatchingSurveys renderSurvey canRenderSurvey getNextSurveyStep identify setPersonProperties'.split(
              ' ',
            ),
          n = 0;
        n < o.length;
        n++
      )
        g(u, o[n])
      e._i.push([i, s, a])
    }),
    (e.__SV = 1))
})(document, window.posthog || [])
if (!window.__posthog_initialized)
  posthog.init(document.currentScript.dataset.phKey, {
    api_host: document.currentScript.dataset.phHost,
    person_profiles: 'identified_only',
    capture_pageleave: true,
    persistence: 'memory',
  })
window.__posthog_initialized = true
/* eslint-enable */
