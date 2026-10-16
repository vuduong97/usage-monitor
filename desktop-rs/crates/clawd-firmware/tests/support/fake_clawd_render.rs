//! Test double for `clawd_render` (see Cargo.toml). Behaviour comes from the `mode` file in the
//! folder the program lives in. Every mode first logs `args` and `stdin` next to it.
use std::io::{Read, Write};

const FRAME_BYTES: usize = 240 * 240 * 3;

fn main() {
    let dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
        .unwrap_or_default();
    let mut stdin = Vec::new();
    let _ = std::io::stdin().read_to_end(&mut stdin);
    let args: Vec<String> = std::env::args().skip(1).collect();
    let log = |name: &str, data: &[u8]| {
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join(name))
            .and_then(|mut f| f.write_all(data));
    };
    log("args.log", format!("{}\n", args.join(" ")).as_bytes());
    log("stdin.log", &[&stdin[..], b"\n"].concat());
    let mode = std::fs::read_to_string(dir.join("mode")).unwrap_or_default();
    let out = std::io::stdout();
    let mut out = out.lock();
    match mode.trim() {
        "exit1" => std::process::exit(1),
        "short" => {
            let _ = out.write_all(&[0; 100]);
        }
        "long" => {
            let _ = out.write_all(&vec![0; FRAME_BYTES + 5]);
        }
        "nope" => {
            let _ = out.write_all(b"nope");
        }
        "reject" => {
            // the clock ignores a payload: exit 2, but draws the waiting screen without one
            if stdin.is_empty() {
                let _ = out.write_all(&vec![9; FRAME_BYTES]);
            } else {
                std::process::exit(2);
            }
        }
        "hang" => std::thread::sleep(std::time::Duration::from_secs(60)),
        "clip" => {
            let _ = out.write_all(&std::fs::read(dir.join("clip.bin")).unwrap_or_default());
        }
        _ => {
            // a frame whose every byte says how long the payload was
            let _ = out.write_all(&vec![(stdin.len() % 256) as u8; FRAME_BYTES]);
        }
    }
}
