use std::path::PathBuf;

/// Unified configuration sourced from environment variables for all modules.
#[derive(Debug, Clone)]
pub struct Config {
    // ── Shared ───────────────────────────────────────────────────────────────
    pub project_root: PathBuf,

    // ── Playwright paths (derived from project_root) ──────────────────────
    pub playwright_root: PathBuf,

    // ── Kubernetes ───────────────────────────────────────────────────────────
    pub kubeconfig: Option<PathBuf>,
    pub cluster_url: Option<String>,
    pub oc_path: String,
    pub virtctl_path: String,

    // ── GitHub ───────────────────────────────────────────────────────────────
    pub github_repo: String,

    // ── Jira / memory ────────────────────────────────────────────────────────
    pub jira_base_url: String,
    pub github_commit_pages: u32,
    pub store_path: PathBuf,

    // ── CI triage / Jenkins ──────────────────────────────────────────────────
    pub jenkins_url: Option<String>,
    pub jenkins_user: Option<String>,
    pub jenkins_token: Option<String>,

    // ── Product docs ───────────────────────────────────────────────────────
    pub docs_cache_path: PathBuf,
}

impl Config {
    pub fn from_env() -> Self {
        let project_root = std::env::var("KUBEVIRT_PROJECT_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
                PathBuf::from(home).join("Developer/Projects/kubevirt-ui")
            });

        let playwright_root = project_root.join("playwright");

        let kubeconfig = std::env::var("KUBECONFIG")
            .ok()
            .map(PathBuf::from)
            .or_else(|| {
                let pw_cfg = project_root.join(".kubeconfigs/test-config");
                if pw_cfg.exists() { Some(pw_cfg) } else { None }
            })
            .or_else(|| {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
                let default = PathBuf::from(home).join(".kube/config");
                if default.exists() { Some(default) } else { None }
            });

        let store_path = std::env::var("STORE_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
                PathBuf::from(home).join(".local/share/kubevirt-memory/store.json")
            });

        Self {
            project_root,
            playwright_root,
            kubeconfig,
            cluster_url: std::env::var("CLUSTER_URL").ok(),
            oc_path: std::env::var("OC_PATH").unwrap_or_else(|_| "oc".into()),
            virtctl_path: std::env::var("VIRTCTL_PATH").unwrap_or_else(|_| "virtctl".into()),
            github_repo: std::env::var("GITHUB_REPO")
                .unwrap_or_else(|_| "kubevirt-ui/kubevirt-plugin".into()),
            jira_base_url: std::env::var("JIRA_BASE_URL")
                .unwrap_or_else(|_| "https://redhat.atlassian.net".into()),
            github_commit_pages: std::env::var("GITHUB_COMMIT_PAGES")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(10u32),
            store_path,
            jenkins_url: std::env::var("JENKINS_URL").ok(),
            jenkins_user: std::env::var("JENKINS_USER").ok(),
            jenkins_token: std::env::var("JENKINS_TOKEN").ok(),
            docs_cache_path: std::env::var("DOCS_CACHE_PATH")
                .map(PathBuf::from)
                .unwrap_or_else(|_| {
                    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
                    PathBuf::from(home).join(".local/share/kubevirt-memory/docs-cache.json")
                }),
        }
    }

    /// Compatibility method — same as the `playwright_root` field.
    pub fn playwright_root(&self) -> &std::path::Path {
        &self.playwright_root
    }

    pub fn junit_path(&self) -> PathBuf {
        self.project_root.join("junit-results/junit.xml")
    }

    pub fn allure_dir(&self) -> PathBuf {
        self.project_root.join("allure-results")
    }

    pub fn tests_dir(&self) -> PathBuf {
        self.playwright_root.join("tests")
    }

    pub fn docs_dir(&self) -> PathBuf {
        self.playwright_root.join("docs")
    }
}
