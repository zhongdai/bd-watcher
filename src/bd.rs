use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use serde::Deserialize;
use tokio::process::Command;
use tokio::time;

use crate::model::{Component, DepType, Dependency, Issue, Snapshot};

#[derive(Debug, Clone)]
pub struct BdRunner {
    pub repo: PathBuf,
    pub focus: Option<String>,
}

impl BdRunner {
    pub fn new(repo: impl AsRef<Path>, focus: Option<String>) -> Self {
        Self {
            repo: repo.as_ref().to_path_buf(),
            focus,
        }
    }

    pub async fn check_available() -> Result<()> {
        let output = Command::new("bd")
            .arg("--version")
            .output()
            .await
            .context("failed to spawn `bd` — is it on PATH?")?;
        if !output.status.success() {
            return Err(anyhow!("`bd --version` exited {}", output.status));
        }
        Ok(())
    }

    pub async fn fetch(&self) -> Result<Snapshot> {
        // The embedded Dolt DB allows a single writer at a time. Concurrent
        // bd writes (e.g. an agent running `bd update`) briefly hold an
        // exclusive lock and our read collides. Retry a few times within
        // the poll tick before surfacing the error to the user.
        const BACKOFFS_MS: &[u64] = &[150, 350, 700];

        let mut attempt = 0usize;
        loop {
            match self.fetch_once().await {
                Ok(snap) => return Ok(snap),
                Err(e) => {
                    if let Some(&delay) = BACKOFFS_MS.get(attempt) {
                        if is_transient_lock(&e) {
                            time::sleep(Duration::from_millis(delay)).await;
                            attempt += 1;
                            continue;
                        }
                    }
                    return Err(e);
                }
            }
        }
    }

    async fn fetch_once(&self) -> Result<Snapshot> {
        let mut cmd = Command::new("bd");
        cmd.current_dir(&self.repo);
        cmd.arg("graph");
        match &self.focus {
            Some(id) => {
                cmd.arg(id);
            }
            None => {
                cmd.arg("--all");
            }
        }
        cmd.arg("--json");
        cmd.arg("--readonly");

        let output = cmd.output().await.context("failed to execute `bd graph`")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            // bd writes its structured error to stdout in some modes, plain
            // text to stderr in others; surface whichever has content.
            let msg = if !stderr.trim().is_empty() {
                stderr.trim().to_string()
            } else {
                stdout.trim().to_string()
            };
            return Err(anyhow!("`bd graph` exited {}: {}", output.status, msg));
        }

        let components = parse_graph_output(&output.stdout)
            .context("failed to parse `bd graph --json` output")?;

        Ok(Snapshot {
            components,
            fetched_at: Utc::now(),
        })
    }
}

fn is_transient_lock(err: &anyhow::Error) -> bool {
    let s = format!("{err:#}").to_lowercase();
    s.contains("exclusive lock") || s.contains("another process holds")
}

/// `bd graph --all --json` returns `Vec<Component>`.
/// `bd graph <id> --json` returns a different object shape (root/issues/layout).
/// Accept both and normalize into `Vec<Component>`.
///
/// When the database has no open issues, bd emits the plain text
/// "No open issues found" and ignores --json. Treat that (and any empty
/// output) as an empty graph rather than a parse error.
fn parse_graph_output(bytes: &[u8]) -> Result<Vec<Component>> {
    let trimmed = bytes
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .map(|i| &bytes[i..])
        .unwrap_or(&[]);
    match trimmed.first() {
        None => return Ok(Vec::new()),
        Some(b'[') | Some(b'{') => {}
        // Non-JSON payload (e.g. "No open issues found").
        _ => return Ok(Vec::new()),
    }

    if let Ok(cs) = serde_json::from_slice::<Vec<Component>>(bytes) {
        return Ok(cs);
    }
    let single: SingleEpic = serde_json::from_slice(bytes)
        .map_err(|e| anyhow!("not a recognized bd graph shape: {e}"))?;
    Ok(vec![single.into_component()])
}

#[derive(Debug, Deserialize)]
struct SingleEpic {
    root: Issue,
    #[serde(default)]
    issues: Vec<Issue>,
    #[serde(default)]
    layout: Option<SingleLayout>,
}

#[derive(Debug, Deserialize)]
struct SingleLayout {
    #[serde(rename = "Nodes", default)]
    nodes: HashMap<String, SingleNode>,
}

#[derive(Debug, Deserialize)]
struct SingleNode {
    #[serde(rename = "DependsOn", default)]
    depends_on: Option<Vec<String>>,
}

impl SingleEpic {
    fn into_component(self) -> Component {
        // Derive Dependency list from layout.Nodes[*].DependsOn. The single-
        // epic format doesn't label edge types, so record them as Blocks
        // (the semantic that ordering info represents).
        let mut dependencies: Vec<Dependency> = Vec::new();
        if let Some(layout) = self.layout {
            for (id, node) in layout.nodes {
                if let Some(deps) = node.depends_on {
                    for dep in deps {
                        dependencies.push(Dependency {
                            issue_id: id.clone(),
                            depends_on_id: dep,
                            dep_type: DepType::Blocks,
                        });
                    }
                }
            }
        }
        Component {
            root: self.root,
            issues: self.issues,
            dependencies,
        }
    }
}
