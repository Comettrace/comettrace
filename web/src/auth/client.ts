export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly body: string,
  ) {
    super(`the server answered ${status}`)
    this.name = 'ApiError'
  }
}

export const isUnauthorized = (error: unknown) => error instanceof ApiError && error.status === 401

/* The server answers in plain text when it refuses something a form asked for, so that is what a
   screen should show rather than a sentence of its own invention. */
export const because = (error: unknown, otherwise: string) =>
  error instanceof ApiError && error.body.length > 0 ? error.body : otherwise

/* Every /auth route answers 403 without this header, which is what keeps a form on another site
   from posting here with the session cookie attached. */
const REQUESTED_WITH = { 'x-requested-with': 'comettrace-web' }

async function send(path: string, init: RequestInit): Promise<Response> {
  const response = await fetch(path, { credentials: 'include', ...init, headers: { ...REQUESTED_WITH, ...init.headers } })
  if (!response.ok) throw new ApiError(response.status, (await response.text()).trim())
  return response
}

export const get = (path: string) => send(path, { method: 'GET' })

const carrying = (method: string) => (path: string, body?: unknown) =>
  send(path, {
    method,
    headers: body === undefined ? {} : { 'content-type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  })

export const post = carrying('POST')
export const patch = carrying('PATCH')
export const del = carrying('DELETE')

export const json = async <T>(response: Response): Promise<T> => (await response.json()) as T
