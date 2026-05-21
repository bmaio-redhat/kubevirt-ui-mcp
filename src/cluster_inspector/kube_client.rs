use std::path::Path;

use reqwest::header::AUTHORIZATION;
use serde_json::Value;
use tracing::debug;

/// Minimal Kubernetes REST client built on reqwest.
/// Reads the kubeconfig to get server URL + auth token (or client cert).
#[derive(Clone)]
pub struct KubeClient {
    inner: reqwest::Client,
    server: String,
    token: Option<String>,
}

impl KubeClient {
    pub fn from_kubeconfig(kubeconfig: Option<&Path>, cluster_url: Option<&str>) -> Result<Self, String> {
        let (server, token, ca_cert) = load_kubeconfig(kubeconfig, cluster_url)?;

        let mut builder = reqwest::Client::builder()
            .user_agent("kubevirt-cluster-inspector/0.1 (Cursor MCP)")
            .danger_accept_invalid_certs(true); // many test clusters use self-signed certs

        if let Some(ca) = ca_cert {
            if let Ok(cert) = reqwest::Certificate::from_pem(ca.as_bytes()) {
                builder = builder.add_root_certificate(cert);
            }
        }

        let inner = builder.build().map_err(|e| e.to_string())?;

        Ok(Self { inner, server, token })
    }

    /// GET /apis/{group}/{version}/namespaces/{ns}/{kind_plural}/{name}
    pub async fn get_namespaced(
        &self,
        group: &str,
        version: &str,
        kind_plural: &str,
        namespace: &str,
        name: &str,
    ) -> Result<Value, String> {
        let path = api_path(group, version, Some(namespace), kind_plural, Some(name));
        self.get(&path).await
    }

    /// GET /apis/{group}/{version}/{kind_plural}/{name} (cluster-scoped)
    pub async fn get_cluster(
        &self,
        group: &str,
        version: &str,
        kind_plural: &str,
        name: &str,
    ) -> Result<Value, String> {
        let path = api_path(group, version, None, kind_plural, Some(name));
        self.get(&path).await
    }

    /// List namespaced or cluster-scoped resources with optional label selector.
    pub async fn list(
        &self,
        group: &str,
        version: &str,
        kind_plural: &str,
        namespace: Option<&str>,
        label_selector: Option<&str>,
    ) -> Result<Value, String> {
        let mut path = api_path(group, version, namespace, kind_plural, None);
        if let Some(sel) = label_selector {
            path.push_str(&format!(
                "?labelSelector={}",
                urlencoding_simple(sel)
            ));
        }
        self.get(&path).await
    }

    /// GET /api/v1/namespaces/{ns}/events with fieldSelector for the resource.
    pub async fn get_events(
        &self,
        namespace: &str,
        involved_name: &str,
        involved_kind: &str,
    ) -> Result<Value, String> {
        let field_selector = format!(
            "involvedObject.name={},involvedObject.kind={}",
            involved_name, involved_kind
        );
        let path = format!(
            "/api/v1/namespaces/{}/events?fieldSelector={}",
            namespace,
            urlencoding_simple(&field_selector)
        );
        self.get(&path).await
    }

    async fn get(&self, path: &str) -> Result<Value, String> {
        let url = format!("{}{}", self.server.trim_end_matches('/'), path);
        debug!("GET {}", url);

        let mut req = self.inner.get(&url);
        if let Some(token) = &self.token {
            req = req.header(AUTHORIZATION, format!("Bearer {}", token));
        }

        let resp = req.send().await.map_err(|e| format!("HTTP error: {}", e))?;
        let status = resp.status();
        let body = resp.text().await.map_err(|e| format!("Response read error: {}", e))?;

        if !status.is_success() {
            return Err(format!("Kubernetes API returned {}: {}", status, truncate(&body, 300)));
        }

        serde_json::from_str(&body).map_err(|e| format!("JSON parse error: {} — body: {}", e, truncate(&body, 200)))
    }
}

// ── Kubeconfig parsing ────────────────────────────────────────────────────────

fn load_kubeconfig(
    path: Option<&Path>,
    override_url: Option<&str>,
) -> Result<(String, Option<String>, Option<String>), String> {
    // If explicit cluster URL is given and no kubeconfig, try token from file
    if let Some(url) = override_url {
        let token = read_in_cluster_token();
        return Ok((url.to_string(), token, None));
    }

    let cfg_path = path.ok_or_else(|| "No kubeconfig found. Set KUBECONFIG or CLUSTER_URL.".to_string())?;
    let content = std::fs::read_to_string(cfg_path)
        .map_err(|e| format!("Cannot read kubeconfig: {}", e))?;

    let yaml: Value = serde_yaml_parse(&content)?;

    let current_context = yaml
        .get("current-context")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let contexts = yaml.get("contexts").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let context = contexts
        .iter()
        .find(|c| c.get("name").and_then(|n| n.as_str()) == Some(current_context))
        .and_then(|c| c.get("context"))
        .cloned()
        .unwrap_or(Value::Null);

    let cluster_name = context.get("cluster").and_then(|v| v.as_str()).unwrap_or("");
    let user_name = context.get("user").and_then(|v| v.as_str()).unwrap_or("");

    let clusters = yaml.get("clusters").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let cluster = clusters
        .iter()
        .find(|c| c.get("name").and_then(|n| n.as_str()) == Some(cluster_name))
        .and_then(|c| c.get("cluster"))
        .cloned()
        .unwrap_or(Value::Null);

    let server = override_url
        .map(|s| s.to_string())
        .or_else(|| cluster.get("server").and_then(|v| v.as_str()).map(|s| s.to_string()))
        .ok_or_else(|| "No server URL found in kubeconfig".to_string())?;

    let ca_cert = cluster
        .get("certificate-authority-data")
        .and_then(|v| v.as_str())
        .and_then(|b64| base64_decode_pem(b64));

    let users = yaml.get("users").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let user = users
        .iter()
        .find(|u| u.get("name").and_then(|n| n.as_str()) == Some(user_name))
        .and_then(|u| u.get("user"))
        .cloned()
        .unwrap_or(Value::Null);

    let token = user
        .get("token")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Ok((server, token, ca_cert))
}

fn read_in_cluster_token() -> Option<String> {
    std::fs::read_to_string("/var/run/secrets/kubernetes.io/serviceaccount/token").ok()
}

// ── Minimal YAML → JSON conversion (kubeconfig is YAML) ──────────────────────
// We use a simple heuristic: serde_json can't parse YAML, but kubeconfig YAML
// is close enough to JSON-like that we can use a minimal inline parser.
// For production use, add serde_yaml; for now we handle the common cases.

fn serde_yaml_parse(yaml: &str) -> Result<Value, String> {
    // Try to use serde_json if it somehow is already JSON
    if let Ok(v) = serde_json::from_str(yaml) {
        return Ok(v);
    }

    // Use a minimal YAML→JSON conversion by shelling out to a Python one-liner
    // if available, otherwise fall back to manual kubeconfig field extraction.
    let output = std::process::Command::new("python3")
        .args(["-c", "import sys, json, yaml; print(json.dumps(yaml.safe_load(sys.stdin.read())))"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(stdin) = child.stdin.as_mut() {
                let _ = stdin.write_all(yaml.as_bytes());
            }
            child.wait_with_output().ok()
        });

    if let Some(out) = output {
        if out.status.success() {
            let json_str = String::from_utf8_lossy(&out.stdout);
            if let Ok(v) = serde_json::from_str(json_str.trim()) {
                return Ok(v);
            }
        }
    }

    // Last resort: try yq
    let output = std::process::Command::new("yq")
        .args(["-o=json", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(stdin) = child.stdin.as_mut() {
                let _ = stdin.write_all(yaml.as_bytes());
            }
            child.wait_with_output().ok()
        });

    if let Some(out) = output {
        if out.status.success() {
            let json_str = String::from_utf8_lossy(&out.stdout);
            if let Ok(v) = serde_json::from_str(json_str.trim()) {
                return Ok(v);
            }
        }
    }

    Err("Cannot parse kubeconfig YAML — install python3+pyyaml or yq".to_string())
}

fn base64_decode_pem(b64: &str) -> Option<String> {
    // Use base64 command line tool to decode
    let output = std::process::Command::new("base64")
        .args(["-d"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(stdin) = child.stdin.as_mut() {
                let _ = stdin.write_all(b64.as_bytes());
            }
            child.wait_with_output().ok()
        });

    output.and_then(|out| {
        if out.status.success() {
            String::from_utf8(out.stdout).ok()
        } else {
            None
        }
    })
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn api_path(
    group: &str,
    version: &str,
    namespace: Option<&str>,
    kind_plural: &str,
    name: Option<&str>,
) -> String {
    let base = if group.is_empty() {
        format!("/api/{}", version)
    } else {
        format!("/apis/{}/{}", group, version)
    };

    let ns_segment = namespace
        .filter(|ns| !ns.is_empty())
        .map(|ns| format!("/namespaces/{}", ns))
        .unwrap_or_default();

    let name_segment = name.filter(|n| !n.is_empty()).map(|n| format!("/{}", n)).unwrap_or_default();

    format!("{}{}/{}{}", base, ns_segment, kind_plural, name_segment)
}

fn urlencoding_simple(s: &str) -> String {
    s.replace(' ', "%20")
        .replace(',', "%2C")
        .replace('=', "%3D")
        .replace('/', "%2F")
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() > max {
        &s[..max]
    } else {
        s
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_path_core_namespaced() {
        let p = api_path("", "v1", Some("default"), "pods", Some("mypod"));
        assert_eq!(p, "/api/v1/namespaces/default/pods/mypod");
    }

    #[test]
    fn api_path_custom_resource_namespaced() {
        let p = api_path("kubevirt.io", "v1", Some("test-ns"), "virtualmachines", Some("vm-test"));
        assert_eq!(p, "/apis/kubevirt.io/v1/namespaces/test-ns/virtualmachines/vm-test");
    }

    #[test]
    fn api_path_cluster_scoped() {
        let p = api_path("storage.k8s.io", "v1", None, "storageclasses", None);
        assert_eq!(p, "/apis/storage.k8s.io/v1/storageclasses");
    }

    #[test]
    fn api_path_cluster_scoped_named() {
        let p = api_path("", "v1", None, "nodes", Some("worker-1"));
        assert_eq!(p, "/api/v1/nodes/worker-1");
    }

    #[test]
    fn api_path_empty_namespace_ignored() {
        let p = api_path("kubevirt.io", "v1", Some(""), "virtualmachines", None);
        assert_eq!(p, "/apis/kubevirt.io/v1/virtualmachines");
    }

    #[test]
    fn urlencoding_replaces_special_chars() {
        let encoded = urlencoding_simple("app=myapp,env=test");
        assert_eq!(encoded, "app%3Dmyapp%2Cenv%3Dtest");
    }

    #[test]
    fn truncate_shortens_long_string() {
        let s = "a".repeat(500);
        let t = truncate(&s, 100);
        assert_eq!(t.len(), 100);
    }

    #[test]
    fn truncate_preserves_short_string() {
        let s = "hello";
        assert_eq!(truncate(s, 100), "hello");
    }
}
