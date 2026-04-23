use std::path::Path;
use std::process::Stdio;

use tokio::process::Command;

/// Owner+repo for the GitHub project a bd repo is mirrored to. Used to
/// build PR URLs for the `v` keybinding that opens a bead's PR in the
/// default browser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhRepo {
    pub owner: String,
    pub name: String,
}

impl GhRepo {
    pub fn pr_url(&self, pr: u32) -> String {
        format!("https://github.com/{}/{}/pull/{pr}", self.owner, self.name)
    }
}

/// Detects the GitHub owner+repo for a directory by reading the
/// `origin` git remote. Returns `None` if the directory isn't a git
/// repo, has no `origin`, or the remote URL isn't github.com.
pub async fn detect(repo_dir: &Path) -> Option<GhRepo> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_dir)
        .arg("remote")
        .arg("get-url")
        .arg("origin")
        .stderr(Stdio::null())
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let url = String::from_utf8(output.stdout).ok()?;
    parse_github_remote(url.trim())
}

/// Parses `owner/name` out of the common github.com remote URL shapes:
///   - `git@github.com:owner/name.git`
///   - `https://github.com/owner/name.git`
///   - `https://github.com/owner/name`
///   - `ssh://git@github.com/owner/name.git`
///   - `git+ssh://git@github.com/owner/name.git`  (bd's `sync.remote`)
pub fn parse_github_remote(url: &str) -> Option<GhRepo> {
    let after = url.split("github.com").nth(1)?;
    let after = after.trim_start_matches([':', '/']);
    let path = after.split_whitespace().next()?;
    let path = path.strip_suffix(".git").unwrap_or(path);
    let (owner, rest) = path.split_once('/')?;
    let name = rest.split(['/', '?', '#']).next()?;
    if owner.is_empty() || name.is_empty() {
        return None;
    }
    Some(GhRepo {
        owner: owner.to_string(),
        name: name.to_string(),
    })
}

/// Extracts a GitHub PR number from a bead's `external_ref` field when
/// it matches the `gh-<N>` convention documented in CLAUDE.md. Returns
/// None for other ref shapes (jira-*, linear-*), empty refs, or
/// unparseable input.
pub fn parse_pr_number(external_ref: Option<&str>) -> Option<u32> {
    external_ref
        .and_then(|s| s.strip_prefix("gh-"))
        .and_then(|n| n.parse::<u32>().ok())
}

/// Opens `url` in the OS's default browser. Spawns the platform opener
/// detached; we don't wait on it. Returns any spawn error for the
/// caller to surface via toast.
pub fn open_in_browser(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    let cmd = "open";
    #[cfg(target_os = "linux")]
    let cmd = "xdg-open";
    #[cfg(target_os = "windows")]
    let cmd = "explorer";
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    let cmd = "xdg-open";

    std::process::Command::new(cmd)
        .arg(url)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gh(owner: &str, name: &str) -> GhRepo {
        GhRepo {
            owner: owner.to_string(),
            name: name.to_string(),
        }
    }

    #[test]
    fn parses_https_remote() {
        assert_eq!(
            parse_github_remote("https://github.com/zhongdai/bd-watcher.git"),
            Some(gh("zhongdai", "bd-watcher")),
        );
    }

    #[test]
    fn parses_ssh_remote() {
        assert_eq!(
            parse_github_remote("git@github.com:zhongdai/bd-watcher.git"),
            Some(gh("zhongdai", "bd-watcher")),
        );
    }

    #[test]
    fn parses_bd_sync_remote_shape() {
        assert_eq!(
            parse_github_remote("git+ssh://git@github.com/ROKT/selection.git"),
            Some(gh("ROKT", "selection")),
        );
    }

    #[test]
    fn rejects_non_github_remotes() {
        assert_eq!(parse_github_remote("https://gitlab.com/foo/bar.git"), None);
        assert_eq!(parse_github_remote("git@bitbucket.org:foo/bar.git"), None);
    }

    #[test]
    fn rejects_malformed_inputs() {
        assert_eq!(parse_github_remote(""), None);
        assert_eq!(parse_github_remote("github.com"), None);
        assert_eq!(parse_github_remote("https://github.com/owner"), None);
    }

    #[test]
    fn pr_url_builds_correctly() {
        assert_eq!(
            gh("zhongdai", "bd-watcher").pr_url(196),
            "https://github.com/zhongdai/bd-watcher/pull/196",
        );
    }

    #[test]
    fn parse_pr_number_extracts_gh_prefix() {
        assert_eq!(parse_pr_number(Some("gh-196")), Some(196));
        assert_eq!(parse_pr_number(Some("gh-10006")), Some(10006));
    }

    #[test]
    fn parse_pr_number_rejects_non_gh_or_malformed() {
        assert_eq!(parse_pr_number(None), None);
        assert_eq!(parse_pr_number(Some("jira-SEL-1")), None);
        assert_eq!(parse_pr_number(Some("gh-")), None);
        assert_eq!(parse_pr_number(Some("gh-abc")), None);
    }
}
