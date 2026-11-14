/**
 * The page's connection to the local Fabricator server.
 *
 * Commands are POST /api/cmd/<name>. Events and terminal output arrive on one
 * WebSocket. The launch token is in the URL the server opened (or added by the
 * Vite dev proxy) and is sent on every call.
 */

export interface InvokeEvent<T> {
  payload: T
}

type Listener = (payload: unknown) => void

const listeners = new Map<string, Set<Listener>>()
const early: { event: string; payload: unknown }[] = []
const channels = new Map<string, (payload: unknown) => void>()

let socket: WebSocket | undefined
let tokenCache: string | undefined

/** The tab's label: `main`, or `app-<key>` for an installed app. */
export function currentWindowLabel(): string {
  if (typeof location === 'undefined') return 'main'
  return new URLSearchParams(location.search).get('window') || 'main'
}

function sessionToken(): string {
  if (tokenCache) return tokenCache
  if (typeof location === 'undefined') return ''
  const fromUrl = new URLSearchParams(location.search).get('token')
  if (fromUrl) {
    sessionStorage.setItem('fabricator-token', fromUrl)
    const url = new URL(location.href)
    url.searchParams.delete('token')
    history.replaceState(null, '', `${url.pathname}${url.search}${url.hash}`)
    tokenCache = fromUrl
    return fromUrl
  }
  tokenCache = sessionStorage.getItem('fabricator-token') || ''
  return tokenCache
}

function authHeaders(extra?: Record<string, string>): Record<string, string> {
  const headers: Record<string, string> = { ...extra }
  const token = sessionToken()
  if (token) headers.Authorization = `Bearer ${token}`
  const label = currentWindowLabel()
  if (label) headers['X-Fabricator-Window'] = label
  return headers
}

/** A stream of values for one command, such as a terminal's output. */
export class Channel<T> {
  readonly __channel: string
  onmessage: ((payload: T) => void) | null = null

  constructor() {
    this.__channel = crypto.randomUUID()
    channels.set(this.__channel, (payload) => {
      this.onmessage?.(payload as T)
    })
  }
}

function encode(value: unknown): unknown {
  if (value instanceof Channel) return { __channel: value.__channel }
  if (Array.isArray(value)) return value.map(encode)
  if (value && typeof value === 'object') {
    const out: Record<string, unknown> = {}
    for (const [key, item] of Object.entries(value)) out[key] = encode(item)
    return out
  }
  return value
}

function fromBase64(text: string): ArrayBuffer {
  const binary = atob(text)
  const bytes = new Uint8Array(binary.length)
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i)
  return bytes.buffer
}

function decode(data: unknown): unknown {
  if (data && typeof data === 'object' && '__bytes' in data) {
    const encoded = (data as { __bytes?: unknown }).__bytes
    if (typeof encoded === 'string') return fromBase64(encoded)
  }
  return data
}

export async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const response = await fetch(`/api/cmd/${command}`, {
    method: 'POST',
    headers: authHeaders({ 'Content-Type': 'application/json' }),
    body: JSON.stringify(encode(args ?? {}))
  })
  const body = (await response.json().catch(() => ({}))) as { ok?: boolean; data?: unknown; error?: unknown }
  if (!response.ok || body.ok === false) {
    throw typeof body.error === 'string' ? body.error : response.statusText || 'The command failed.'
  }
  return decode(body.data) as T
}

function dispatch(event: string, payload: unknown): void {
  if (event === 'channel' && payload && typeof payload === 'object') {
    const message = payload as { id?: unknown; payload?: unknown }
    if (typeof message.id === 'string') {
      channels.get(message.id)?.(message.payload)
      return
    }
  }
  if (event === 'host:pick') {
    window.dispatchEvent(new CustomEvent('fabricator-pick', { detail: payload }))
  }
  const set = listeners.get(event)
  if (!set || set.size === 0) {
    early.push({ event, payload })
    if (early.length > 100) early.shift()
    return
  }
  for (const listener of set) listener(payload)
}

export function listen<T>(name: string, cb: (event: InvokeEvent<T>) => void): Promise<() => void> {
  let set = listeners.get(name)
  if (!set) {
    set = new Set()
    listeners.set(name, set)
  }
  const listener: Listener = (payload) => cb({ payload: payload as T })
  set.add(listener)
  for (const item of early.filter((item) => item.event === name)) listener(item.payload)
  for (let i = early.length - 1; i >= 0; i--) {
    if (early[i].event === name) early.splice(i, 1)
  }
  connect()
  return Promise.resolve(() => {
    set?.delete(listener)
  })
}

/** Open the event socket. Safe to call more than once. */
export function connect(): void {
  if (socket || typeof WebSocket === 'undefined') return
  const token = sessionToken()
  const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:'
  const query = token ? `?token=${encodeURIComponent(token)}` : ''
  const next = new WebSocket(`${protocol}//${location.host}/ws${query}`)
  socket = next
  next.onmessage = (event) => {
    let message: { event?: unknown; payload?: unknown }
    try {
      message = JSON.parse(String(event.data)) as { event?: unknown; payload?: unknown }
    } catch {
      return
    }
    if (typeof message.event === 'string') dispatch(message.event, message.payload)
  }
  next.onclose = () => {
    if (socket === next) socket = undefined
    window.setTimeout(connect, 1000)
  }
}

/** Headers for a call that isn't a command, such as the folder browser. */
export function requestHeaders(extra?: Record<string, string>): Record<string, string> {
  return authHeaders(extra)
}
