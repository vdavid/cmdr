import type { HandleServerError } from '@sveltejs/kit'

export const handleError: HandleServerError = ({ error, event }) => {
  const hasAccess = event.request.headers.get('cf-access-authenticated-user-email')
  if (hasAccess) {
    const err = error instanceof Error ? `${error.message}\n${error.stack}` : String(error)
    return { message: err }
  }
  return { message: 'Internal Error' }
}
