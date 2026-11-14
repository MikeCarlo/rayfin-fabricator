#!/usr/bin/env node
// Checks the renderer bundle in dist/ for what the packaged app relies on but unit tests
// can't see (they mock Monaco and never load index.html). Run after `npm run build:renderer`.
//   - Assets load through relative ./ URLs: Tauri serves dist/ from a custom protocol.
//   - index.html carries the Content-Security-Policy, and it allows no remote origins.
//     tauri.conf.json sets `csp: null`, so this tag is the app's only CSP. The app's own
//     URI schemes aren't remote: the webview hands their requests to the app, which on
//     Windows registers each as http://<scheme>.localhost.
//   - Every Monaco web worker is bundled, so the editors work offline.
//   - pdf.js's worker is bundled, so a report migration can draw its pages offline.

import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const DIST = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', 'dist')
const WORKERS = ['editor', 'json', 'css', 'html', 'ts']
// The preview's stills (src-tauri/src/services/preview.rs, STILL_SCHEME).
const OWN_SCHEME_ORIGINS = ['http://preview-still.localhost']

const indexHtml = path.join(DIST, 'index.html')
if (!fs.existsSync(indexHtml)) {
  console.error('dist/index.html not found. Run `npm run build:renderer` first.')
  process.exit(1)
}

const problems = []
const html = fs.readFileSync(indexHtml, 'utf8')
for (const [, , url] of html.matchAll(/<(?:script|link)\b[^>]*?\b(src|href)="([^"]*)"/g)) {
  if (!url.startsWith('./'))
    problems.push(`index.html loads ${url}; asset URLs must be relative (./)`)
}

const csp = html.match(/http-equiv="Content-Security-Policy"\s+content="([^"]*)"/)?.[1]
if (!csp) problems.push('index.html has no Content-Security-Policy meta tag')
else {
  // The preview is an iframe of a local app or a published https app. Scripts
  // still have to come from this page. Loopback and https are allowed only on
  // the directives that load the preview.
  const allowedRemote = new Set([
    'http://127.0.0.1:*',
    'http://localhost:*',
    'ws://127.0.0.1:*',
    'ws://localhost:*',
    'https:',
    ...OWN_SCHEME_ORIGINS
  ])
  const directives = Object.fromEntries(
    csp
      .split(';')
      .map((part) => part.trim())
      .filter(Boolean)
      .map((part) => {
        const [name, ...sources] = part.split(/\s+/)
        return [name, sources]
      })
  )
  for (const source of directives['script-src'] ?? []) {
    if (/https?:|\*/.test(source)) problems.push(`script-src allows a remote origin: ${source}`)
  }
  for (const [name, sources] of Object.entries(directives)) {
    if (name === 'script-src') continue
    for (const source of sources) {
      if (/https?:|\*/.test(source) && !allowedRemote.has(source)) {
        problems.push(`${name} allows ${source}`)
      }
    }
  }
}

const assets = fs.readdirSync(path.join(DIST, 'assets'))
for (const worker of WORKERS) {
  if (!assets.some((name) => name.startsWith(`${worker}.worker-`) && name.endsWith('.js'))) {
    problems.push(
      `dist/assets has no ${worker}.worker-*.js, so Monaco's ${worker} worker isn't bundled`
    )
  }
}
if (!assets.some((name) => name.startsWith('pdf.worker.min-') && name.endsWith('.mjs'))) {
  problems.push("dist/assets has no pdf.worker.min-*.mjs, so pdf.js's worker isn't bundled")
}

if (problems.length) {
  console.error(`Renderer build check failed:\n  ${problems.join('\n  ')}`)
  process.exit(1)
}
console.log(
  `Renderer build check passed: relative assets, CSP, ${WORKERS.length} Monaco workers, the pdf.js worker.`
)
