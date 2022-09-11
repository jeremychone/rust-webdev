///! as_string  trait/implementations
///! ----
use std::fs::DirEntry;
use std::path::PathBuf;

pub trait XString {
	fn x_string(&self) -> Option<String>;
}

impl XString for PathBuf {
	fn x_string(&self) -> Option<String> {
		self.to_str().map(|v| v.to_string())
	}
}

impl XString for Option<PathBuf> {
	fn x_string(&self) -> Option<String> {
		match self {
			Some(path) => XString::x_string(path),
			None => None,
		}
	}
}

impl XString for DirEntry {
	fn x_string(&self) -> Option<String> {
		self.path().to_str().map(|s| s.to_string())
	}
}

impl XString for Option<DirEntry> {
	fn x_string(&self) -> Option<String> {
		self.as_ref().and_then(|v| DirEntry::x_string(v))
	}
}
