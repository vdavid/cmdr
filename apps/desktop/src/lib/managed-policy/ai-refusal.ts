/**
 * The one copy map for `ManagedAiRefusal`: every surface that says "your organization said no" to
 * AI (the provider control, the service picker, the connection check, the consent switch, the
 * translate toast) words it here, so a new surface can't spell the reason differently. The backend
 * decides the refusal; this only names it.
 */

import type { ManagedAiRefusal } from '$lib/ipc/bindings'
import { tString } from '$lib/intl/messages.svelte'

export function managedAiRefusalMessage(refusal: ManagedAiRefusal): string {
  switch (refusal) {
    case 'aiOff':
      return tString('ai.managed.aiOff')
    case 'cloudAiOff':
      return tString('ai.managed.cloudAiOff')
    case 'hostNotAllowed':
      return tString('ai.managed.hostNotAllowed')
  }
}
