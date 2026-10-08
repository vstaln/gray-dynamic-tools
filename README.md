<p align="center">
  <img src="assets/gray-logo.svg" alt="gray" width="96">
</p>
<h1 align="center">gray-dynamic-tools</h1>
<p align="center">User-defined shell tools as JSON files, live-reloaded into the model's toolset.</p>
<p align="center">
  <a href="https://github.com/vstaln/gray-dynamic-tools/blob/main/LICENSE"><img alt="MIT License" src="https://img.shields.io/badge/license-MIT-blue.svg"></a>
  <img alt="gray plugin" src="https://img.shields.io/badge/gray-plugin-7aa2f7.svg">
  <img alt="rust" src="https://img.shields.io/badge/built%20with-rust-orange.svg">
</p>

A protocol-1.3 sidecar that turns `~/.gray/dynamic-tools/*.json` into real
model-callable tools.

A sidecar plugin for [gray](https://github.com/vstaln/gray), scaffolded by
[gray-account](https://github.com/vstaln/gray-account).

## Tool definitions

One `.json` file per tool under `~/.gray/dynamic-tools/`:

```json
{
  "name": "xc_logs",
  "description": "Tail the app log",
  "parameters": {
    "type": "object",
    "properties": { "lines": { "type": "integer", "description": "Line count" } },
    "required": ["lines"]
  },
  "run": "tail -n {args.lines} ~/app.log"
}
```

- `name`, `description`, `parameters` are shown to the model verbatim.
- `run` is executed through `sh -c` when the model calls the tool. Each
  `{args.key}` hole is replaced by the JSON encoding of that argument —
  strings arrive quoted (`"hello world"`), so templates are injection-safe.
  A missing key expands to `""`.
- Non-zero exit → tool error carrying stderr. Success → stdout (or stderr
  when stdout is empty).

Add, edit, or delete a file and the sidecar emits `host/tools_changed`; the
host rebuilds the live tool set at the start of the next turn. No restart
needed. Files that fail to parse or lack a field are skipped.

## Install

```sh
gray plugin install dynamic-tools
```

## Wire methods

- `plugin/manifest` — `protocol: "1.3"`, empty `tools` hint
- `plugin/tools` — live set read fresh from disk
- `tool/call` — runs the `run` template via `sh -c`
- `host/tools_changed` — emitted when the dir fingerprint changes
- `plugin/shutdown`

No `host/*` capabilities required.

## Develop

```sh
cargo test
gray account check      # entry point + manifest handshake
gray account publish    # check → build → release → publish to the gray registry
```

Bump `version` in `Cargo.toml` before each `publish`; the registry refuses to
republish a version.

---
Part of the [gray](https://github.com/vstaln/gray) plugin ecosystem —
the open-source AI agent harness. <https://gray.alignment.id>
