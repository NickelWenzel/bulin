// Bridge between src/analyzer/web.rs and wgsl-analyzer-web, the wgsl-analyzer language server
// compiled to emscripten and running in a Web Worker. Trunk copies the package to
// `wgsl-analyzer/` next to index.html (see web/fetch-wgsl-analyzer.sh).
//
// Callbacks into Rust always run from a fresh task or microtask, never inside a call from Rust,
// so the Rust side never holds a borrow while one runs.

// A worker whose pthreads COEP blocks never finishes starting and reports nothing.
const START_TIMEOUT_MS = 30_000

let server = null
// Bumped by every start, stop and failure, so a callback from an older run is ignored.
let generation = 0
// Messages sent before the server is up.
let queue = []

export function analyzer_start(root, filesJson, onMessage, onFailed) {
  const mine = ++generation
  queue = []

  const fail = (reason) =>
    queueMicrotask(() => {
      if (mine !== generation) return
      generation++
      server?.dispose()
      server = null
      queue = []
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
      for (const message of queue) server.sendMessage(message)
      queue = []
    })
    .catch((error) => {
      clearTimeout(timeout)
      fail(`it failed to load: ${error?.message ?? error}`)
    })
}

export function analyzer_send(json) {
  const message = JSON.parse(json)
  if (server) server.sendMessage(message)
  else queue.push(message)
}

export function analyzer_stop() {
  generation++
  server?.dispose()
  server = null
  queue = []
}
