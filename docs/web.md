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
