use crate::cmd::cmd_app;
use crate::tmpl::{HTML_DIR_LIST_END, HTML_DIR_LIST_START, JS_LIVE_CONTENT, JS_LIVE_SCRIPT_TAG};
use crate::xts::XString;
use axum::Router;
use axum::body::Body;
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Request, State, WebSocketUpgrade};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use futures::{SinkExt, StreamExt};
use notify::RecursiveMode;
use notify_debouncer_mini::new_debouncer;
use pathdiff::diff_paths;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::{format as f, fs};
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tower::ServiceExt;
use tower_http::services::ServeDir;

// region:    --- Modules

mod cmd;
mod tmpl;
mod xts;

// endregion: --- Modules

const DEFAULT_PORT: u16 = 8080;
const DEFAULT_WEB_FOLDER: &str = "./";
const DEBOUNCE_DURATION_MS: u64 = 50;

#[derive(Default)]
struct Counter(Arc<Mutex<i32>>);
impl Counter {
	#[allow(unused)]
	fn inc(&self) -> i32 {
		let mut val = self.0.lock().unwrap();
		*val += 1;
		*val
	}
}

struct AppState {
	root_dir: Arc<PathBuf>,
	live_mode: bool,
	broadcast_change_tx: broadcast::Sender<()>,
	#[allow(unused)]
	live_ws_counter: Arc<Counter>,
	serve_dir: ServeDir,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	let app_args = cmd_app().get_matches();

	// --- Get the port
	let port = app_args
		.get_one::<String>("port")
		.and_then(|val| val.parse::<u16>().ok())
		.unwrap_or(DEFAULT_PORT);

	// --- Get the root directory path
	let root_dir_str = app_args
		.get_one::<String>("dir")
		.map(|v| v.to_owned())
		.unwrap_or_else(|| DEFAULT_WEB_FOLDER.to_owned());

	// --- Root dir to be served
	let root_dir = Path::new(&root_dir_str).to_path_buf();
	let root_dir = Arc::new(root_dir);

	// --- webdev live watch
	let live_mode = app_args.get_flag("live");

	let live_ws_counter = Counter::default();
	let live_ws_counter = Arc::new(live_ws_counter);

	let watch_paths = app_args.get_many::<String>("watch").map(|vals| vals.collect::<Vec<_>>());
	let watch_paths = watch_paths
		.map(|v| v.iter().map(|i| Path::new(i).to_path_buf()).collect::<Vec<PathBuf>>())
		.unwrap_or_else(|| vec![root_dir.as_ref().clone()]);

	let (broadcast_change_tx, _) = do_watch_paths(watch_paths).await;

	let serve_dir = ServeDir::new(root_dir.as_ref());

	let state = Arc::new(AppState {
		root_dir: root_dir.clone(),
		live_mode,
		broadcast_change_tx,
		live_ws_counter,
		serve_dir,
	});

	let routes = Router::new()
		.route(
			"/_webdev_live.js",
			get(|| async {
				Response::builder()
					.header("content-type", "text/javascript;charset=UTF-8")
					.body(Body::from(JS_LIVE_CONTENT))
					.unwrap()
			}),
		)
		.route(
			"/_webdev_live_ws",
			get(|ws: WebSocketUpgrade, State(state): State<Arc<AppState>>| async move {
				ws.on_upgrade(move |socket| live_watch(socket, state.broadcast_change_tx.subscribe()))
			}),
		)
		.fallback(special_file_handler)
		.with_state(state)
		.layer(middleware::from_fn(log_mw));

	// --- Serve service
	println!(
		"Starting webdev server http://localhost:{port}/ at dir {}",
		root_dir.to_string_lossy()
	);
	if !live_mode {
		println!(
			"\tFor live mode add '<script src=\"/_webdev_live.js\"></script>' to htmls,
\tor run command with 'webdev -l' to automatically add script tag to all served html files."
		);
	} else {
		println!("\tlive mode on.")
	}

	let is_public = app_args.get_flag("public");
	let addr = if is_public {
		println!("! public mode on (listening on 0.0.0.0)");
		format!("0.0.0.0:{port}")
	} else {
		format!("127.0.0.1:{port}")
	};

	let listener = TcpListener::bind(addr).await?;
	axum::serve(listener, routes).await?;

	Ok(())
}

// region:    --- Live Watch
async fn do_watch_paths(watch_paths: Vec<PathBuf>) -> (broadcast::Sender<()>, broadcast::Receiver<()>) {
	let (change_tx, change_rx) = broadcast::channel(32);

	let change_tx_clone = change_tx.clone();

	// Note - Must be block because the notify watch rx is blocking
	//        Otherwise, endup by not sending all events.
	tokio::task::spawn_blocking(move || {
		let (tx, rx) = std::sync::mpsc::channel();

		// Create a watcher object, delivering debounced events.
		// The notification back-end is selected based on the platform.
		// let mut watcher = watcher(tx, Duration::from_millis(200)).unwrap();

		// No specific tickrate, max debounce time
		let mut debouncer = new_debouncer(Duration::from_millis(DEBOUNCE_DURATION_MS), tx).unwrap();

		let watcher = debouncer.watcher();

		for watch_path in watch_paths {
			println!("watching path: {}", watch_path.to_string_lossy());
			watcher.watch(watch_path.as_ref(), RecursiveMode::Recursive).unwrap();
		}

		// print all events, non returning
		for _events in rx {
			// let events = _events.unwrap();
			// for e in events {
			// 	println!("  ->> event {:?} {}", e.kind, e.path.to_string_lossy())
			// }
			println!("Change detected. Broadcasting change event to _webdev_live_ws websockets.");
			let _ = change_tx_clone.send(());
		}
	});

	(change_tx, change_rx)
}

async fn live_watch(ws: WebSocket, mut change_rx: broadcast::Receiver<()>) {
	let (mut ws_tx, _) = ws.split();

	tokio::task::spawn(async move {
		loop {
			let _ = change_rx.recv().await;
			let send_res = ws_tx.send(Message::Text("server_files_changed".into())).await;
			// if we have an error, we break which will drop this websocket
			if send_res.is_err() {
				break;
			}
		}
	});
}
// endregion: --- Live Watch

// region:    --- Special File (dir and extension less)
#[derive(Debug)]
struct PathInfo {
	root_dir: Arc<PathBuf>,
	target_path: PathBuf,
}

#[derive(Debug)]
enum SpecialPath {
	Dir(PathInfo),
	ExtLessFile(PathInfo),
	HtmlFile(PathInfo),
	NotSpecial,
}

async fn special_file_handler(State(state): State<Arc<AppState>>, req: Request<Body>) -> impl IntoResponse {
	let uri = req.uri().clone();
	let web_path = uri.path().trim_start_matches('/');

	// -- Add .html on extension less path.
	// If no extension and not end with /, for now add `.html`
	// Later, this might be a config property.
	let target_path = if !web_path.is_empty() && !web_path.contains('.') && !web_path.ends_with('/') {
		state.root_dir.join(f!("{web_path}.html"))
	} else {
		state.root_dir.join(web_path)
	};

	let path_info = PathInfo {
		root_dir: state.root_dir.clone(),
		target_path,
	};

	let special_path = if path_info.target_path.is_dir() {
		SpecialPath::Dir(path_info)
	} else if path_info.target_path.is_file() {
		match path_info.target_path.extension().and_then(|s| s.to_str()) {
			None => SpecialPath::ExtLessFile(path_info),
			Some("html") | Some("HTML") => SpecialPath::HtmlFile(path_info),
			_ => SpecialPath::NotSpecial,
		}
	} else {
		SpecialPath::NotSpecial
	};

	match special_path {
		SpecialPath::Dir(path_info) => {
			// TODO: Needs to handle the case when we have a index.html
			let PathInfo { root_dir, target_path } = path_info;
			let mut html = String::new();

			let paths = fs::read_dir(&target_path);
			match paths {
				Ok(paths) => {
					for path in paths {
						if let Some(path) = path.ok().map(|v| v.path()) {
							if let Some(diff) = diff_paths(&path, root_dir.as_ref()).x_string() {
								let disp = path.file_name().and_then(|s| s.to_str()).unwrap_or("unknown");
								let suffix = if path.is_dir() { "/" } else { "" };
								let href = format!("/{}{suffix}", diff);
								html.push_str(&format!(r#"<a href="{}">{}{suffix}</a>"#, href, disp));
							}
						}
					}
				}
				Err(_) => html.push_str(&format!("Cannot read dir of '{}'", target_path.to_string_lossy())),
			}

			let html = f!("{HTML_DIR_LIST_START}{html}{HTML_DIR_LIST_END}");

			Html(html).into_response()
		}
		SpecialPath::ExtLessFile(path_info) | SpecialPath::HtmlFile(path_info) => {
			match fs::read_to_string(path_info.target_path) {
				Ok(mut html) => {
					if state.live_mode {
						html.push_str(JS_LIVE_SCRIPT_TAG);
					}
					Html(html).into_response()
				}
				Err(_) => state.serve_dir.clone().oneshot(req).await.unwrap().into_response(),
			}
		}
		// When not special, use serve_dir to handle the request.
		SpecialPath::NotSpecial => state.serve_dir.clone().oneshot(req).await.unwrap().into_response(),
	}
}
// endregion: --- Special File (dir and extension less)

async fn log_mw(req: Request, next: Next) -> Response {
	let start = Instant::now();
	let method = req.method().clone();
	let path = req.uri().path().to_string();

	let response = next.run(req).await;

	let status = response.status();
	let elapsed = start.elapsed().as_micros() as f64 / 1000.;

	println!(" {method} {status} {path} ({elapsed}ms)");

	response
}
