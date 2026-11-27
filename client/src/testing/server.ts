// A stand-in for the server the page talks to: the tests say what each route answers, and read
// back what the page asked. Nothing leaves the test: a route no test answered gets a 404.

/** A request the page made. */
export interface Asked {
  method: string
  /** The path, without the query. */
  path: string
  /** The query, with its `?`, or ''. */
  search: string
  /** The body, read as JSON when it is JSON. */
  body: any
}

/** What a route answers: a whole answer, or (anything else) JSON with 200. */
export interface Answer {
  status?: number
  json?: unknown
  /** A body that is not JSON. */
  text?: string
  headers?: Record<string, string>
}

type Handler = (asked: Asked) => unknown
interface Route {
  method: string | null
  pattern: string | RegExp
  handler: Handler
}

const isAnswer = (value: unknown): value is Answer =>
  typeof value === 'object' &&
  value !== null &&
  ('status' in value || 'json' in value || 'text' in value) &&
  Object.keys(value).every((key) => ['status', 'json', 'text', 'headers'].includes(key))

export class Server {
  routes: Route[] = []
  asked: Asked[] = []
  /** Requests no route answered. */
  strays: Asked[] = []

  /**
   * Answers requests whose path is `pattern` (a string, whole) or matches it (a RegExp, on the
   * path and the query), of `method` when one is given, with `answer`: a function of the
   * request, or what it returns. The route set last is asked first.
   */
  on(pattern: string | RegExp, answer: unknown, method: string | null = null) {
    const handler = typeof answer === 'function' ? (answer as Handler) : () => answer
    this.routes.unshift({ method, pattern, handler })
    return this
  }

  /** The requests to paths that match `pattern` (of `method`, when given). */
  calls(pattern: string | RegExp, method: string | null = null) {
    return this.asked.filter(
      (asked) =>
        (method === null || asked.method === method) && matches(pattern, asked.path, asked.search),
    )
  }

  /** What the page's fetch does here. */
  fetch = async (input: RequestInfo | URL, init: RequestInit = {}) => {
    const url = new URL(String(input instanceof Request ? input.url : input), location.href)
    const method = (init.method ?? 'GET').toUpperCase()
    return this.answer(method, url, typeof init.body === 'string' ? init.body : undefined)
  }

  /** What navigator.sendBeacon does here: a POST, whose answer nobody reads. */
  beacon = (url: string, body: Blob) => {
    void body.text().then((text) => this.answer('POST', new URL(url, location.href), text))
    return true
  }

  async answer(method: string, url: URL, raw: string | undefined) {
    let body: any = raw
    try {
      body = raw === undefined ? undefined : JSON.parse(raw)
    } catch {
      /* not JSON: kept as it is */
    }
    const asked: Asked = { method, path: url.pathname, search: url.search, body }
    this.asked.push(asked)
    const route = this.routes.find(
      (one) =>
        (one.method === null || one.method === method) &&
        matches(one.pattern, url.pathname, url.search),
    )
    if (!route) {
      this.strays.push(asked)
      return respond({ status: 404, json: { error: 'no route' } })
    }
    const value = await route.handler(asked)
    return respond(isAnswer(value) ? value : { json: value })
  }
}

function matches(pattern: string | RegExp, path: string, search: string) {
  return typeof pattern === 'string' ? path === pattern : pattern.test(path + search)
}

function respond({ status = 200, json, text, headers = {} }: Answer) {
  const body = text ?? JSON.stringify(json ?? null)
  const type = text === undefined ? { 'content-type': 'application/json' } : {}
  return new Response(body, { status, headers: { ...type, ...headers } })
}

/** A server that answers nothing yet. */
export const server = () => new Server()
