<div align="center">
  <img alt="gray-dynamic-tools" src="assets/icon.svg" width="120" height="120" />
  <h1>gray-dynamic-tools</h1>
  <p><strong>Turn JSON files in ~/.gray/dynamic-tools into real model-callable tools.</strong></p>
  <p>
    <a href="https://gray.alignment.id">Website</a> ·
    <a href="https://gray.alignment.id/plugins/gray-dynamic-tools">Store</a> ·
    <a href="https://github.com/vstaln/gray-dynamic-tools">Source</a> ·
    <a href="https://github.com/vstaln/gray">gray</a>
  </p>
  <p>
    <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-1c1c20?style=flat-square&labelColor=0a0a0b" /></a>
    <a href="https://www.rust-lang.org"><img alt="Built with Rust" src="https://img.shields.io/badge/built%20with-rust-1c1c20?style=flat-square&labelColor=0a0a0b&logo=rust&logoColor=d4a373" /></a>
    <a href="https://gray.alignment.id/plugins/gray-dynamic-tools"><img alt="gray plugin" src="https://img.shields.io/badge/gray-plugin-1c1c20?style=flat-square&labelColor=0a0a0b&color=7aa2f7" /></a>
  </p>
</div>

<br/>

```bash
gray plugin install gray-dynamic-tools
```

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

## Tags

`gray` `plugin` `dynamic-tools` `rust`

---
Part of the [gray](https://github.com/vstaln/gray) plugin ecosystem —
the open-source AI agent harness. <https://gray.alignment.id>
