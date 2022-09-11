use clap::{crate_version, Arg, Command};

pub fn cmd_app() -> Command<'static> {
	Command::new("webdev")
		.version(&crate_version!()[..])
		.about("Simple static file web serving using warp")
		.arg(Arg::new("public").long("public").help("Open the server the world"))
		.arg(Arg::new("port").short('p').takes_value(true).help("port (default 8080)"))
		.arg(
			Arg::new("watch")
				.short('w')
				.takes_value(true)
				.multiple_occurrences(true)
				.help("watch path to watch (can be multiple). By default same a the serving folder."),
		)
		.arg(Arg::new("dir").short('d').takes_value(true).help("Root local dir to be served"))
		.arg(
			Arg::new("live")
				.short('l')
				.long("live")
				.help("Add script tag to all html file for live reload"),
		)
}
