pub mod kube_client;
pub mod tools;

pub use kube_client::KubeClient;

use std::sync::Arc;
use tracing::warn;

use crate::config::Config;

pub fn build_kube_client(cfg: &Config) -> Arc<KubeClient> {
    match KubeClient::from_kubeconfig(
        cfg.kubeconfig.as_deref(),
        cfg.cluster_url.as_deref(),
    ) {
        Ok(c) => Arc::new(c),
        Err(e) => {
            warn!("Could not initialize KubeClient: {}. Cluster tools will fail gracefully.", e);
            match KubeClient::from_kubeconfig(None, Some("http://localhost:8080")) {
                Ok(c) => Arc::new(c),
                Err(e2) => {
                    warn!("Fallback KubeClient also failed: {}. Using dummy.", e2);
                    Arc::new(KubeClient::from_kubeconfig(None, Some("http://localhost:8080")).unwrap())
                }
            }
        }
    }
}
