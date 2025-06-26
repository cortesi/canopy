use expectrl::Session;
use std::process::Command;

/// Spawn a binary built by Cargo using `expectrl`.
///
/// `name` should be the crate name as listed in `Cargo.toml`.
/// Any additional arguments are passed to the binary.
/// The returned [`Session`] can be used to drive the process.
pub fn spawn_bin(name: &str, args: &[&str]) -> std::result::Result<Session, expectrl::Error> {
    let bin_path = std::env::var(format!("CARGO_BIN_EXE_{name}"))
        .expect("missing CARGO_BIN_EXE path for binary");
    let mut cmd = Command::new(bin_path);
    cmd.args(args);
    Session::spawn(cmd)
}

pub use expectrl::{Eof, Regex};
