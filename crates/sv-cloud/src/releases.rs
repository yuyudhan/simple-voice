// FilePath: crates/sv-cloud/src/releases.rs
//! The newest published release, from GitHub's `releases/latest` endpoint. It skips drafts and
//! pre-releases, and it is the same release the install script follows.

use std::time::Duration;

use semver::Version;
use serde::Deserialize;
use sv_domain::{AppError, AppResult};

const GITHUB_PREFIX: &str = "https://github.com/";
const API_BASE: &str = "https://api.github.com/repos";
const TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LatestRelease {
    pub version: Version,
    /// The release page, with its notes.
    pub url: String,
}

#[derive(Debug, Deserialize)]
struct ReleaseResponse {
    tag_name: String,
    html_url: String,
}

/// `https://github.com/owner/name` (the workspace `repository` field) → its latest-release API URL.
pub fn latest_release_url(repository: &str) -> AppResult<String> {
    let (owner, name) = github_repository(repository)?;
    Ok(format!("{API_BASE}/{owner}/{name}/releases/latest"))
}

/// The install script published with every release (`scripts/install.sh`), always the newest.
pub fn install_script_url(repository: &str) -> AppResult<String> {
    let (owner, name) = github_repository(repository)?;
    Ok(format!(
        "{GITHUB_PREFIX}{owner}/{name}/releases/latest/download/install.sh"
    ))
}

fn github_repository(repository: &str) -> AppResult<(&str, &str)> {
    let path = repository
        .trim()
        .strip_prefix(GITHUB_PREFIX)
        .map(|path| path.trim_end_matches('/'))
        .map(|path| path.strip_suffix(".git").unwrap_or(path));
    match path.and_then(|path| path.split_once('/')) {
        Some((owner, name)) if !owner.is_empty() && !name.is_empty() && !name.contains('/') => {
            Ok((owner, name))
        }
        _ => Err(AppError::invalid(format!(
            "“{repository}” is not a GitHub repository URL"
        ))),
    }
}

/// Fetches the latest release. `user_agent` is required by GitHub's API.
pub async fn latest_release(
    client: &reqwest::Client,
    url: &str,
    user_agent: &str,
) -> AppResult<LatestRelease> {
    let response = client
        .get(url)
        .header(reqwest::header::USER_AGENT, user_agent)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .timeout(TIMEOUT)
        .send()
        .await
        .map_err(|error| transport_error(&error))?;
    let status = response.status().as_u16();
    let body = response
        .bytes()
        .await
        .map_err(|error| transport_error(&error))?;
    match status {
        200 => parse_release(&body),
        404 => Err(AppError::network("No release has been published yet")),
        // GitHub answers 403 as well as 429 once the hourly anonymous limit is used up.
        403 | 429 => Err(AppError::network(
            "GitHub is limiting requests from this network; try again later",
        )),
        _ => Err(AppError::network(format!(
            "GitHub answered with HTTP {status}"
        ))),
    }
}

fn parse_release(body: &[u8]) -> AppResult<LatestRelease> {
    let release: ReleaseResponse = serde_json::from_slice(body)
        .map_err(|_| AppError::network("GitHub returned a release it could not read"))?;
    let version = parse_tag(&release.tag_name).ok_or_else(|| {
        AppError::network(format!(
            "The latest release tag “{}” is not a version",
            release.tag_name
        ))
    })?;
    Ok(LatestRelease {
        version,
        url: release.html_url,
    })
}

/// `v0.3.0` or `0.3.0` → `0.3.0`.
fn parse_tag(tag: &str) -> Option<Version> {
    let tag = tag.trim();
    Version::parse(tag.strip_prefix('v').unwrap_or(tag)).ok()
}

fn transport_error(error: &reqwest::Error) -> AppError {
    if error.is_timeout() {
        AppError::network(format!(
            "GitHub did not answer within {} s",
            TIMEOUT.as_secs()
        ))
    } else {
        AppError::network(format!("Could not reach GitHub: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_the_api_url_from_the_repository_url() {
        let expected = Ok("https://api.github.com/repos/owner/app/releases/latest".to_owned());
        assert_eq!(latest_release_url("https://github.com/owner/app"), expected);
        assert_eq!(
            latest_release_url("https://github.com/owner/app/"),
            expected
        );
        assert_eq!(
            latest_release_url("https://github.com/owner/app.git"),
            expected
        );
        for bad in [
            "https://gitlab.com/owner/app",
            "https://github.com/owner",
            "https://github.com/owner/app/tree/main",
            "",
        ] {
            assert!(latest_release_url(bad).is_err(), "{bad} accepted");
        }
    }

    #[test]
    fn builds_the_install_script_url_from_the_repository_url() {
        assert_eq!(
            install_script_url("https://github.com/owner/app.git"),
            Ok("https://github.com/owner/app/releases/latest/download/install.sh".to_owned())
        );
        assert!(install_script_url("https://gitlab.com/owner/app").is_err());
    }

    #[test]
    fn reads_the_version_from_the_tag() {
        let body = br#"{"tag_name":"v0.10.0","html_url":"https://github.com/o/a/releases/tag/v0.10.0","draft":false}"#;
        let release = parse_release(body);
        assert_eq!(
            release,
            Ok(LatestRelease {
                version: Version::new(0, 10, 0),
                url: "https://github.com/o/a/releases/tag/v0.10.0".to_owned(),
            })
        );
        assert!(parse_release(br#"{"tag_name":"nightly","html_url":"x"}"#).is_err());
        assert!(parse_release(b"<html>rate limited</html>").is_err());
    }

    #[test]
    fn orders_versions_numerically_not_as_text() {
        let newer = |a: &str, b: &str| parse_tag(a) > parse_tag(b);
        assert!(newer("v0.10.0", "0.9.9"));
        assert!(newer("1.0.0", "1.0.0-beta.2"));
        assert!(!newer("v0.2.0", "0.2.0"));
    }
}
