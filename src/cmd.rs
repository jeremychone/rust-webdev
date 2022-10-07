use clap::{crate_version, Arg, ArgAction, Command};

pub fn cmd_app() -> Command {
	Command::new("webdev")
		.version(crate_version!())
		.about("Simple static file web serving using warp")
		.arg(Arg::new("public").num_args(0).long("public").help("Open the server the world"))
		.arg(Arg::new("port").short('p').num_args(1).help("port (default 8080)"))
		.arg(
			Arg::new("watch")
				.short('w')
				.num_args(1)
				.action(ArgAction::Append)
				.help("watch path to watch (can be multiple). By default same a the serving folder."),
		)
		.arg(Arg::new("dir").short('d').num_args(1).help("Root local dir to be served"))
		.arg(
			Arg::new("live")
				.short('l')
				.num_args(0)
				.long("live")
				.help("Add script tag to all html file for live reload"),
		)
}
