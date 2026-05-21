use std::sync::Arc;

use serde_json::Value;

use crate::cluster_inspector::kube_client::KubeClient;
use crate::mcp::protocol::ToolCallResult;

/// Known KubeVirt resource types to enumerate in a namespace inventory.
const RESOURCE_TYPES: &[(&str, &str, &str, &str)] = &[
    // (group, version, kind_plural, display_name)
    ("kubevirt.io", "v1", "virtualmachines", "VirtualMachines"),
    ("kubevirt.io", "v1", "virtualmachineinstances", "VirtualMachineInstances"),
    ("kubevirt.io", "v1", "virtualmachineinstancemigrations", "VMMigrations"),
    ("cdi.kubevirt.io", "v1beta1", "datavolumes", "DataVolumes"),
    ("snapshot.kubevirt.io", "v1beta1", "virtualmachinesnapshots", "VMSnapshots"),
    ("snapshot.kubevirt.io", "v1beta1", "virtualmachinesnapshotcontents", "VMSnapshotContents"),
    ("instancetype.kubevirt.io", "v1beta1", "virtualmachineinstancetypes", "InstanceTypes"),
    ("instancetype.kubevirt.io", "v1beta1", "virtualmachinepreferences", "Preferences"),
    ("migrations.kubevirt.io", "v1alpha1", "migrationpolicies", "MigrationPolicies"),
    ("template.openshift.io", "v1", "templates", "Templates"),
    ("k8s.cni.cncf.io", "v1", "network-attachment-definitions", "NetworkAttachmentDefs"),
    ("", "v1", "pods", "Pods"),
    ("", "v1", "persistentvolumeclaims", "PVCs"),
    ("", "v1", "secrets", "Secrets"),
    ("", "v1", "configmaps", "ConfigMaps"),
];

pub async fn get_namespace_inventory(params: &Value, client: &Arc<KubeClient>) -> ToolCallResult {
    let namespace = match params.get("namespace").and_then(|v| v.as_str()) {
        Some(ns) => ns,
        None => return ToolCallResult::error("Missing required parameter: namespace"),
    };

    let mut out = format!("## Namespace inventory: {}\n\n", namespace);
    let mut total = 0usize;

    for (group, version, kind_plural, display_name) in RESOURCE_TYPES {
        match client.list(group, version, kind_plural, Some(namespace), None).await {
            Ok(list) => {
                let items =
                    list.get("items").and_then(|v| v.as_array()).cloned().unwrap_or_default();
                if !items.is_empty() {
                    total += items.len();
                    out.push_str(&format!("### {} ({})\n", display_name, items.len()));
                    for item in &items {
                        let name = item
                            .get("metadata")
                            .and_then(|m| m.get("name"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("(unknown)");
                        let phase = item
                            .get("status")
                            .and_then(|s| {
                                s.get("phase")
                                    .or(s.get("printableStatus"))
                                    .or(s.get("readyReplicas").map(|_| s.get("phase")).flatten())
                            })
                            .and_then(|v| v.as_str())
                            .unwrap_or("(no phase)");
                        let finalizers = item
                            .get("metadata")
                            .and_then(|m| m.get("finalizers"))
                            .and_then(|v| v.as_array())
                            .map(|a| a.len())
                            .unwrap_or(0);

                        out.push_str(&format!(
                            "  - {} [{}]{}",
                            name,
                            phase,
                            if finalizers > 0 {
                                format!(" ⚠ {} finalizer(s)", finalizers)
                            } else {
                                String::new()
                            }
                        ));
                        out.push('\n');
                    }
                    out.push('\n');
                }
            }
            Err(_) => {
                // Resource type may not be installed; silently skip
            }
        }
    }

    if total == 0 {
        out.push_str("(namespace is empty or all resource types returned errors)\n");
    } else {
        out.push_str(&format!("Total objects found: {}\n", total));
    }

    ToolCallResult::text(out)
}

pub async fn explain_stuck_namespace(params: &Value, client: &Arc<KubeClient>) -> ToolCallResult {
    let namespace = match params.get("namespace").and_then(|v| v.as_str()) {
        Some(ns) => ns,
        None => return ToolCallResult::error("Missing required parameter: namespace"),
    };

    // Check namespace object itself
    let ns_obj = client.list("", "v1", "namespaces", None, None).await;
    let ns_phase = ns_obj
        .as_ref()
        .ok()
        .and_then(|list| list.get("items"))
        .and_then(|v| v.as_array())
        .and_then(|arr| {
            arr.iter().find(|ns| {
                ns.get("metadata").and_then(|m| m.get("name")).and_then(|v| v.as_str())
                    == Some(namespace)
            })
        })
        .and_then(|ns| ns.get("status"))
        .and_then(|s| s.get("phase"))
        .and_then(|v| v.as_str())
        .unwrap_or("(unknown)")
        .to_string();

    let mut out = format!(
        "## Stuck namespace analysis: {}\n\nCurrent phase: {}\n\n",
        namespace, ns_phase
    );

    // Check for resources with finalizers
    let mut blocking = Vec::new();

    for (group, version, kind_plural, display_name) in RESOURCE_TYPES {
        match client.list(group, version, kind_plural, Some(namespace), None).await {
            Ok(list) => {
                let items =
                    list.get("items").and_then(|v| v.as_array()).cloned().unwrap_or_default();
                for item in &items {
                    let name = item
                        .get("metadata")
                        .and_then(|m| m.get("name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("(unknown)");
                    let finalizers: Vec<String> = item
                        .get("metadata")
                        .and_then(|m| m.get("finalizers"))
                        .and_then(|v| v.as_array())
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_str())
                                .map(|s| s.to_string())
                                .collect()
                        })
                        .unwrap_or_default();

                    let deletion_timestamp = item
                        .get("metadata")
                        .and_then(|m| m.get("deletionTimestamp"))
                        .and_then(|v| v.as_str())
                        .is_some();

                    if !finalizers.is_empty() || deletion_timestamp {
                        blocking.push(format!(
                            "  - {} {} [finalizers: {}] {}",
                            display_name,
                            name,
                            finalizers.join(", "),
                            if deletion_timestamp { "(⚠ awaiting deletion)" } else { "" }
                        ));
                    }
                }
            }
            Err(_) => {}
        }
    }

    if blocking.is_empty() {
        out.push_str("No resources with blocking finalizers found.\n");
        out.push_str(
            "Namespace may be stuck due to a webhook timeout or cluster-level controller issue.\n",
        );
    } else {
        out.push_str(&format!("### Blocking resources ({} found)\n", blocking.len()));
        out.push_str(&blocking.join("\n"));
        out.push_str("\n\nFix: remove finalizers with `oc patch <resource> <name> -n <ns> -p '{\"metadata\":{\"finalizers\":[]}}' --type=merge`\n");
    }

    ToolCallResult::text(out)
}
