//! Minimal clipboard helper that shells out to whichever native tool is
//! available. Keeps the dependency footprint light — no clipboard crate.

use std::io::Write;
use std::process::{Command, Stdio};

/// Attempts to copy `text` to the system clipboard using the first available
/// helper. Returns the name of the tool that succeeded, or an error message.
pub fn copy(text: &str) -> Result<&'static str, String> {
    let candidates: &[(&str, &[&str])] = &[
        ("pbcopy", &[]),
        ("wl-copy", &[]),
        ("xclip", &["-selection", "clipboard"]),
        ("xsel", &["--clipboard", "--input"]),
    ];

    let mut last_err: Option<String> = None;
    for (bin, args) in candidates {
        match spawn_and_write(bin, args, text) {
            Ok(()) => return Ok(*bin),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| "no clipboard helper found".into()))
}

fn spawn_and_write(bin: &str, args: &[&str], text: &str) -> Result<(), String> {
    let mut child = Command::new(bin)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("{bin}: {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(text.as_bytes())
            .map_err(|e| format!("{bin} stdin: {e}"))?;
    }
    let status = child.wait().map_err(|e| format!("{bin} wait: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{bin} exited {status}"))
    }
}
