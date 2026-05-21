use std::sync::Arc;

use serde_json::Value;

use crate::cluster_inspector::kube_client::KubeClient;
use crate::mcp::protocol::ToolCallResult;

// ── get_resource ──────────────────────────────────────────────────────────────

pub async fn get_resource(params: &Value, client: &Arc<KubeClient>) -> ToolCallResult {
    let group = params.get("group").and_then(|v| v.as_str()).unwrap_or("");
    let version = match params.get("version").and_then(|v| v.as_str()) {
        Some(v) => v,
        None => return ToolCallResult::error("Missing required parameter: version"),
    };
    let kind_plural = match params.get("kind_plural").and_then(|v| v.as_str()) {
        Some(k) => k,
        None => return ToolCallResult::error("Missing required parameter: kind_plural"),
    };
    let name = match params.get("name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => return ToolCallResult::error("Missing required parameter: name"),
    };
    let namespace = params.get("namespace").and_then(|v| v.as_str()).unwrap_or("");

    let resource_result = if namespace.is_empty() {
        client.get_cluster(group, version, kind_plural, name).await
    } else {
        client.get_namespaced(group, version, kind_plural, namespace, name).await
    };

    match resource_result {
        Ok(resource) => {
            // Also fetch events for the resource
            let events_text = if !namespace.is_empty() {
                let kind = kind_from_plural(kind_plural);
                match client.get_events(namespace, name, &kind).await {
                    Ok(events) => format_events(&events),
                    Err(e) => format!("(Could not fetch events: {})", e),
                }
            } else {
                "(cluster-scoped resource — no namespace events)".to_string()
            };

            let resource_pretty = serde_json::to_string_pretty(&resource)
                .unwrap_or_else(|e| format!("Serialization error: {}", e));

            ToolCallResult::text(format!(
                "## {}/{} — {}\n\n### Resource\n```json\n{}\n```\n\n### Events\n{}",
                kind_plural,
                name,
                if namespace.is_empty() { "(cluster-scoped)" } else { namespace },
                resource_pretty,
                events_text
            ))
        }
        Err(e) => ToolCallResult::error(format!("Failed to get {}/{}: {}", kind_plural, name, e)),
    }
}

// ── list_resources ────────────────────────────────────────────────────────────

pub async fn list_resources(params: &Value, client: &Arc<KubeClient>) -> ToolCallResult {
    let group = params.get("group").and_then(|v| v.as_str()).unwrap_or("");
    let version = match params.get("version").and_then(|v| v.as_str()) {
        Some(v) => v,
        None => return ToolCallResult::error("Missing required parameter: version"),
    };
    let kind_plural = match params.get("kind_plural").and_then(|v| v.as_str()) {
        Some(k) => k,
        None => return ToolCallResult::error("Missing required parameter: kind_plural"),
    };
    let namespace = params.get("namespace").and_then(|v| v.as_str());
    let label_selector = params.get("label_selector").and_then(|v| v.as_str());

    match client.list(group, version, kind_plural, namespace, label_selector).await {
        Ok(list) => {
            let items = list.get("items").and_then(|v| v.as_array()).cloned().unwrap_or_default();

            if items.is_empty() {
                return ToolCallResult::text(format!(
                    "No {} found{}{}.",
                    kind_plural,
                    namespace.map(|ns| format!(" in namespace '{}'", ns)).unwrap_or_default(),
                    label_selector.map(|l| format!(" with label '{}'", l)).unwrap_or_default(),
                ));
            }

            let summaries: Vec<String> = items
                .iter()
                .map(|item| {
                    let name = item
                        .get("metadata")
                        .and_then(|m| m.get("name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("(unknown)");
                    let ns = item
                        .get("metadata")
                        .and_then(|m| m.get("namespace"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let phase = item
                        .get("status")
                        .and_then(|s| s.get("phase").or(s.get("printableStatus")))
                        .and_then(|v| v.as_str())
                        .unwrap_or("(no phase)");

                    if ns.is_empty() {
                        format!("  - {} [{}]", name, phase)
                    } else {
                        format!("  - {}/{} [{}]", ns, name, phase)
                    }
                })
                .collect();

            ToolCallResult::text(format!(
                "{} {} found:\n{}",
                items.len(),
                kind_plural,
                summaries.join("\n")
            ))
        }
        Err(e) => ToolCallResult::error(format!("Failed to list {}: {}", kind_plural, e)),
    }
}

// ── get_hco_status ────────────────────────────────────────────────────────────

pub async fn get_hco_status(params: &Value, client: &Arc<KubeClient>) -> ToolCallResult {
    let name = params
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("kubevirt-hyperconverged");
    let namespace = params
        .get("namespace")
        .and_then(|v| v.as_str())
        .unwrap_or("kubevirt-hyperconverged");

    match client
        .get_namespaced("hco.kubevirt.io", "v1beta1", "hyperconvergeds", namespace, name)
        .await
    {
        Ok(hco) => {
            let conditions = extract_conditions(&hco);
            let versions = hco
                .get("status")
                .and_then(|s| s.get("versions"))
                .cloned()
                .unwrap_or(Value::Null);
            let related_objects = hco
                .get("status")
                .and_then(|s| s.get("relatedObjects"))
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|o| {
                            let kind = o.get("kind").and_then(|v| v.as_str())?;
                            let name = o.get("name").and_then(|v| v.as_str())?;
                            Some(format!("  - {} {}", kind, name))
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .unwrap_or_default();

            let versions_pretty =
                serde_json::to_string_pretty(&versions).unwrap_or_else(|_| "(none)".into());

            ToolCallResult::text(format!(
                "## HyperConverged: {}/{}\n\n\
                ### Conditions\n{}\n\n\
                ### Component Versions\n```json\n{}\n```\n\n\
                ### Related Objects\n{}",
                namespace,
                name,
                conditions,
                versions_pretty,
                related_objects
            ))
        }
        Err(e) => ToolCallResult::error(format!("Failed to get HCO: {}", e)),
    }
}

// ── get_vm_events ─────────────────────────────────────────────────────────────

pub async fn get_vm_events(params: &Value, client: &Arc<KubeClient>) -> ToolCallResult {
    let name = match params.get("name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => return ToolCallResult::error("Missing required parameter: name"),
    };
    let namespace = match params.get("namespace").and_then(|v| v.as_str()) {
        Some(ns) => ns,
        None => return ToolCallResult::error("Missing required parameter: namespace"),
    };
    let kind = params.get("kind").and_then(|v| v.as_str()).unwrap_or("VirtualMachine");

    match client.get_events(namespace, name, kind).await {
        Ok(events) => {
            let formatted = format_events(&events);
            ToolCallResult::text(format!(
                "## Events for {} {}/{}\n\n{}",
                kind, namespace, name, formatted
            ))
        }
        Err(e) => ToolCallResult::error(format!("Failed to get events: {}", e)),
    }
}

// ── get_storage_class_info ────────────────────────────────────────────────────

pub async fn get_storage_class_info(client: &Arc<KubeClient>) -> ToolCallResult {
    match client.list("storage.k8s.io", "v1", "storageclasses", None, None).await {
        Ok(list) => {
            let items = list.get("items").and_then(|v| v.as_array()).cloned().unwrap_or_default();

            if items.is_empty() {
                return ToolCallResult::text("No storage classes found.");
            }

            let lines: Vec<String> = items
                .iter()
                .map(|sc| {
                    let name = sc
                        .get("metadata")
                        .and_then(|m| m.get("name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("(unknown)");
                    let provisioner = sc
                        .get("provisioner")
                        .and_then(|v| v.as_str())
                        .unwrap_or("(unknown)");
                    let binding_mode = sc
                        .get("volumeBindingMode")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Immediate");
                    let reclaim = sc
                        .get("reclaimPolicy")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Delete");
                    let is_default = sc
                        .get("metadata")
                        .and_then(|m| m.get("annotations"))
                        .and_then(|a| {
                            a.get("storageclass.kubernetes.io/is-default-class")
                                .or(a.get("storageclass.beta.kubernetes.io/is-default-class"))
                        })
                        .and_then(|v| v.as_str())
                        == Some("true");

                    format!(
                        "  - {} | provisioner: {} | bindingMode: {} | reclaim: {}{}",
                        name,
                        provisioner,
                        binding_mode,
                        reclaim,
                        if is_default { " [DEFAULT]" } else { "" }
                    )
                })
                .collect();

            ToolCallResult::text(format!("{} storage classes:\n{}", items.len(), lines.join("\n")))
        }
        Err(e) => ToolCallResult::error(format!("Failed to list storage classes: {}", e)),
    }
}

// ── get_node_status ───────────────────────────────────────────────────────────

pub async fn get_node_status(client: &Arc<KubeClient>) -> ToolCallResult {
    match client.list("", "v1", "nodes", None, None).await {
        Ok(list) => {
            let items = list.get("items").and_then(|v| v.as_array()).cloned().unwrap_or_default();

            if items.is_empty() {
                return ToolCallResult::text("No nodes found.");
            }

            let lines: Vec<String> = items
                .iter()
                .map(|node| {
                    let name = node
                        .get("metadata")
                        .and_then(|m| m.get("name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("(unknown)");

                    let ready = node
                        .get("status")
                        .and_then(|s| s.get("conditions"))
                        .and_then(|c| c.as_array())
                        .and_then(|arr| {
                            arr.iter()
                                .find(|c| c.get("type").and_then(|t| t.as_str()) == Some("Ready"))
                                .and_then(|c| c.get("status").and_then(|s| s.as_str()))
                        })
                        .unwrap_or("Unknown");

                    let os_image = node
                        .get("status")
                        .and_then(|s| s.get("nodeInfo"))
                        .and_then(|i| i.get("osImage"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("(unknown)");

                    let roles: Vec<&str> = node
                        .get("metadata")
                        .and_then(|m| m.get("labels"))
                        .and_then(|l| l.as_object())
                        .map(|labels| {
                            labels
                                .keys()
                                .filter_map(|k| k.strip_prefix("node-role.kubernetes.io/"))
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();

                    let taints_count = node
                        .get("spec")
                        .and_then(|s| s.get("taints"))
                        .and_then(|t| t.as_array())
                        .map(|a| a.len())
                        .unwrap_or(0);

                    format!(
                        "  - {} | Ready: {} | roles: {} | OS: {} | taints: {}",
                        name,
                        ready,
                        if roles.is_empty() { "worker".to_string() } else { roles.join(",") },
                        os_image,
                        taints_count
                    )
                })
                .collect();

            ToolCallResult::text(format!("{} nodes:\n{}", items.len(), lines.join("\n")))
        }
        Err(e) => ToolCallResult::error(format!("Failed to list nodes: {}", e)),
    }
}

// ── Shared helpers ────────────────────────────────────────────────────────────

fn format_events(events: &Value) -> String {
    let items = match events.get("items").and_then(|v| v.as_array()) {
        Some(a) => a,
        None => return "(no events)".to_string(),
    };

    if items.is_empty() {
        return "(no events found)".to_string();
    }

    let mut lines: Vec<String> = items
        .iter()
        .map(|e| {
            let reason = e.get("reason").and_then(|v| v.as_str()).unwrap_or("(no reason)");
            let message = e.get("message").and_then(|v| v.as_str()).unwrap_or("(no message)");
            let count = e.get("count").and_then(|v| v.as_u64()).unwrap_or(1);
            let last_time = e
                .get("lastTimestamp")
                .or(e.get("eventTime"))
                .and_then(|v| v.as_str())
                .unwrap_or("(unknown time)");
            let etype = e.get("type").and_then(|v| v.as_str()).unwrap_or("Normal");
            format!("  [{} x{}] {} ({}): {}", etype, count, reason, last_time, message)
        })
        .collect();

    lines.sort();
    lines.join("\n")
}

fn extract_conditions(resource: &Value) -> String {
    let conditions = match resource
        .get("status")
        .and_then(|s| s.get("conditions"))
        .and_then(|c| c.as_array())
    {
        Some(c) => c,
        None => return "(no conditions)".to_string(),
    };

    conditions
        .iter()
        .map(|c| {
            let ctype = c.get("type").and_then(|v| v.as_str()).unwrap_or("(unknown)");
            let status = c.get("status").and_then(|v| v.as_str()).unwrap_or("(unknown)");
            let reason = c.get("reason").and_then(|v| v.as_str()).unwrap_or("");
            let message = c.get("message").and_then(|v| v.as_str()).unwrap_or("");
            if reason.is_empty() && message.is_empty() {
                format!("  - {}: {}", ctype, status)
            } else {
                format!("  - {}: {} — {} {}", ctype, status, reason, message)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn kind_from_plural(kind_plural: &str) -> String {
    // Simple heuristic: VirtualMachines → VirtualMachine
    match kind_plural {
        "virtualmachines" => "VirtualMachine".into(),
        "virtualmachineinstances" => "VirtualMachineInstance".into(),
        "virtualmachineinstancemigrations" => "VirtualMachineInstanceMigration".into(),
        "datavolumes" => "DataVolume".into(),
        "virtualmachinesnapshots" => "VirtualMachineSnapshot".into(),
        "pods" => "Pod".into(),
        "persistentvolumeclaims" => "PersistentVolumeClaim".into(),
        other => {
            // Capitalize first letter and remove trailing 's'
            let mut chars = other.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => {
                    let s: String = c.to_uppercase().collect::<String>() + chars.as_str();
                    if s.ends_with('s') { s[..s.len() - 1].to_string() } else { s }
                }
            }
        }
    }
}
