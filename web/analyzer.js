// Bridge between src/analyzer/web.rs and wgsl-analyzer-web, the wgsl-analyzer language server
// compiled to emscripten and running in a Web Worker. Trunk copies the package to
// `wgsl-analyzer/` next to index.html (see web/fetch-wgsl-analyzer.sh).
//
// Callbacks into Rust always run from a fresh task or microtask, never inside a call from Rust,
// so the Rust side never holds a borrow while one runs.
//
// One server runs at a time, numbered by a generation. Each Rust link takes a generation when it is
// created and may start its server later, once its files are read, so starting and stopping name
// the generation they mean, and a call for an older one does nothing.

// A worker whose pthreads COEP blocks never finishes starting and reports nothing.
const START_TIMEOUT_MS = 30_000

let server = null
// The current generation. Bumped by every begin, stop and failure, so a callback from an older
// run is ignored.
let generation = 0
// What the current generation sent before its server is up, in order: LSP messages and file
// writes and deletions, which must reach the server in the order they were made.
let queue = []

function reset() {
  server?.dispose()
  server = null
  queue = []
}

export function analyzer_begin() {
  reset()
  return ++generation
}

export function analyzer_start(mine, root, filesJson, onMessage, onFailed) {
  if (mine !== generation) return

  const fail = (reason) =>
    queueMicrotask(() => {
      if (mine !== generation) return
      generation++
      reset()
      onFailed(reason)
    })

  if (!globalThis.crossOriginIsolated) {
    fail('the page is not cross-origin isolated (it needs COOP/COEP headers)')
    return
  }

  const timeout = setTimeout(
    () => fail(`it did not start within ${START_TIMEOUT_MS / 1000} s`),
    START_TIMEOUT_MS,
  )
  // Resolved against the page, so it works under any public URL.
  const url = new URL('wgsl-analyzer/index.js', document.baseURI).href
  import(url)
    .then(({ WgslAnalyzerServer }) =>
      WgslAnalyzerServer.start({
        root,
        files: JSON.parse(filesJson),
        onStderr: (line) => console.debug('[wgsl-analyzer]', line),
        onExit: (code) => fail(`it exited with code ${code}`),
      }),
    )
    .then((started) => {
      clearTimeout(timeout)
      if (mine !== generation) {
        started.dispose()
        return
      }
      server = started
      server.onMessage((message) => {
        if (mine === generation) onMessage(JSON.stringify(message))
      })
      for (const item of queue) deliver(item)
      queue = []
    })
    .catch((error) => {
      clearTimeout(timeout)
      fail(`it failed to load: ${error?.message ?? error}`)
    })
}

function deliver(item) {
  switch (item.type) {
    case 'lsp':
      server.sendMessage(item.message)
      break
    case 'write':
      server.writeFile(item.path, item.text)
      break
    case 'delete':
      server.deleteFile(item.path)
      break
  }
}

function send(item) {
  if (server) deliver(item)
  else queue.push(item)
}

export function analyzer_send(json) {
  send({ type: 'lsp', message: JSON.parse(json) })
}

// `path` is relative to the workspace root.
export function analyzer_write_file(path, text) {
  send({ type: 'write', path, text })
}

export function analyzer_delete_file(path) {
  send({ type: 'delete', path })
}

export function analyzer_stop(mine) {
  if (mine !== generation) return
  generation++
  reset()
}
