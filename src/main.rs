use crate::cmd::cmd_app;
use crate::tmpl::JS_LIVE_SCRIPT_TAG;
use crate::tmpl::{HTML_DIR_LIST_END, HTML_DIR_LIST_START, JS_LIVE_CONTENT};
use crate::xts::XString;
use futures::{SinkExt, StreamExt};
use notify::RecursiveMode;
use notify_debouncer_mini::new_debouncer;
use pathdiff::diff_paths;
use std::format as f;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;
use warp::hyper::Response;
use warp::log::Info;
use warp::path::FullPath;
use warp::reply::Html;
use warp::ws::{Message, WebSocket};
use warp::Filter;

mod cmd;
mod tmpl;
mod xts;

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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	let app = cmd_app().get_matches();

	// --- Get the port
	let port = app
		.get_one::<String>("port")
		.and_then(|val| val.parse::<u16>().ok())
		.unwrap_or(DEFAULT_PORT);

	// --- Get the root directory path
	let root_dir = app
		.get_one::<String>("dir")
		.map(|v| v.to_owned())
		.unwrap_or_else(|| DEFAULT_WEB_FOLDER.to_owned());

	// --- Root dir to be served
	let root_dir = Path::new(&root_dir).to_path_buf();

	// --- webdev live watch
	let live_mode = app.get_flag("live");

	let live_ws_counter = Counter::default();
	let live_ws_counter = Arc::new(live_ws_counter);

	let watch_paths = app.get_many::<String>("watch").map(|vals| vals.collect::<Vec<_>>());
	let watch_paths = watch_paths
		.map(|v| v.iter().map(|i| Path::new(i).to_path_buf()).collect::<Vec<PathBuf>>())
		.unwrap_or_else(|| vec![root_dir.clone()]);

	let (broadcast_change_tx, _) = do_watch_paths(watch_paths).await;

	let webdev_live_js = warp::path("_webdev_live.js").and(warp::get()).map(|| {
		Response::builder()
			.header("content-type", "text/javascript;charset=UTF-8")
			.body(JS_LIVE_CONTENT)
	});

	let webdev_watch_ws = warp::path("_webdev_live_ws")
		// The `ws()` filter will prepare the Websocket handshake.
		.and(warp::ws())
		.and(warp::any().map(move || broadcast_change_tx.subscribe()))
		.and(warp::any().map(move || live_ws_counter.clone()))
		.map(
			|ws: warp::ws::Ws, change_rx: broadcast::Receiver<()>, live_ws_counter: Arc<Counter>| {
				// And then our closure will be called when it completes...
				ws.on_upgrade(|websocket| live_watch(websocket, change_rx, live_ws_counter))
			},
		);

	let webdev_live_watch = webdev_live_js.or(webdev_watch_ws);

	// --- Special fitlers for dir listing and html files
	let special_filter = with_path_type(Arc::new(root_dir.clone()))
		.and(warp::any().map(move || live_mode))
		.and_then(special_file_handler);

	// --- Fall back to normal file serving
	let warp_dir_filter = warp::fs::dir(root_dir.clone());

	// --- Combine Routes
	let routes = webdev_live_watch.or(special_filter).or(warp_dir_filter);

	// add the log
	let routes = routes.with(warp::log::custom(log_req));

	// --- Serve service
	println!(
		"Starting webdev server http://localhost:{}/ at dir {}",
		port,
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

	let is_public = app.get_flag("public");

	let ip = if is_public {
		println!("! public mode on (listening on 0.0.0.0)");
		[0, 0, 0, 0]
	} else {
		[127, 0, 0, 1]
	};
	warp::serve(routes).run((ip, port)).await;

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

async fn live_watch(ws: WebSocket, mut change_rx: broadcast::Receiver<()>, _live_ws_counter: Arc<Counter>) {
	let (mut ws_tx, _) = ws.split();

	tokio::task::spawn(async move {
		loop {
			let _ = change_rx.recv().await;
			let send_res = ws_tx.send(Message::text("server_files_changed".to_string())).await;
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

fn with_path_type(
	root_dir: Arc<PathBuf>,
) -> impl Filter<Extract = (SpecialPath,), Error = std::convert::Infallible> + Clone {
	warp::any().and(warp::path::full()).map(move |full_path: FullPath| {
		let web_path = full_path.as_str().trim_start_matches('/');

		// -- Add .html on extension less path.
		// If no extension and not end with /, for now add `.html`
		// Later, this might be a config property.
		let target_path = if !web_path.is_empty() && !web_path.contains('.') && !web_path.ends_with('/') {
			root_dir.join(format!("{web_path}.html"))
		} else {
			root_dir.join(web_path)
		};

		let path_info = PathInfo {
			root_dir: root_dir.clone(),
			target_path,
		};

		if path_info.target_path.is_dir() {
			SpecialPath::Dir(path_info)
		} else if path_info.target_path.is_file() {
			match path_info.target_path.extension().and_then(|s| s.to_str()) {
				None => SpecialPath::ExtLessFile(path_info),
				Some("html") | Some("HTML") => SpecialPath::HtmlFile(path_info),
				_ => SpecialPath::NotSpecial,
			}
		} else {
			SpecialPath::NotSpecial
		}
	})
}

async fn special_file_handler(special_path: SpecialPath, live_mode: bool) -> Result<Html<String>, warp::Rejection> {
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
								let href = format!("/{}", diff);
								html.push_str(&format!(r#"<a href="{}">{}{suffix}</a>"#, href, disp));
							}
						}
					}
				}
				Err(_) => html.push_str(&format!("Cannot read dir of '{}'", target_path.to_string_lossy())),
			}

			let html = f!("{HTML_DIR_LIST_START}{html}{HTML_DIR_LIST_END}");

			Ok(warp::reply::html(html))
		}
		SpecialPath::ExtLessFile(path_info) | SpecialPath::HtmlFile(path_info) => {
			// FIXME: Remove the unwrap
			let mut html = fs::read_to_string(path_info.target_path).unwrap();
			if live_mode {
				html.push_str(JS_LIVE_SCRIPT_TAG);
			}
			Ok(warp::reply::html(html))
		}
		// When not special, return not found in this handler, so that the default warp::dir
		// filter can take over.
		SpecialPath::NotSpecial => Err(warp::reject::not_found()),
	}
}
// endregion: --- Special File (dir and extension less)

fn log_req(info: Info) {
	println!(
		" {} {} {} ({}ms)",
		info.method(),
		info.status(),
		info.path(),
		info.elapsed().as_micros() as f64 / 1000.
	);
}
