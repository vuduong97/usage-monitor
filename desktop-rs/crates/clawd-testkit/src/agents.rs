//! Fakes for `clawd-agents` tests (phase 05 owns this file): the `<home>` placeholder of the
//! golden fixtures (`tools/golden/export_agents.py`) and snapshots of agent config files in a
//! temp home. Never touches the real home: every path is under the caller's temp dir.

use std::fs;
use std::path::Path;

use serde_json::Value;

/// The placeholder the exporter writes for the temp home.
pub const HOME_PLACEHOLDER: &str = "<home>";

/// `<home>` in every string of `value` -> `home` (with `/` separators).
pub fn expand_home(home: &Path, value: &Value) -> Value {
    let home = home.to_string_lossy().replace('\\', "/");
    map_strings(value, &|s| s.replace(HOME_PLACEHOLDER, &home))
}

/// The exporter's `norm`: the temp home in a string becomes `<home>`, and a string that held it
/// gets `/` separators, so Windows and POSIX runs compare equal.
pub fn normalize_home(home: &Path, value: &Value) -> Value {
    let native = home.to_string_lossy().into_owned();
    let posix = native.replace('\\', "/");
    map_strings(value, &|s| {
        let replaced = s
            .replace(&native, HOME_PLACEHOLDER)
            .replace(&posix, HOME_PLACEHOLDER);
        if replaced.contains(HOME_PLACEHOLDER) {
            replaced.replace('\\', "/")
        } else {
            replaced
        }
    })
}

fn map_strings(value: &Value, f: &dyn Fn(&str) -> String) -> Value {
    match value {
        Value::String(s) => Value::String(f(s)),
        Value::Array(a) => Value::Array(a.iter().map(|v| map_strings(v, f)).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, v)| (k.clone(), map_strings(v, f)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Write the fixture's `initial` map (`relpath -> JSON value | raw text`) under `home`.
pub fn write_initial(home: &Path, initial: &Value) {
    let Some(files) = initial.as_object() else {
        return;
    };
    for (rel, content) in files {
        let path = home.join(rel);
        fs::create_dir_all(path.parent().expect("parent")).expect("create dir");
        let text = match content {
            Value::String(s) => s.clone(),
            other => serde_json::to_string_pretty(other).expect("json"),
        };
        fs::write(path, text).expect("write initial file");
    }
}

/// What the exporter's `snapshot` records for one watched file: `null` when missing, `{"raw"}`
/// when it is not JSON, else `{"value", "backed_up", "text"?}` (`text` only when `expect_text`).
pub fn snapshot_file(home: &Path, rel: &str, expect_text: bool) -> Value {
    let path = home.join(rel);
    if !path.is_file() {
        return Value::Null;
    }
    let text = fs::read_to_string(&path).expect("read watched file");
    let Ok(parsed) = serde_json::from_str::<Value>(&text) else {
        return serde_json::json!({ "raw": text });
    };
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    let prefix = format!("{name}.bak-clawd-");
    let backed_up = fs::read_dir(path.parent().unwrap())
        .expect("read dir")
        .flatten()
        .any(|e| e.file_name().to_string_lossy().starts_with(&prefix));
    let value = normalize_home(home, &parsed);
    let mut out = serde_json::json!({ "value": value, "backed_up": backed_up });
    if expect_text {
        out["text"] = Value::String(text);
    }
    out
}
