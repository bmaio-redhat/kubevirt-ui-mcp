use serde_json::{json, Value};
use tracing::warn;

use crate::cluster_inspector::kube_client::KubeClient;

pub async fn get_cluster_info(client: &KubeClient) -> Value {
    let mut info = serde_json::Map::new();

    match client.list("", "v1", "nodes", None, None).await {
        Ok(nodes) => {
            let items = nodes.get("items").and_then(|v| v.as_array()).cloned().unwrap_or_default();
            info.insert("nodeCount".into(), json!(items.len()));
            if let Some(first) = items.first() {
                let version = first
                    .get("status")
                    .and_then(|s| s.get("nodeInfo"))
                    .and_then(|n| n.get("kubeletVersion"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                info.insert("kubernetesVersion".into(), json!(version));
            }
        }
        Err(e) => {
            warn!("Could not list nodes: {}", e);
            info.insert("nodeCount".into(), json!("error"));
            info.insert("kubernetesVersion".into(), json!(format!("error: {}", e)));
        }
    }

    // KubeVirt version via HCO
    match client
        .get_namespaced("hco.kubevirt.io", "v1beta1", "hyperconvergeds", "kubevirt-hyperconverged", "kubevirt-hyperconverged")
        .await
    {
        Ok(hco) => {
            let versions = hco
                .get("status")
                .and_then(|s| s.get("versions"))
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let kv_version = versions
                .iter()
                .find(|v| v.get("name").and_then(|n| n.as_str()) == Some("kubevirt"))
                .and_then(|v| v.get("version"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            info.insert("kubevirtVersion".into(), json!(kv_version));
            let cnv_version = versions
                .iter()
                .find(|v| v.get("name").and_then(|n| n.as_str()) == Some("operator"))
                .and_then(|v| v.get("version"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            info.insert("cnvVersion".into(), json!(cnv_version));
        }
        Err(_) => {
            info.insert("kubevirtVersion".into(), json!("not found"));
            info.insert("cnvVersion".into(), json!("not found"));
        }
    }

    json!(info)
}

pub async fn list_vms(client: &KubeClient, namespace: Option<&str>) -> Value {
    match client.list("kubevirt.io", "v1", "virtualmachines", namespace, None).await {
        Ok(list) => {
            let items = list.get("items").and_then(|v| v.as_array()).cloned().unwrap_or_default();
            let vms: Vec<Value> = items
                .iter()
                .map(|vm| {
                    let name = vm
                        .get("metadata")
                        .and_then(|m| m.get("name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let ns = vm
                        .get("metadata")
                        .and_then(|m| m.get("namespace"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let status = vm
                        .get("status")
                        .and_then(|s| s.get("printableStatus"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("Unknown");
                    let cpu = vm
                        .get("spec")
                        .and_then(|s| s.get("template"))
                        .and_then(|t| t.get("spec"))
                        .and_then(|s| s.get("domain"))
                        .and_then(|d| d.get("cpu"))
                        .cloned()
                        .unwrap_or(Value::Null);
                    let memory = vm
                        .get("spec")
                        .and_then(|s| s.get("template"))
                        .and_then(|t| t.get("spec"))
                        .and_then(|s| s.get("domain"))
                        .and_then(|d| d.get("resources"))
                        .and_then(|r| r.get("requests"))
                        .and_then(|r| r.get("memory"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    let run_strategy = vm
                        .get("spec")
                        .and_then(|s| s.get("runStrategy"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    json!({
                        "name": name,
                        "namespace": ns,
                        "status": status,
                        "cpu": cpu,
                        "memory": memory,
                        "runStrategy": run_strategy,
                    })
                })
                .collect();
            json!({ "vms": vms, "count": vms.len() })
        }
        Err(e) => json!({ "error": format!("Failed to list VMs: {}", e) }),
    }
}

pub async fn get_vm_detail(client: &KubeClient, namespace: &str, name: &str) -> Value {
    match client
        .get_namespaced("kubevirt.io", "v1", "virtualmachines", namespace, name)
        .await
    {
        Ok(vm) => {
            let vmi = client
                .get_namespaced("kubevirt.io", "v1", "virtualmachineinstances", namespace, name)
                .await
                .ok();
            let events = client.get_events(namespace, name, "VirtualMachine").await.ok();
            json!({
                "vm": vm,
                "vmi": vmi,
                "events": events,
            })
        }
        Err(e) => json!({ "error": format!("Failed to get VM {}/{}: {}", namespace, name, e) }),
    }
}

pub async fn list_test_namespaces(client: &KubeClient) -> Value {
    match client.list("", "v1", "namespaces", None, None).await {
        Ok(list) => {
            let items = list.get("items").and_then(|v| v.as_array()).cloned().unwrap_or_default();
            let test_ns: Vec<Value> = items
                .iter()
                .filter(|ns| {
                    ns.get("metadata")
                        .and_then(|m| m.get("name"))
                        .and_then(|v| v.as_str())
                        .map(|n| n.starts_with("pw-"))
                        .unwrap_or(false)
                })
                .map(|ns| {
                    let name = ns
                        .get("metadata")
                        .and_then(|m| m.get("name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let phase = ns
                        .get("status")
                        .and_then(|s| s.get("phase"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("Unknown");
                    let created = ns
                        .get("metadata")
                        .and_then(|m| m.get("creationTimestamp"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let age_hours = if !created.is_empty() {
                        age_hours_from_ts(created)
                    } else {
                        0.0
                    };
                    json!({ "name": name, "status": phase, "created": created, "ageHours": age_hours })
                })
                .collect();

            let mut sorted = test_ns;
            sorted.sort_by(|a, b| {
                let age_a = a["ageHours"].as_f64().unwrap_or(0.0);
                let age_b = b["ageHours"].as_f64().unwrap_or(0.0);
                age_b.partial_cmp(&age_a).unwrap_or(std::cmp::Ordering::Equal)
            });

            json!({ "count": sorted.len(), "namespaces": sorted })
        }
        Err(e) => json!({ "error": format!("Failed to list namespaces: {}", e) }),
    }
}

pub async fn cleanup_stale_namespaces(
    client: &KubeClient,
    older_than_hours: f64,
) -> Value {
    let ns_result = list_test_namespaces(client).await;
    let namespaces = match ns_result.get("namespaces").and_then(|v| v.as_array()) {
        Some(arr) => arr.clone(),
        None => return json!({ "error": "Could not list test namespaces" }),
    };

    let stale: Vec<_> = namespaces
        .iter()
        .filter(|ns| ns["ageHours"].as_f64().unwrap_or(0.0) > older_than_hours)
        .collect();

    if stale.is_empty() {
        return json!({
            "message": format!("No stale namespaces older than {}h found", older_than_hours),
            "deleted": [],
            "failed": [],
        });
    }

    let mut deleted = Vec::new();
    let mut failed = Vec::new();

    for ns in &stale {
        let name = ns["name"].as_str().unwrap_or_default();
        let status = tokio::process::Command::new("oc")
            .args(["delete", "namespace", name, "--wait=false"])
            .output()
            .await;
        match status {
            Ok(out) if out.status.success() => deleted.push(name.to_string()),
            Ok(out) => failed.push(json!({
                "name": name,
                "error": String::from_utf8_lossy(&out.stderr).to_string()
            })),
            Err(e) => failed.push(json!({ "name": name, "error": e.to_string() })),
        }
    }

    json!({
        "message": format!("Cleaned up {} of {} stale namespaces", deleted.len(), stale.len()),
        "deleted": deleted,
        "failed": failed,
    })
}

pub async fn check_cluster_health(client: &KubeClient) -> Value {
    let mut checks: Vec<Value> = Vec::new();

    // API server reachable
    match client.list("", "v1", "namespaces", None, None).await {
        Ok(_) => checks.push(json!({ "check": "API Server", "status": "ok", "detail": "Reachable" })),
        Err(e) => {
            checks.push(json!({ "check": "API Server", "status": "error", "detail": e.to_string() }));
            return json!({ "healthy": false, "checks": checks });
        }
    }

    // CNV operator
    match client.list("", "v1", "pods", Some("openshift-cnv"), None).await {
        Ok(pods) => {
            let items = pods.get("items").and_then(|v| v.as_array()).cloned().unwrap_or_default();
            let operator_pods: Vec<_> = items
                .iter()
                .filter(|p| {
                    let name = p.get("metadata").and_then(|m| m.get("name")).and_then(|v| v.as_str()).unwrap_or("");
                    name.contains("hyperconverged") || name.contains("hco-operator")
                })
                .collect();
            let running = operator_pods.iter().filter(|p| {
                p.get("status").and_then(|s| s.get("phase")).and_then(|v| v.as_str()) == Some("Running")
            }).count();
            checks.push(json!({
                "check": "CNV Operator",
                "status": if running > 0 { "ok" } else { "warning" },
                "detail": format!("{} pod(s) running", running),
            }));
        }
        Err(_) => checks.push(json!({ "check": "CNV Operator", "status": "warning", "detail": "Could not check openshift-cnv namespace" })),
    }

    // virt-api
    match client.list("", "v1", "pods", Some("openshift-cnv"), None).await {
        Ok(pods) => {
            let items = pods.get("items").and_then(|v| v.as_array()).cloned().unwrap_or_default();
            let virt_api: Vec<_> = items.iter().filter(|p| {
                p.get("metadata").and_then(|m| m.get("name")).and_then(|v| v.as_str())
                    .map(|n| n.starts_with("virt-api")).unwrap_or(false)
            }).collect();
            let running = virt_api.iter().filter(|p| {
                p.get("status").and_then(|s| s.get("phase")).and_then(|v| v.as_str()) == Some("Running")
            }).count();
            checks.push(json!({
                "check": "virt-api",
                "status": if running > 0 { "ok" } else { "warning" },
                "detail": format!("{}/{} pods running", running, virt_api.len()),
            }));
        }
        Err(_) => checks.push(json!({ "check": "virt-api", "status": "warning", "detail": "Could not check" })),
    }

    // Storage classes
    match client.list("storage.k8s.io", "v1", "storageclasses", None, None).await {
        Ok(list) => {
            let items = list.get("items").and_then(|v| v.as_array()).cloned().unwrap_or_default();
            let names: Vec<_> = items.iter()
                .filter_map(|s| s.get("metadata").and_then(|m| m.get("name")).and_then(|v| v.as_str()))
                .collect();
            let has_virt = names.iter().any(|n| n.contains("ceph") || n.contains("rbd") || n.contains("virt"));
            checks.push(json!({
                "check": "Storage Classes",
                "status": if has_virt { "ok" } else { "warning" },
                "detail": format!("{} classes found", names.len()),
            }));
        }
        Err(_) => checks.push(json!({ "check": "Storage Classes", "status": "warning", "detail": "Could not list storage classes" })),
    }

    // Nodes
    match client.list("", "v1", "nodes", None, None).await {
        Ok(list) => {
            let items = list.get("items").and_then(|v| v.as_array()).cloned().unwrap_or_default();
            let total = items.len();
            let ready = items.iter().filter(|n| {
                n.get("status")
                    .and_then(|s| s.get("conditions"))
                    .and_then(|c| c.as_array())
                    .map(|conds| conds.iter().any(|c| {
                        c.get("type").and_then(|v| v.as_str()) == Some("Ready")
                            && c.get("status").and_then(|v| v.as_str()) == Some("True")
                    }))
                    .unwrap_or(false)
            }).count();
            checks.push(json!({
                "check": "Nodes",
                "status": if ready == total { "ok" } else { "warning" },
                "detail": format!("{}/{} nodes ready", ready, total),
            }));
        }
        Err(_) => checks.push(json!({ "check": "Nodes", "status": "warning", "detail": "Could not list nodes" })),
    }

    let healthy = checks.iter().all(|c| c["status"].as_str() != Some("error"));
    json!({ "healthy": healthy, "checks": checks })
}

fn age_hours_from_ts(ts: &str) -> f64 {
    // Parse RFC3339 timestamps. Use chrono if available.
    if let Ok(dt) = ts.parse::<chrono::DateTime<chrono::Utc>>() {
        let age = chrono::Utc::now().signed_duration_since(dt);
        return age.num_seconds() as f64 / 3600.0;
    }
    0.0
}
