pub struct DocSection {
    pub id: &'static str,
    pub title: &'static str,
    pub dir: &'static str,
    pub file: &'static str,
    pub tags: &'static [&'static str],
}

pub const SECTIONS: &[DocSection] = &[
    // ── About ────────────────────────────────────────────────────────────
    DocSection { id: "about", title: "About OpenShift Virtualization", dir: "virt/about_virt", file: "about-virt.adoc", tags: &["overview", "features", "vsphere-comparison"] },
    DocSection { id: "architecture", title: "Architecture", dir: "virt/about_virt", file: "virt-architecture.adoc", tags: &["architecture", "components", "virt-api", "virt-controller", "virt-handler"] },
    DocSection { id: "security-policies", title: "Security Policies", dir: "virt/about_virt", file: "virt-security-policies.adoc", tags: &["security", "rbac", "scc", "policies"] },
    DocSection { id: "supported-limits", title: "Supported Limits", dir: "virt/about_virt", file: "virt-supported-limits.adoc", tags: &["limits", "maximums", "scale"] },

    // ── Install ──────────────────────────────────────────────────────────
    DocSection { id: "installing", title: "Installing OpenShift Virtualization", dir: "virt/install", file: "installing-virt.adoc", tags: &["install", "operator", "subscription"] },
    DocSection { id: "requirements", title: "Requirements", dir: "virt/install", file: "virt-requirements.adoc", tags: &["requirements", "prerequisites", "hardware"] },
    DocSection { id: "preparing-cluster", title: "Preparing Cluster for Virtualization", dir: "virt/install", file: "preparing-cluster-for-virt.adoc", tags: &["install", "preparation", "cluster"] },
    DocSection { id: "uninstalling", title: "Uninstalling OpenShift Virtualization", dir: "virt/install", file: "uninstalling-virt.adoc", tags: &["uninstall", "remove"] },

    // ── Getting started ──────────────────────────────────────────────────
    DocSection { id: "getting-started", title: "Getting Started", dir: "virt/getting_started", file: "virt-getting-started.adoc", tags: &["quickstart", "getting-started", "tutorial"] },
    DocSection { id: "cli-tools", title: "Using CLI Tools", dir: "virt/getting_started", file: "virt-using-the-cli-tools.adoc", tags: &["cli", "virtctl", "oc"] },

    // ── Creating VMs ─────────────────────────────────────────────────────
    DocSection { id: "vm-from-instancetypes", title: "Creating VMs from Instance Types", dir: "virt/creating_vm", file: "virt-creating-vms-from-instance-types.adoc", tags: &["create", "instancetype", "catalog"] },
    DocSection { id: "vm-from-templates", title: "Creating VMs from Templates", dir: "virt/creating_vm", file: "virt-creating-vms-from-templates.adoc", tags: &["create", "templates", "catalog"] },
    DocSection { id: "vm-from-cli", title: "Creating VMs from CLI", dir: "virt/creating_vms_advanced", file: "virt-creating-vms-from-cli.adoc", tags: &["create", "cli", "yaml", "manifest"] },
    DocSection { id: "vm-from-container-disks", title: "Creating VMs from Container Disks", dir: "virt/creating_vms_advanced", file: "virt-creating-vms-from-container-disks.adoc", tags: &["create", "container-disk", "registry"] },
    DocSection { id: "vm-from-web-images", title: "Creating VMs from Web Images", dir: "virt/creating_vms_advanced", file: "virt-creating-vms-from-web-images.adoc", tags: &["create", "web-image", "url", "import"] },
    DocSection { id: "vm-uploading-images", title: "Uploading Images for VMs", dir: "virt/creating_vms_advanced", file: "virt-creating-vms-uploading-images.adoc", tags: &["create", "upload", "image"] },
    DocSection { id: "vm-cloning", title: "Cloning VMs", dir: "virt/creating_vms_advanced", file: "virt-cloning-vms.adoc", tags: &["clone", "copy"] },
    DocSection { id: "vm-cloning-pvcs", title: "Creating VMs by Cloning PVCs", dir: "virt/creating_vms_advanced", file: "virt-creating-vms-by-cloning-pvcs.adoc", tags: &["clone", "pvc", "storage"] },
    DocSection { id: "vm-rh-images", title: "Creating VMs from Red Hat Images Overview", dir: "virt/creating_vms_advanced", file: "virt-creating-vms-from-rh-images-overview.adoc", tags: &["create", "rhel", "golden-image", "boot-source"] },
    DocSection { id: "vm-golden-image-heterogeneous", title: "Golden Images on Heterogeneous Clusters", dir: "virt/creating_vms_advanced", file: "virt-golden-image-heterogeneous-clusters.adoc", tags: &["golden-image", "heterogeneous", "multi-arch"] },

    // ── Managing VMs ─────────────────────────────────────────────────────
    DocSection { id: "vm-edit", title: "Editing VMs", dir: "virt/managing_vms", file: "virt-edit-vms.adoc", tags: &["edit", "modify", "vm-details"] },
    DocSection { id: "vm-controlling-states", title: "Controlling VM States", dir: "virt/managing_vms", file: "virt-controlling-vm-states.adoc", tags: &["start", "stop", "pause", "restart", "run-strategy"] },
    DocSection { id: "vm-consoles", title: "Accessing VM Consoles", dir: "virt/managing_vms", file: "virt-accessing-vm-consoles.adoc", tags: &["console", "vnc", "serial", "rdp"] },
    DocSection { id: "vm-ssh", title: "Accessing VMs via SSH", dir: "virt/managing_vms", file: "virt-accessing-vm-ssh.adoc", tags: &["ssh", "access", "nodeport"] },
    DocSection { id: "vm-delete", title: "Deleting VMs", dir: "virt/managing_vms", file: "virt-delete-vms.adoc", tags: &["delete", "remove"] },
    DocSection { id: "vm-delete-protection", title: "VM Delete Protection", dir: "virt/managing_vms", file: "virt-enabling-disabling-vm-delete-protection.adoc", tags: &["delete-protection", "safety"] },
    DocSection { id: "vm-boot-order", title: "Editing Boot Order", dir: "virt/managing_vms", file: "virt-edit-boot-order.adoc", tags: &["boot-order", "boot", "disk-order"] },
    DocSection { id: "vm-export", title: "Exporting VMs", dir: "virt/managing_vms", file: "virt-exporting-vms.adoc", tags: &["export", "backup"] },
    DocSection { id: "vm-list", title: "Listing VMs", dir: "virt/managing_vms", file: "virt-list-vms.adoc", tags: &["list", "overview"] },
    DocSection { id: "vm-vmis", title: "Managing VMIs", dir: "virt/managing_vms", file: "virt-manage-vmis.adoc", tags: &["vmi", "instance", "runtime"] },
    DocSection { id: "vm-pipelines", title: "Managing VMs with OpenShift Pipelines", dir: "virt/managing_vms", file: "virt-managing-vms-openshift-pipelines.adoc", tags: &["pipelines", "tekton", "ci-cd"] },
    DocSection { id: "vm-storage-migration", title: "Migrating VMs to Different Storage Class", dir: "virt/managing_vms", file: "virt-migrating-vms-in-single-cluster-to-different-storage-class.adoc", tags: &["storage-migration", "storage-class", "migrate"] },
    DocSection { id: "vm-vtpm", title: "Using vTPM Devices", dir: "virt/managing_vms", file: "virt-using-vtpm-devices.adoc", tags: &["vtpm", "tpm", "security"] },
    DocSection { id: "vm-virtio-drivers", title: "Installing VirtIO Drivers on Windows", dir: "virt/managing_vms", file: "virt-install-virtio-drivers-on-windows-vms.adoc", tags: &["windows", "virtio", "drivers"] },
    DocSection { id: "vm-guest-agent", title: "Installing QEMU Guest Agent", dir: "virt/managing_vms", file: "virt-installing-qemu-guest-agent.adoc", tags: &["guest-agent", "qemu", "monitoring"] },
    DocSection { id: "vm-customize-console", title: "Customizing the Web Console", dir: "virt/managing_vms", file: "virt-customize-web-console.adoc", tags: &["web-console", "customize", "ui"] },

    // ── Networking ────────────────────────────────────────────────────────
    DocSection { id: "networking-overview", title: "Networking Overview", dir: "virt/vm_networking", file: "virt-networking-overview.adoc", tags: &["networking", "overview", "multus", "ovn"] },
    DocSection { id: "networking-pod-network", title: "Connecting VM to Default Pod Network", dir: "virt/vm_networking", file: "virt-connecting-vm-to-default-pod-network.adoc", tags: &["networking", "pod-network", "masquerade"] },
    DocSection { id: "networking-linux-bridge", title: "Connecting VM to Linux Bridge", dir: "virt/vm_networking", file: "virt-connecting-vm-to-linux-bridge.adoc", tags: &["networking", "linux-bridge", "bridge"] },
    DocSection { id: "networking-sriov", title: "Connecting VM to SR-IOV", dir: "virt/vm_networking", file: "virt-connecting-vm-to-sriov.adoc", tags: &["networking", "sriov", "sr-iov", "passthrough"] },
    DocSection { id: "networking-ovn-secondary", title: "Connecting VM to OVN Secondary Network", dir: "virt/vm_networking", file: "virt-connecting-vm-to-ovn-secondary-network.adoc", tags: &["networking", "ovn", "secondary", "layer2"] },
    DocSection { id: "networking-primary-udn", title: "Connecting VM to Primary UDN", dir: "virt/vm_networking", file: "virt-connecting-vm-to-primary-udn.adoc", tags: &["networking", "udn", "primary"] },
    DocSection { id: "networking-secondary-udn", title: "Connecting VM to Secondary UDN", dir: "virt/vm_networking", file: "virt-connecting-vm-to-secondary-udn.adoc", tags: &["networking", "udn", "secondary"] },
    DocSection { id: "networking-service-mesh", title: "Connecting VM to Service Mesh", dir: "virt/vm_networking", file: "virt-connecting-vm-to-service-mesh.adoc", tags: &["networking", "service-mesh", "istio"] },
    DocSection { id: "networking-expose-service", title: "Exposing VM with a Service", dir: "virt/vm_networking", file: "virt-exposing-vm-with-service.adoc", tags: &["networking", "service", "nodeport", "loadbalancer"] },
    DocSection { id: "networking-hotplug", title: "Hot-Plugging Network Interfaces", dir: "virt/vm_networking", file: "virt-hot-plugging-network-interfaces.adoc", tags: &["networking", "hotplug", "nic"] },
    DocSection { id: "networking-ips", title: "Configuring and Viewing IPs for VMs", dir: "virt/vm_networking", file: "virt-configuring-viewing-ips-for-vms.adoc", tags: &["networking", "ip", "address"] },
    DocSection { id: "networking-physical", title: "Configuring Physical Networks", dir: "virt/vm_networking", file: "virt-configuring-physical-networks.adoc", tags: &["networking", "physical", "nmstate", "nncp"] },
    DocSection { id: "networking-fqdn", title: "Accessing VM Internal FQDN", dir: "virt/vm_networking", file: "virt-accessing-vm-internal-fqdn.adoc", tags: &["networking", "dns", "fqdn"] },
    DocSection { id: "networking-secondary-fqdn", title: "Accessing VM Secondary Network FQDN", dir: "virt/vm_networking", file: "virt-accessing-vm-secondary-network-fqdn.adoc", tags: &["networking", "dns", "fqdn", "secondary"] },
    DocSection { id: "networking-dpdk", title: "Using DPDK with SR-IOV", dir: "virt/vm_networking", file: "virt-using-dpdk-with-sriov.adoc", tags: &["networking", "dpdk", "sriov", "performance"] },
    DocSection { id: "networking-mac-pool", title: "Using MAC Address Pool for VMs", dir: "virt/vm_networking", file: "virt-using-mac-address-pool-for-vms.adoc", tags: &["networking", "mac", "address-pool"] },
    DocSection { id: "networking-link-state", title: "Setting Interface Link State", dir: "virt/vm_networking", file: "virt-setting-interface-link-state.adoc", tags: &["networking", "link-state", "interface"] },
    DocSection { id: "networking-migration-network", title: "Dedicated Network for Live Migration", dir: "virt/vm_networking", file: "virt-dedicated-network-live-migration.adoc", tags: &["networking", "migration", "dedicated-network"] },

    // ── Storage ──────────────────────────────────────────────────────────
    DocSection { id: "storage-overview", title: "Storage Configuration Overview", dir: "virt/storage", file: "virt-storage-config-overview.adoc", tags: &["storage", "overview", "cdi"] },
    DocSection { id: "storage-profile", title: "Configuring Storage Profile", dir: "virt/storage", file: "virt-configuring-storage-profile.adoc", tags: &["storage", "storage-profile", "access-mode", "volume-mode"] },
    DocSection { id: "storage-local-hpp", title: "Configuring Local Storage with HPP", dir: "virt/storage", file: "virt-configuring-local-storage-with-hpp.adoc", tags: &["storage", "local", "hpp", "hostpath"] },
    DocSection { id: "storage-bootsource", title: "Automatic Boot Source Updates", dir: "virt/storage", file: "virt-automatic-bootsource-updates.adoc", tags: &["storage", "boot-source", "golden-image", "auto-update"] },
    DocSection { id: "storage-clone-permissions", title: "Enabling User Permissions to Clone DataVolumes", dir: "virt/storage", file: "virt-enabling-user-permissions-to-clone-datavolumes.adoc", tags: &["storage", "clone", "rbac", "datavolume"] },
    DocSection { id: "storage-cdi-resourcequota", title: "Configuring CDI for Namespace ResourceQuota", dir: "virt/storage", file: "virt-configuring-cdi-for-namespace-resourcequota.adoc", tags: &["storage", "cdi", "resourcequota", "quota"] },
    DocSection { id: "storage-scratch-space", title: "Preparing CDI Scratch Space", dir: "virt/storage", file: "virt-preparing-cdi-scratch-space.adoc", tags: &["storage", "cdi", "scratch"] },
    DocSection { id: "storage-fs-overhead", title: "Reserving PVC Space for Filesystem Overhead", dir: "virt/storage", file: "virt-reserving-pvc-space-fs-overhead.adoc", tags: &["storage", "filesystem", "overhead", "pvc"] },
    DocSection { id: "storage-preallocation", title: "Using Preallocation for DataVolumes", dir: "virt/storage", file: "virt-using-preallocation-for-datavolumes.adoc", tags: &["storage", "preallocation", "performance"] },
    DocSection { id: "storage-dv-annotations", title: "Managing Data Volume Annotations", dir: "virt/storage", file: "virt-managing-data-volume-annotations.adoc", tags: &["storage", "datavolume", "annotations"] },
    DocSection { id: "storage-csi", title: "Storage with CSI Paradigm", dir: "virt/storage", file: "virt-storage-with-csi-paradigm.adoc", tags: &["storage", "csi", "container-storage-interface"] },

    // ── Live migration ───────────────────────────────────────────────────
    DocSection { id: "migration-about", title: "About Live Migration", dir: "virt/live_migration", file: "virt-about-live-migration.adoc", tags: &["migration", "live-migration", "overview"] },
    DocSection { id: "migration-configuring", title: "Configuring Live Migration", dir: "virt/live_migration", file: "virt-configuring-live-migration.adoc", tags: &["migration", "configuration", "policy", "limits"] },
    DocSection { id: "migration-initiating", title: "Initiating Live Migration", dir: "virt/live_migration", file: "virt-initiating-live-migration.adoc", tags: &["migration", "initiate", "trigger"] },
    DocSection { id: "migration-cross-cluster", title: "Configuring Cross-Cluster Live Migration Network", dir: "virt/live_migration", file: "virt-configuring-cross-cluster-live-migration-network.adoc", tags: &["migration", "cross-cluster", "network"] },
    DocSection { id: "migration-mtv-providers", title: "About MTV Providers", dir: "virt/live_migration", file: "virt-about-mtv-providers.adoc", tags: &["migration", "mtv", "providers", "vmware"] },

    // ── Monitoring ────────────────────────────────────────────────────────
    DocSection { id: "monitoring-overview", title: "Monitoring Overview", dir: "virt/monitoring", file: "virt-monitoring-overview.adoc", tags: &["monitoring", "overview", "dashboard"] },
    DocSection { id: "monitoring-vm-health", title: "Monitoring VM Health", dir: "virt/monitoring", file: "virt-monitoring-vm-health.adoc", tags: &["monitoring", "health", "liveness", "readiness"] },
    DocSection { id: "monitoring-prometheus", title: "Prometheus Queries for VMs", dir: "virt/monitoring", file: "virt-prometheus-queries.adoc", tags: &["monitoring", "prometheus", "metrics", "queries"] },
    DocSection { id: "monitoring-custom-metrics", title: "Exposing Custom Metrics for VMs", dir: "virt/monitoring", file: "virt-exposing-custom-metrics-for-vms.adoc", tags: &["monitoring", "metrics", "custom"] },
    DocSection { id: "monitoring-downward-metrics", title: "Exposing Downward Metrics", dir: "virt/monitoring", file: "virt-exposing-downward-metrics.adoc", tags: &["monitoring", "downward-metrics", "guest"] },
    DocSection { id: "monitoring-runbooks", title: "Runbooks", dir: "virt/monitoring", file: "virt-runbooks.adoc", tags: &["monitoring", "runbooks", "alerts", "troubleshooting"] },
    DocSection { id: "checkups-network", title: "Running Cluster Checkups", dir: "virt/monitoring", file: "virt-running-cluster-checkups.adoc", tags: &["checkups", "network", "validation"] },
    DocSection { id: "checkups-storage", title: "Storage Checkups", dir: "virt/monitoring", file: "virt-storage-checkups.adoc", tags: &["checkups", "storage", "validation"] },

    // ── Nodes ────────────────────────────────────────────────────────────
    DocSection { id: "nodes-eviction", title: "Eviction Strategies", dir: "virt/nodes", file: "virt-eviction-strategies.adoc", tags: &["nodes", "eviction", "run-strategy", "drain"] },
    DocSection { id: "nodes-maintenance", title: "Node Maintenance", dir: "virt/nodes", file: "virt-node-maintenance.adoc", tags: &["nodes", "maintenance", "drain", "cordon"] },
    DocSection { id: "nodes-cpu-models", title: "Managing Node Labeling for Obsolete CPU Models", dir: "virt/nodes", file: "virt-managing-node-labeling-obsolete-cpu-models.adoc", tags: &["nodes", "cpu", "labeling", "models"] },
    DocSection { id: "nodes-ksm", title: "Activating KSM", dir: "virt/nodes", file: "virt-activating-ksm.adoc", tags: &["nodes", "ksm", "memory", "deduplication"] },
    DocSection { id: "nodes-reconciliation", title: "Preventing Node Reconciliation", dir: "virt/nodes", file: "virt-preventing-node-reconciliation.adoc", tags: &["nodes", "reconciliation"] },

    // ── Post-installation ────────────────────────────────────────────────
    DocSection { id: "post-install-config", title: "Post-Installation Configuration", dir: "virt/post_installation_configuration", file: "virt-post-install-config.adoc", tags: &["post-install", "configuration", "hco"] },
    DocSection { id: "post-install-network", title: "Post-Installation Network Configuration", dir: "virt/post_installation_configuration", file: "virt-post-install-network-config.adoc", tags: &["post-install", "networking"] },
    DocSection { id: "post-install-storage", title: "Post-Installation Storage Configuration", dir: "virt/post_installation_configuration", file: "virt-post-install-storage-config.adoc", tags: &["post-install", "storage"] },
    DocSection { id: "post-install-node-placement", title: "Node Placement for Virt Components", dir: "virt/post_installation_configuration", file: "virt-node-placement-virt-components.adoc", tags: &["post-install", "node-placement", "affinity"] },
    DocSection { id: "post-install-density", title: "Configuring Higher VM Workload Density", dir: "virt/post_installation_configuration", file: "virt-configuring-higher-vm-workload-density.adoc", tags: &["post-install", "density", "overcommit", "memory"] },
    DocSection { id: "post-install-cert-rotation", title: "Configuring Certificate Rotation", dir: "virt/post_installation_configuration", file: "virt-configuring-certificate-rotation.adoc", tags: &["post-install", "certificates", "tls"] },
    DocSection { id: "post-install-physical-cores", title: "Physical Cores Allocation for VMs", dir: "virt/post_installation_configuration", file: "virt-physical-cores-allocation-vms.adoc", tags: &["post-install", "cpu", "cores", "dedicated"] },
    DocSection { id: "post-install-redfish", title: "KubeVirt Redfish", dir: "virt/post_installation_configuration", file: "virt-kubevirt-redfish.adoc", tags: &["post-install", "redfish", "bmc", "ipmi"] },

    // ── Backup / restore ─────────────────────────────────────────────────
    DocSection { id: "backup-overview", title: "Backup and Restore Overview", dir: "virt/backup_restore", file: "virt-backup-restore-overview.adoc", tags: &["backup", "restore", "oadp"] },
    DocSection { id: "backup-snapshots", title: "Backup and Restore with Snapshots", dir: "virt/backup_restore", file: "virt-backup-restore-snapshots.adoc", tags: &["backup", "snapshots", "restore"] },
    DocSection { id: "disaster-recovery", title: "Disaster Recovery", dir: "virt/backup_restore", file: "virt-disaster-recovery.adoc", tags: &["disaster-recovery", "dr", "failover"] },

    // ── Support ──────────────────────────────────────────────────────────
    DocSection { id: "support-overview", title: "Support Overview", dir: "virt/support", file: "virt-support-overview.adoc", tags: &["support", "overview", "web-console"] },
    DocSection { id: "troubleshooting", title: "Troubleshooting", dir: "virt/support", file: "virt-troubleshooting.adoc", tags: &["support", "troubleshooting", "debug"] },
    DocSection { id: "collecting-data", title: "Collecting Virtualization Data", dir: "virt/support", file: "virt-collecting-virt-data.adoc", tags: &["support", "must-gather", "logs", "data-collection"] },

    // ── Release notes (latest only) ──────────────────────────────────────
    DocSection { id: "release-notes-4-22", title: "Release Notes 4.22", dir: "virt/release_notes", file: "virt-4-22-release-notes.adoc", tags: &["release-notes", "4.22", "new-features", "changes"] },
    DocSection { id: "release-notes-4-21", title: "Release Notes 4.21", dir: "virt/release_notes", file: "virt-4-21-release-notes.adoc", tags: &["release-notes", "4.21", "new-features", "changes"] },

    // ── Updating ─────────────────────────────────────────────────────────
    DocSection { id: "upgrading", title: "Upgrading OpenShift Virtualization", dir: "virt/updating", file: "upgrading-virt.adoc", tags: &["upgrade", "update", "version"] },
];

pub fn find_section(id: &str) -> Option<&'static DocSection> {
    SECTIONS.iter().find(|s| s.id == id)
}

pub fn search_sections(filter: &str) -> Vec<&'static DocSection> {
    let terms: Vec<String> = filter.split_whitespace().map(|t| t.to_lowercase()).collect();
    SECTIONS.iter().filter(|s| {
        let haystack = format!("{} {} {}", s.title, s.id, s.tags.join(" ")).to_lowercase();
        terms.iter().all(|t| haystack.contains(t))
    }).collect()
}
