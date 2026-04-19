use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use tokio::process::Command;

use crate::model::{Component, Snapshot};

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

        let components: Vec<Component> = serde_json::from_slice(&output.stdout)
            .context("failed to parse `bd graph --json` output")?;

        Ok(Snapshot {
            components,
            fetched_at: Utc::now(),
        })
    }
}
