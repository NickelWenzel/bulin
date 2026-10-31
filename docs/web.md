# Web build

```sh
trunk serve
```

## Language server

On the web, the shader editor's language features come from
[wgsl-analyzer-web](https://www.npmjs.com/package/wgsl-analyzer-web): the wgsl-analyzer server
compiled to emscripten and running in a Web Worker. `web/fetch-wgsl-analyzer.sh` runs as a trunk
`pre_build` hook. It downloads the pinned version into `web/vendor/wgsl-analyzer` and checks its
npm integrity hash. `index.html` then copies it into the output as `wgsl-analyzer/`.

- **Hosting.** The worker needs `SharedArrayBuffer`, so the page must be cross-origin isolated.
  The server has to send both of these headers:

  ```
  Cross-Origin-Opener-Policy: same-origin
  Cross-Origin-Embedder-Policy: require-corp
  ```

  `trunk serve` sends them (`Trunk.toml`). Without them the editor still works, and its status
  reads "wgsl-analyzer: unavailable".
- **Offline builds.** `BULIN_NO_ANALYZER=1 trunk build` skips the download and builds without
  the analyzer.
- **Updating the server.** Set `VERSION` and `INTEGRITY` in `web/fetch-wgsl-analyzer.sh`. The
  integrity hash is the `dist.integrity` field that `npm view wgsl-analyzer-web@<version>` prints.
  The native build links the `wgsl-analyzer` crate pinned in `Cargo.toml`, so keep the two close.

## Library

On the web, bulin keeps folders and projects in the browser, in the IndexedDB database `bulin`.
Open Folder and Open Project show this library.

- **Eviction.** bulin asks the browser to keep the library (`navigator.storage.persist()`), but a
  browser may still clear it, Safari in particular. Export what you want to keep.
- **Several tabs.** Tabs don't see each other's changes. When two tabs save the same file, the
  last write wins.
- **Export.** The open folder downloads as a zip, the open project as a `.bulin` file. Both are
  exported as last saved, without unsaved edits. A folder's zip keeps `.bulin/settings.json`, so
  importing it restores the main file, the uniforms and the clock.
- **Import.** A zip, a directory or a `.bulin` file becomes a new library entry and opens.
  - An import may be at most 64 MiB before unpacking and 64 MiB unpacked, in at most 10,000
    entries. Larger imports are refused.
  - `.git/`, `.bulin/bulin/`, `.bulin/uniforms/` and `.bulin/tmp/` are skipped, since bulin
    regenerates its own and doesn't use git's.
  - Entries whose path leaves the folder or isn't valid, files that aren't UTF-8 text, and a
    `settings.json` bulin can't read are left out, and the library lists them.
