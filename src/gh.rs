use std::path::Path;
use std::process::Stdio;

use tokio::process::Command;

/// Owner+repo for the GitHub project a bd repo is mirrored to. Used to
/// build PR URLs for clickable hyperlinks in the focused-epic view.
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

/// Detect the GitHub owner+repo for a directory by reading the `origin`
/// git remote. Returns `None` if the directory isn't a git repo, has no
/// `origin` remote, or the remote URL doesn't point at github.com.
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

/// Parse an `owner/name` pair out of a github.com remote URL. Handles the
/// shapes git remotes typically take:
///   - `git@github.com:owner/name.git`
///   - `https://github.com/owner/name.git`
///   - `https://github.com/owner/name`
///   - `ssh://git@github.com/owner/name.git`
///   - `git+ssh://git@github.com/owner/name.git`  (bd's `sync.remote` shape)
pub fn parse_github_remote(url: &str) -> Option<GhRepo> {
    let after = url.split("github.com").nth(1)?;
    let after = after.trim_start_matches([':', '/']);
    let path = after.split_whitespace().next()?;
    let path = path.strip_suffix(".git").unwrap_or(path);
    let (owner, rest) = path.split_once('/')?;
    // Trim anything after the repo name (extra path, query, fragment).
    let name = rest.split(['/', '?', '#']).next()?;
    if owner.is_empty() || name.is_empty() {
        return None;
    }
    Some(GhRepo {
        owner: owner.to_string(),
        name: name.to_string(),
    })
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
        assert_eq!(
            parse_github_remote("https://github.com/zhongdai/bd-watcher"),
            Some(gh("zhongdai", "bd-watcher")),
        );
    }

    #[test]
    fn parses_ssh_remote() {
        assert_eq!(
            parse_github_remote("git@github.com:zhongdai/bd-watcher.git"),
            Some(gh("zhongdai", "bd-watcher")),
        );
        assert_eq!(
            parse_github_remote("ssh://git@github.com/zhongdai/bd-watcher.git"),
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
        assert_eq!(parse_github_remote("https://example.com/foo/bar"), None);
    }

    #[test]
    fn rejects_malformed_inputs() {
        assert_eq!(parse_github_remote(""), None);
        assert_eq!(parse_github_remote("github.com"), None);
        assert_eq!(parse_github_remote("https://github.com/"), None);
        assert_eq!(parse_github_remote("https://github.com/owner"), None);
        assert_eq!(parse_github_remote("https://github.com/owner/"), None);
    }

    #[test]
    fn pr_url_builds_correctly() {
        assert_eq!(
            gh("zhongdai", "bd-watcher").pr_url(196),
            "https://github.com/zhongdai/bd-watcher/pull/196",
        );
    }
}
