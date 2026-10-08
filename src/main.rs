//! gray-dynamic-tools — user-defined shell tools, live-reloaded.
//!
//! Port of pi's `dynamic-tools` extension. Where pi registered echo tools at
//! runtime, this sidecar reads its whole live tool set from
//! `~/.gray/dynamic-tools/*.json` — one file per tool:
//!
//! ```json
//! {"name": "xc_logs", "description": "Tail the app log",
//!  "parameters": {"type": "object",
//!    "properties": {"lines": {"type": "integer"}}, "required": ["lines"]},
//!  "run": "tail -n {args.lines} ~/app.log"}
//! ```
//!
//! `plugin/tools` reports the current set (protocol 1.3), `tool/call` runs
//! the template through `sh -c` with every `{args.k}` hole replaced by the
//! JSON encoding of that argument (strings arrive quoted, so templates stay
//! injection-safe; a missing key expands to `""`). The directory's mtime is
//! fingerprinted on every frame; a change emits `host/tools_changed` so the
//! host refreshes the live set.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

fn manifest() -> Value {
    json!({
        "name": "dynamic-tools",
        "version": env!("CARGO_PKG_VERSION"),
        "protocol": "1.3",
        "tools": [],
    })
}

fn tools_dir() -> PathBuf {
    let home = std::env::var_os("GRAY_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".gray")))
        .unwrap_or_else(|| PathBuf::from("."));
    home.join("dynamic-tools")
}

/// One definition file → the tool shape the host sees, or `None` when the
/// file can't stand as a tool (missing name/description/parameters/run).
fn load_tool_file(path: &Path) -> Option<(Value, String)> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    let name = v.get("name")?.as_str()?;
    let description = v.get("description")?.as_str()?;
    let parameters = v.get("parameters")?.as_object()?;
    let run = v.get("run")?.as_str()?;
    let tool = json!({"name": name, "description": description, "parameters": parameters});
    Some((tool, run.to_string()))
}

/// Sorted (name → def) list of every valid tool file in `dir`.
fn load_defs(dir: &Path) -> Vec<(Value, String)> {
    let mut defs = vec![];
    let Ok(rd) = std::fs::read_dir(dir) else { return defs };
    let mut paths: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    paths.sort();
    for p in paths {
        if let Some(d) = load_tool_file(&p) {
            defs.push(d);
        }
    }
    defs.sort_by(|a, b| a.0["name"].as_str().cmp(&b.0["name"].as_str()));
    defs
}

/// Fingerprint of the tool dir — sorted (name, mtime, len) per `.json`
/// file — so adds, edits and deletes all register as a change.
fn dir_signature(dir: &Path) -> Vec<(String, u128, u64)> {
    let mut sig = vec![];
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_none_or(|x| x != "json") {
                continue;
            }
            if let Ok(m) = e.metadata() {
                let mtime = m
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_nanos())
                    .unwrap_or(0);
                sig.push((e.file_name().to_string_lossy().into_owned(), mtime, m.len()));
            }
        }
    }
    sig.sort();
    sig
}

/// Substitute `{args.key}` holes with `serde_json`'s encoding of the value
/// (quoted for strings — injection-safe inside `sh -c`). Unknown or
/// non-identifier holes are left untouched; a missing key becomes `""`.
fn expand(template: &str, args: &Value) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(i) = rest.find("{args.") {
        out.push_str(&rest[..i]);
        let tail = &rest[i + 6..];
        let end = tail.find('}');
        let valid = end.is_some_and(|j| {
            !tail[..j].is_empty()
                && tail[..j]
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_')
        });
        match (end, valid) {
            (Some(j), true) => {
                let key = &tail[..j];
                let v = args.get(key).cloned().unwrap_or(Value::String(String::new()));
                out.push_str(&serde_json::to_string(&v).unwrap_or_else(|_| "\"\"".into()));
                rest = &tail[j + 1..];
            }
            _ => {
                out.push_str("{args.");
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Find the `run` template for `name`, expand it, execute via `sh -c`.
fn call_tool(dir: &Path, name: &str, args: &Value) -> Result<String, String> {
    let run = load_defs(dir)
        .into_iter()
        .find(|(t, _)| t["name"].as_str() == Some(name))
        .map(|(_, r)| r);
    let Some(template) = run else {
        return Err(format!("unknown tool: {name}"));
    };
    let script = expand(&template, args);
    match std::process::Command::new("sh")
        .args(["-c", &script])
        .stdin(std::process::Stdio::null())
        .output()
    {
        Err(e) => Err(format!("couldn't spawn sh: {e}")),
        Ok(o) if o.status.success() => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            let stderr = String::from_utf8_lossy(&o.stderr);
            let body = if stdout.trim().is_empty() { stderr } else { stdout };
            Ok(if body.is_empty() {
                "(no output)".into()
            } else {
                body.into_owned()
            })
        }
        Ok(o) => Err(format!(
            "exit {}\n{}",
            o.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&o.stderr)
        )),
    }
}

/// One request → `Some(reply)`, or `None` for notifications and for replies
/// to our own `host/*` requests (id but no method). The bool asks the loop
/// to exit after writing the reply.
fn handle(req: &Value, dir: &Path) -> (Option<Value>, bool) {
    let id = req.get("id").cloned();
    let method = req.get("method").and_then(Value::as_str).unwrap_or("");
    let params = req.get("params").cloned().unwrap_or(Value::Null);
    let Some(id) = id else {
        return (None, method == "plugin/shutdown");
    };
    if method.is_empty() {
        return (None, false);
    }
    let result = match method {
        "plugin/manifest" => manifest(),
        "plugin/tools" => {
            let tools: Vec<Value> = load_defs(dir).into_iter().map(|(t, _)| t).collect();
            json!({ "tools": tools })
        }
        "tool/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params.get("args").cloned().unwrap_or_else(|| json!({}));
            match call_tool(dir, name, &args) {
                Ok(text) => json!({ "content": text }),
                Err(text) => json!({ "content": text, "is_error": true }),
            }
        }
        "plugin/shutdown" => return (Some(json!({ "id": id, "result": {} })), true),
        _ => {
            let error = json!({ "code": -32601, "message": "method not found" });
            return (Some(json!({ "id": id, "error": error })), false);
        }
    };
    (Some(json!({ "id": id, "result": result })), false)
}

fn main() -> std::io::Result<()> {
    if std::env::args().nth(1).as_deref() == Some("manifest") {
        println!("{}", manifest());
        return Ok(());
    }
    let dir = tools_dir();
    let mut sig: Option<Vec<(String, u128, u64)>> = None;
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        // Re-fingerprint on every frame; a delta means the live set changed.
        let now = dir_signature(&dir);
        if let Some(prev) = &sig
            && *prev != now
        {
            writeln!(
                stdout,
                "{}",
                json!({"method": "host/tools_changed", "params": {}})
            )?;
            stdout.flush()?;
        }
        sig = Some(now);
        let Ok(req) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let (reply, exit) = handle(&req, &dir);
        if let Some(reply) = reply {
            writeln!(stdout, "{reply}")?;
            stdout.flush()?;
        }
        if exit {
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(dir: &Path, method: &str, params: Value) -> Value {
        handle(&json!({ "id": 1, "method": method, "params": params }), dir)
            .0
            .unwrap()
    }

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("gray-dyn-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn manifest_is_protocol_13_with_empty_hint() {
        let m = manifest();
        assert_eq!(m["protocol"], "1.3");
        assert_eq!(m["tools"], json!([]));
    }

    #[test]
    fn expand_json_escapes_holes() {
        let args = json!({"msg": "a b\"c", "n": 3, "flag": true});
        let out = expand("echo {args.msg} --n {args.n} {args.flag} {args.missing} {args.", &args);
        assert_eq!(out, "echo \"a b\\\"c\" --n 3 true \"\" {args.");
    }

    #[test]
    fn plugin_tools_reads_live_set() {
        let dir = tmpdir("tools");
        std::fs::write(
            dir.join("hi.json"),
            r#"{"name":"hi","description":"say hi","parameters":{"type":"object","properties":{}},"run":"echo hi"}"#,
        )
        .unwrap();
        std::fs::write(dir.join("bad.json"), "not json").unwrap();
        std::fs::write(dir.join("note.txt"), "{}").unwrap();
        let r = call(&dir, "plugin/tools", Value::Null);
        let tools = r["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["name"], "hi");
        assert!(tools[0].get("run").is_none());
    }

    #[test]
    fn tool_call_runs_template() {
        let dir = tmpdir("call");
        std::fs::write(
            dir.join("greet.json"),
            r#"{"name":"greet","description":"g","parameters":{"type":"object","properties":{"who":{"type":"string"}}},"run":"printf 'hello %s' {args.who}"}"#,
        )
        .unwrap();
        let r = call(&dir, "tool/call", json!({"name": "greet", "args": {"who": "world"}}));
        assert_eq!(r["result"]["content"], "hello world");
        assert!(r["result"].get("is_error").is_none());
    }

    #[test]
    fn tool_call_failure_and_unknown() {
        let dir = tmpdir("fail");
        std::fs::write(
            dir.join("boom.json"),
            r#"{"name":"boom","description":"b","parameters":{"type":"object","properties":{}},"run":"echo oops >&2; exit 3"}"#,
        )
        .unwrap();
        let r = call(&dir, "tool/call", json!({"name": "boom", "args": {}}));
        assert_eq!(r["result"]["is_error"], true);
        assert!(r["result"]["content"].as_str().unwrap().contains("oops"));
        let r = call(&dir, "tool/call", json!({"name": "nope", "args": {}}));
        assert_eq!(r["result"]["is_error"], true);
    }

    #[test]
    fn signature_tracks_changes() {
        let dir = tmpdir("sig");
        let a = dir_signature(&dir);
        std::fs::write(dir.join("x.json"), "{}").unwrap();
        let b = dir_signature(&dir);
        assert_ne!(a, b);
    }
}
