import { useEffect, useId, useRef, useState, type JSX } from 'react'
import { requestHeaders } from '../bridge'

interface PickRequest {
  id: string
  mode: 'file' | 'files' | 'folder' | 'folders' | 'save' | 'paths'
  title?: string
  fileName?: string
  filters?: { name: string; extensions: string[] }[]
}

interface Entry {
  name: string
  path: string
  dir: boolean
}

interface Listing {
  path: string
  parent: string | null
  entries: Entry[]
}

async function list(path: string): Promise<Listing> {
  const query = path ? `?path=${encodeURIComponent(path)}` : ''
  const response = await fetch(`/api/fs/list${query}`, { headers: requestHeaders() })
  const body = (await response.json()) as Listing & { error?: string }
  if (!response.ok) throw new Error(body.error || 'Couldn’t open that folder.')
  return body
}

async function resolve(id: string, paths: string[] | null): Promise<void> {
  await fetch(`/api/pick/${encodeURIComponent(id)}`, {
    method: 'POST',
    headers: requestHeaders({ 'Content-Type': 'application/json' }),
    body: JSON.stringify(paths ? { paths } : { cancel: true })
  })
}

/**
 * The folder browser the server asks for when a command used to open a system
 * file dialog. It only lists folders this computer's user can already read.
 */
export default function PathPicker(): JSX.Element | null {
  const titleId = useId()
  const [request, setRequest] = useState<PickRequest | null>(null)
  const [listing, setListing] = useState<Listing | null>(null)
  const [selected, setSelected] = useState<string[]>([])
  const [fileName, setFileName] = useState('')
  const [error, setError] = useState<string | null>(null)
  const dialogRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const onPick = (event: Event): void => {
      const detail = (event as CustomEvent<PickRequest>).detail
      setRequest(detail)
      setSelected([])
      setFileName(detail.fileName || '')
      setError(null)
      setListing(null)
      void list('').then(setListing).catch((reason: unknown) => setError(reason instanceof Error ? reason.message : 'Couldn’t list folders.'))
    }
    window.addEventListener('fabricator-pick', onPick)
    return () => window.removeEventListener('fabricator-pick', onPick)
  }, [])

  useEffect(() => {
    if (request) dialogRef.current?.focus()
  }, [request])

  if (!request || !listing) return request ? <p className="path-picker-loading">Opening folders…</p> : null

  const folder = listing
  const many = request.mode === 'files' || request.mode === 'folders' || request.mode === 'paths'
  const title = request.title || 'Choose'

  async function open(path: string): Promise<void> {
    setError(null)
    try {
      setListing(await list(path))
      setSelected([])
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : 'Couldn’t open that folder.')
    }
  }

  function toggle(entry: Entry): void {
    if (entry.dir && (request?.mode === 'file' || request?.mode === 'files' || request?.mode === 'save')) {
      void open(entry.path)
      return
    }
    if (!many) {
      setSelected([entry.path])
      return
    }
    setSelected((current) => (current.includes(entry.path) ? current.filter((path) => path !== entry.path) : [...current, entry.path]))
  }

  function choose(): void {
    if (!request) return
    if (request.mode === 'save') {
      const name = fileName.trim()
      if (!name || !folder.path) return
      const sep = folder.path.includes('\\') ? '\\' : '/'
      void resolve(request.id, [`${folder.path.replace(/[\\/]$/, '')}${sep}${name}`])
    } else if (request.mode === 'folder' || request.mode === 'folders') {
      const paths = selected.length > 0 ? selected : folder.path ? [folder.path] : []
      if (paths.length === 0) return
      void resolve(request.id, paths)
    } else if (selected.length === 0) {
      return
    } else {
      void resolve(request.id, selected)
    }
    setRequest(null)
  }

  function cancel(): void {
    if (request) void resolve(request.id, null)
    setRequest(null)
  }

  return (
    <div className="path-picker-backdrop">
      <div
        className="path-picker"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        ref={dialogRef}
        tabIndex={-1}
        onKeyDown={(event) => {
          if (event.key === 'Escape') cancel()
        }}
      >
        <h2 id={titleId}>{title}</h2>
        <p className="path-picker-path">{folder.path || 'This computer'}</p>
        {error && (
          <p className="path-picker-error" role="alert">
            {error}
          </p>
        )}
        <ul className="path-picker-list">
          {folder.parent !== null && folder.path !== '' && (
            <li>
              <button type="button" onClick={() => void open(folder.parent || '')}>
                ..
              </button>
            </li>
          )}
          {folder.entries.map((entry) => (
            <li key={entry.path}>
              <button
                type="button"
                aria-pressed={selected.includes(entry.path)}
                onClick={() => toggle(entry)}
                onDoubleClick={() => {
                  if (entry.dir) void open(entry.path)
                  else {
                    setSelected([entry.path])
                  }
                }}
              >
                {entry.dir ? 'Folder' : 'File'}: {entry.name}
              </button>
            </li>
          ))}
        </ul>
        {request.mode === 'save' && (
          <label className="path-picker-name">
            File name
            <input value={fileName} onChange={(event) => setFileName(event.target.value)} />
          </label>
        )}
        <div className="path-picker-actions">
          <button type="button" className="btn" onClick={cancel}>
            Cancel
          </button>
          <button type="button" className="btn btn--primary" onClick={choose}>
            {request.mode === 'save' ? 'Save' : 'Choose'}
          </button>
        </div>
      </div>
    </div>
  )
}
