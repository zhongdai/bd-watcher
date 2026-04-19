//! Verify the real `bd graph --all --json` output parses into our model.
//! Skipped automatically if `bd` is not on PATH or not in a bd repo.

use std::process::Command;

use bd_watcher::model::Component;

#[test]
fn parses_real_bd_graph_all_json_if_available() {
    // Prefer an env-provided path to a bd repo; fall back to any we can find.
    let repo = std::env::var("BD_WATCHER_TEST_REPO").ok();

    let mut cmd = Command::new("bd");
    cmd.arg("graph")
        .arg("--all")
        .arg("--json")
        .arg("--readonly");
    if let Some(ref r) = repo {
        cmd.current_dir(r);
    }

    let output = match cmd.output() {
        Ok(o) => o,
        Err(_) => {
            eprintln!("bd not available, skipping");
            return;
        }
    };

    if !output.status.success() {
        eprintln!(
            "bd graph failed (not in a bd repo?), skipping: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }

    let parsed: Result<Vec<Component>, _> = serde_json::from_slice(&output.stdout);
    assert!(
        parsed.is_ok(),
        "failed to parse real bd output: {:?}",
        parsed.err()
    );
    let components = parsed.unwrap();
    eprintln!("parsed {} components", components.len());
}
