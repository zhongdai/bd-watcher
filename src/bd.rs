use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use serde::Deserialize;
use tokio::process::Command;

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
            return Err(anyhow!(
                "`bd graph` exited {}: {}",
                output.status,
                stderr.trim()
            ));
        }

        let components = parse_graph_output(&output.stdout)
            .context("failed to parse `bd graph --json` output")?;

        Ok(Snapshot {
            components,
            fetched_at: Utc::now(),
        })
    }
}

/// `bd graph --all --json` returns `Vec<Component>`.
/// `bd graph <id> --json` returns a different object shape (root/issues/layout).
/// Accept both and normalize into `Vec<Component>`.
fn parse_graph_output(bytes: &[u8]) -> Result<Vec<Component>> {
    // Try the all-mode shape first.
    if let Ok(cs) = serde_json::from_slice::<Vec<Component>>(bytes) {
        return Ok(cs);
    }
    // Fall back to single-epic shape.
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
