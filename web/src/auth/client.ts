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

/* The bytes as they are, with no json wrapper: the server reads what an image is from the first
   of them, so an envelope would only be something to undo at both ends. The browser sets the
   content type from the file, and the server believes none of it. */
export const offer = (path: string, body: Blob) => send(path, { method: 'POST', body })
export const patch = carrying('PATCH')
export const put = carrying('PUT')
export const del = carrying('DELETE')

/* The server serves this application from the same port and answers any path it does not know
   with index.html, so a request for data that reaches a build older than the application comes
   back as a page with a 200 on it. That is the one failure the documented development loop
   invites — the dev server beside a container nobody rebuilt — and it used to surface as the
   screen saying it could not read something, which sent us looking at the wrong request. */
export async function json<T>(response: Response): Promise<T> {
  if (!response.headers.get('content-type')?.includes('json')) {
    throw new ApiError(response.status, 'the server answered a page rather than data, so it is older than this application and does not serve this path yet')
  }
  return (await response.json()) as T
}
