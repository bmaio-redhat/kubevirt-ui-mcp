use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub jira_base_url: String,
    pub github_repo: String,
    pub github_commit_pages: u32,
    pub store_path: PathBuf,
}

impl Config {
    pub fn from_env() -> Self {
        let jira_base_url = std::env::var("JIRA_BASE_URL")
            .unwrap_or_else(|_| "https://redhat.atlassian.net".into());

        let github_repo = std::env::var("GITHUB_REPO")
            .unwrap_or_else(|_| "kubevirt-ui/kubevirt-plugin".into());

        let github_commit_pages = std::env::var("GITHUB_COMMIT_PAGES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10u32);

        let store_path = std::env::var("STORE_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
                PathBuf::from(home)
                    .join(".local/share/kubevirt-memory/store.json")
            });

        Self {
            jira_base_url,
            github_repo,
            github_commit_pages,
            store_path,
        }
    }
}
