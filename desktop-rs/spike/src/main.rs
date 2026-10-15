// Phase 01 spike: tray-only Tauri app, lazy settings window (Design A), loopback event
// server, reqwest/rustls client. Throwaway: it exists to be measured by tools/perf.
//
// Environment (all optional):
//   CLAWD_SPIKE_LOG      file that receives "<unix ms> <event>" lines (tray_ready, shown, ...)
//   CLAWD_SPIKE_PORT     loopback port of the event server (default 47931, never the real 47913)
//   CLAWD_SPIKE_CONTROL  1: /open /close /trim /status /fetch routes, so a script can drive the
//                        same open_settings() the tray click calls
//   CLAWD_SPIKE_TRIM     1: SetProcessWorkingSetSize(-1,-1) after every window is destroyed
//   CLAWD_SPIKE_ARGS     WebView2 browser arguments to use instead of Tauri's defaults (experiments)
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::convert::Infallible;
use std::fs::File;
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use http_body_util::{BodyExt, Full};
use hyper::body::{Bytes, Incoming};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::window::Color;
use tauri::{AppHandle, Manager, RunEvent, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};
use tokio::net::TcpListener;

const SETTINGS_LABEL: &str = "settings";
const DEFAULT_PORT: u16 = 47931;

// Runs before the page's own scripts. The page itself (ui/index.html) stays untouched.
// Once DOMContentLoaded has fired and two animation frames have been produced, the first
// frame is on its way to the compositor: tell Rust, which then shows the window.
const PERF_JS: &str = r#"
(function () {
  function afterPaint() {
    requestAnimationFrame(function () {
      requestAnimationFrame(function () {
        var fcp = performance.getEntriesByName('first-contentful-paint')[0];
        window.__TAURI_INTERNALS__.invoke('spike_ready', {
          dcl: performance.now(),
          fcp: fcp ? fcp.startTime : -1
        });
      });
    });
  }
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', afterPaint, { once: true });
  } else {
    afterPaint();
  }
})();
"#;

static LOG: OnceLock<Mutex<File>> = OnceLock::new();

fn now_ms() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)
}

fn mark(event: &str) {
    if let Some(file) = LOG.get() {
        if let Ok(mut file) = file.lock() {
            let _ = writeln!(file, "{} {}", now_ms(), event);
        }
    }
}

fn env_flag(name: &str) -> bool {
    std::env::var(name).map(|v| v == "1").unwrap_or(false)
}

#[cfg(windows)]
fn trim_working_set() {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> isize;
        fn SetProcessWorkingSetSize(process: isize, min: usize, max: usize) -> i32;
    }
    // (SIZE_T)-1 for both limits tells Windows to trim the working set now.
    let ok = unsafe { SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX) };
    mark(&format!("trimmed ok={ok}"));
}

#[cfg(not(windows))]
fn trim_working_set() {
    mark("trim unsupported on this platform");
}

#[tauri::command]
fn spike_ready(window: WebviewWindow, dcl: f64, fcp: f64) {
    mark(&format!("ready dcl={dcl:.1} fcp={fcp:.1}"));
    let _ = window.show();
    mark("shown");
    let _ = window.set_focus();
}

/// What the tray click (and the menu item) does. Called from an async context: building a
/// window from a synchronous event handler deadlocks on Windows.
async fn open_settings(app: AppHandle) {
    mark("open_requested");
    if let Some(window) = app.get_webview_window(SETTINGS_LABEL) {
        let _ = window.show();
        let _ = window.set_focus();
        mark("reused_existing_window");
        return;
    }
    let builder = WebviewWindowBuilder::new(&app, SETTINGS_LABEL, WebviewUrl::App("index.html".into()))
        .title("Clawd · Cài đặt")
        .inner_size(820.0, 600.0)
        .min_inner_size(720.0, 540.0)
        .visible(false)
        .background_color(Color(20, 20, 19, 255))
        .initialization_script(PERF_JS);
    // Experiment knob: replaces Tauri's default WebView2 browser arguments.
    #[cfg(windows)]
    let builder = match std::env::var("CLAWD_SPIKE_ARGS") {
        Ok(args) => builder.additional_browser_args(&args),
        Err(_) => builder,
    };
    let built = builder.build();
    match built {
        Ok(_) => mark("window_built"),
        Err(e) => mark(&format!("build_error {e}")),
    }
}

fn spawn_open_settings(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(open_settings(app));
}

type Reply = Response<Full<Bytes>>;

fn reply(status: StatusCode, body: &'static str) -> Reply {
    let mut response = Response::new(Full::new(Bytes::from_static(body.as_bytes())));
    *response.status_mut() = status;
    response
}

async fn route(
    request: Request<Incoming>,
    app: AppHandle,
    client: reqwest::Client,
    control: bool,
) -> Result<Reply, Infallible> {
    let path = request.uri().path().to_owned();
    let method = request.method().clone();
    let response = match (&method, path.as_str()) {
        (&Method::GET, "/state") => reply(StatusCode::OK, r#"{"state":"idle"}"#),
        (&Method::POST, "/poke") => reply(StatusCode::NO_CONTENT, ""),
        (&Method::POST, p) | (&Method::GET, p) if p.starts_with("/hook/") => {
            // Drain the body like the real server will.
            let _ = request.into_body().collect().await;
            reply(StatusCode::NO_CONTENT, "")
        }
        (&Method::GET, "/open") if control => {
            spawn_open_settings(&app);
            reply(StatusCode::ACCEPTED, "opening")
        }
        (&Method::GET, "/close") if control => {
            match app.get_webview_window(SETTINGS_LABEL) {
                Some(window) => {
                    mark("close_requested");
                    let _ = window.close();
                    reply(StatusCode::ACCEPTED, "closing")
                }
                None => reply(StatusCode::NOT_FOUND, "no window"),
            }
        }
        (&Method::GET, "/status") if control => {
            if app.get_webview_window(SETTINGS_LABEL).is_some() {
                reply(StatusCode::OK, r#"{"window":true}"#)
            } else {
                reply(StatusCode::OK, r#"{"window":false}"#)
            }
        }
        (&Method::GET, "/trim") if control => {
            trim_working_set();
            reply(StatusCode::OK, "trimmed")
        }
        (&Method::GET, "/fetch") if control => {
            let url = request
                .uri()
                .query()
                .and_then(|q| q.strip_prefix("url="))
                .unwrap_or("https://example.com")
                .to_owned();
            match client.get(url).send().await {
                Ok(r) if r.status().is_success() => reply(StatusCode::OK, "fetched"),
                _ => reply(StatusCode::BAD_GATEWAY, "fetch failed"),
            }
        }
        _ => reply(StatusCode::NOT_FOUND, "not found"),
    };
    Ok(response)
}

async fn serve(app: AppHandle, client: reqwest::Client, port: u16, control: bool) {
    let listener = match TcpListener::bind(("127.0.0.1", port)).await {
        Ok(listener) => listener,
        Err(e) => {
            mark(&format!("server_bind_error {e}"));
            return;
        }
    };
    mark(&format!("server_ready port={port}"));
    loop {
        let Ok((stream, _)) = listener.accept().await else { continue };
        let app = app.clone();
        let client = client.clone();
        tokio::spawn(async move {
            let service = service_fn(move |request| route(request, app.clone(), client.clone(), control));
            let _ = http1::Builder::new().serve_connection(TokioIo::new(stream), service).await;
        });
    }
}

fn main() {
    if let Ok(path) = std::env::var("CLAWD_SPIKE_LOG") {
        if let Ok(file) = File::create(path) {
            let _ = LOG.set(Mutex::new(file));
        }
    }
    mark("process_start");

    #[cfg(feature = "tls-ring")]
    {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .expect("reqwest client");

    let port = std::env::var("CLAWD_SPIKE_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(DEFAULT_PORT);
    let control = env_flag("CLAWD_SPIKE_CONTROL");
    let trim_after_destroy = env_flag("CLAWD_SPIKE_TRIM");

    let app = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![spike_ready])
        .setup(move |app| {
            let settings = MenuItem::with_id(app, "settings", "Cài đặt…", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Thoát", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&settings, &quit])?;
            TrayIconBuilder::new()
                .icon(app.default_window_icon().expect("window icon").clone())
                .tooltip("Clawd")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "settings" => spawn_open_settings(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        spawn_open_settings(tray.app_handle());
                    }
                })
                .build(app)?;
            mark("tray_ready");
            tauri::async_runtime::spawn(serve(app.handle().clone(), client, port, control));
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("build tauri app");

    app.run(move |_app, event| match event {
        // No window left is not "quit": only app.exit(code) from the tray menu ends the process.
        RunEvent::ExitRequested { code, api, .. } => {
            if code.is_none() {
                api.prevent_exit();
            }
        }
        RunEvent::WindowEvent { event: WindowEvent::Destroyed, .. } => {
            mark("destroyed");
            if trim_after_destroy {
                trim_working_set();
            }
        }
        _ => {}
    });
}
