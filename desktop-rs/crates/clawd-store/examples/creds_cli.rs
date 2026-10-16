//! Manual-test helper for `tools/golden/creds_roundtrip.py`: drives `clawd_store::creds` against
//! the real OS keyring under a throwaway service name.
//!
//! `creds_cli <service> get|put|forget <key> [user] [password]` prints the result as one JSON
//! line (`get`: `["user","password"]` or `null`; `put`: `true`/`false`). It refuses the
//! production service name, so it can never read or change the real Clawd credentials.

use clawd_contracts::{OsStoreNames, KEYRING_SERVICE};
use clawd_store::creds;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [service, action, key, rest @ ..] = args.as_slice() else {
        eprintln!("usage: creds_cli <service> get|put|forget <key> [user] [password]");
        std::process::exit(2);
    };
    if service == KEYRING_SERVICE || !service.starts_with(&format!("{KEYRING_SERVICE}.test-")) {
        eprintln!("refusing service {service:?}: use {KEYRING_SERVICE}.test-<unique>");
        std::process::exit(2);
    }
    let names = OsStoreNames {
        keyring_service: service.clone(),
        ..OsStoreNames::for_test("creds-cli")
    };
    match (action.as_str(), rest) {
        ("get", _) => match creds::get(&names, key) {
            Some((user, password)) => println!("{}", serde_json::json!([user, password])),
            None => println!("null"),
        },
        ("put", [user, password]) => println!("{}", creds::put(&names, key, user, password)),
        ("forget", _) => {
            creds::forget(&names, key);
            println!("true");
        }
        _ => {
            eprintln!("bad arguments");
            std::process::exit(2);
        }
    }
}
