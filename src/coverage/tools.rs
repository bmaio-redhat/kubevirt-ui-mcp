use serde_json::{json, Value};

use crate::cluster_inspector::KubeClient;
use crate::config::Config;
use crate::coverage::scanner::ProjectScanner;
use crate::mcp::protocol::ToolCallResult;

pub fn all_tools() -> Vec<Value> {
    vec![
        json!({
            "name": "get_coverage_for_feature",
            "description": "Find all test specs, step drivers, page objects, and STD docs that cover a given feature area. Returns Jira ticket IDs extracted from test annotations.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "feature": { "type": "string", "description": "Feature area to search for (e.g. \"bootable-volumes\", \"vm-actions\", \"catalog\", \"templates\", \"networking\", \"checkups\", \"overview\", \"quotas\")" }
                },
                "required": ["feature"]
            }
        }),
        json!({
            "name": "get_untested_step_driver_methods",
            "description": "Find public step driver methods that are never called from any spec file or other step driver. Helps identify dead code or missing test coverage.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "get_orphan_page_object_methods",
            "description": "Find public page object methods never referenced by any step driver or test. Candidates for removal or missing step driver integration.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "get_tier_distribution",
            "description": "Get a breakdown of tests by tier (gating, tier1, tier2, fleet-virtualization-acm) with file counts, test counts, and Jira IDs per tier.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "find_tests_by_jira",
            "description": "Look up which tests cover a given Jira ticket by searching for ID(CNV-XXXXX) annotations in spec files.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "ticket_id": { "type": "string", "description": "Jira ticket ID (e.g. \"CNV-78882\")" }
                },
                "required": ["ticket_id"]
            }
        }),
        json!({
            "name": "invalidate_cache",
            "description": "Clear the scanner cache. Call this after making changes to the playwright codebase to get fresh results from coverage tools.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "get_cluster_info",
            "description": "Get cluster version information: Kubernetes version, KubeVirt version, CNV operator version, CDI version, node count.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "list_vms",
            "description": "List virtual machines in a namespace (or all namespaces) with status, CPU, memory, and run strategy.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "description": "Namespace to list VMs from. Omit for all namespaces." }
                }
            }
        }),
        json!({
            "name": "get_vm_detail",
            "description": "Get detailed information about a specific VM: spec, status, conditions, volumes, networks, interfaces, and VMI runtime info (IPs, node, guest OS).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "VM name" },
                    "namespace": { "type": "string", "description": "VM namespace" }
                },
                "required": ["name", "namespace"]
            }
        }),
        json!({
            "name": "list_test_namespaces",
            "description": "List all pw-* test namespaces with age and status. Helps identify stale namespaces from previous test runs.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "cleanup_stale_namespaces",
            "description": "Delete pw-* test namespaces older than a threshold. Defaults to 4 hours.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "older_than_hours": { "type": "number", "description": "Delete namespaces older than this many hours (default: 4)" }
                }
            }
        }),
        json!({
            "name": "check_cluster_health",
            "description": "Pre-flight cluster health check: API server reachability, CNV operator status, virt-api pods, storage classes, node readiness, test namespace count.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "scaffold_test",
            "description": "Generate a .spec.ts test file skeleton following project conventions. Includes proper imports, test.describe, Allure registration, cleanup tracking, and ID(CNV-XXXXX) annotations. Returns the file path and content -- does NOT write to disk.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "feature": { "type": "string", "description": "Feature name in kebab-case or natural language (e.g. \"storage-migration\", \"vm-snapshots\")" },
                    "tier": { "type": "string", "enum": ["gating", "tier1", "tier2"], "description": "Test tier" },
                    "describe_name": { "type": "string", "description": "Custom test.describe title. Defaults to PascalCase of feature." },
                    "jira_ids": { "type": "array", "items": { "type": "string" }, "description": "Jira ticket IDs to annotate" },
                    "tags": { "type": "array", "items": { "type": "string" }, "description": "Additional tags beyond the tier tag" },
                    "use_shared_resources": { "type": "boolean", "description": "Generate read-only test pattern using sharedResources fixture" }
                },
                "required": ["feature", "tier"]
            }
        }),
        json!({
            "name": "scaffold_page_object",
            "description": "Generate a page object class extending BasePage or PageCommons with project conventions. Returns the file path and content -- does NOT write to disk.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Page object name (e.g. \"storage-migration\", \"vm-snapshots\"). \"Page\" suffix added automatically." },
                    "base_class": { "type": "string", "enum": ["BasePage", "PageCommons"], "description": "Base class to extend (default: PageCommons)" },
                    "url_pattern": { "type": "string", "description": "URL pattern for navigation methods. Use {namespace} as placeholder" }
                },
                "required": ["name"]
            }
        }),
        json!({
            "name": "scaffold_step_driver",
            "description": "Generate a StepDriver class wired to a page object following the BasePageStepDriver pattern. Returns the file path and content -- does NOT write to disk.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "feature": { "type": "string", "description": "Feature name (e.g. \"storage-migration\"). Used for class name and file path." },
                    "page_object_name": { "type": "string", "description": "Page object class name to bind. Defaults to PascalCase(feature) + \"Page\"." }
                },
                "required": ["feature"]
            }
        }),
        json!({
            "name": "scaffold_std",
            "description": "Generate a Software Test Description (STD) document from the project template. Returns the file path and content -- does NOT write to disk.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "feature": { "type": "string", "description": "Feature name (e.g. \"storage-migration\")" },
                    "tier": { "type": "string", "enum": ["gating", "tier1", "tier2"], "description": "Test tier for the STD" },
                    "jira_ids": { "type": "array", "items": { "type": "string" }, "description": "Related Jira ticket IDs" }
                },
                "required": ["feature", "tier"]
            }
        }),
        json!({
            "name": "run_tests",
            "description": "Execute Playwright tests with structured parameters. Builds the correct `yarn test-playwright` command from the inputs. Use dryRun to preview the command without executing. Runs from the project root.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "file": { "type": "string", "description": "Test file or glob to run" },
                    "grep": { "type": "string", "description": "Tag or name filter, passed to --grep" },
                    "grep_invert": { "type": "string", "description": "Exclude tests matching this pattern" },
                    "workers": { "type": "number", "description": "Number of parallel workers" },
                    "retries": { "type": "number", "description": "Number of retries for failed tests" },
                    "headed": { "type": "boolean", "description": "Run in headed mode" },
                    "debug": { "type": "boolean", "description": "Enable debug mode" },
                    "timeout": { "type": "number", "description": "Test timeout in milliseconds" },
                    "shard": { "type": "string", "description": "Shard specification (e.g. \"1/4\")" },
                    "skip_cleanup": { "type": "boolean", "description": "Skip test resource cleanup" },
                    "dry_run": { "type": "boolean", "description": "Return the command without executing" }
                }
            }
        }),
        json!({
            "name": "get_test_results",
            "description": "Parse the latest test results from JUnit XML or Allure result files. Returns pass/fail counts, failed test names with error messages, and execution time.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": { "type": "string", "enum": ["junit", "allure"], "description": "Which result source to parse. Omit to auto-detect." }
                }
            }
        }),
        json!({
            "name": "get_pr_details",
            "description": "Get comprehensive PR information in a single call: metadata, files changed, CI check status.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "pr_number": { "type": "number", "description": "Pull request number" },
                    "repo": { "type": "string", "description": "GitHub repo in owner/repo format. Falls back to GITHUB_REPO env var." }
                },
                "required": ["pr_number"]
            }
        }),
        json!({
            "name": "get_pr_files_coverage",
            "description": "Cross-reference a PR's changed files with test coverage. Shows which playwright specs, page objects, and step drivers are changed.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "pr_number": { "type": "number", "description": "Pull request number" },
                    "repo": { "type": "string", "description": "GitHub repo in owner/repo format. Falls back to GITHUB_REPO env var." }
                },
                "required": ["pr_number"]
            }
        }),
        json!({
            "name": "get_pr_comments",
            "description": "Get all review comments (inline code comments) and issue comments (general discussion) for a PR.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "pr_number": { "type": "number", "description": "Pull request number" },
                    "repo": { "type": "string", "description": "GitHub repo in owner/repo format. Falls back to GITHUB_REPO env var." }
                },
                "required": ["pr_number"]
            }
        }),
        json!({
            "name": "list_open_prs",
            "description": "List open pull requests with optional filters for author and label.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "author": { "type": "string", "description": "Filter by PR author" },
                    "label": { "type": "string", "description": "Filter by label name" },
                    "limit": { "type": "number", "description": "Max results to return (default: 20)" },
                    "repo": { "type": "string", "description": "GitHub repo in owner/repo format. Falls back to GITHUB_REPO env var." }
                }
            }
        }),
        json!({
            "name": "search_prs",
            "description": "Search pull requests by keyword across title, body, and comments. Supports GitHub search qualifiers.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query (supports GitHub search qualifiers)" },
                    "limit": { "type": "number", "description": "Max results to return (default: 20)" },
                    "repo": { "type": "string", "description": "GitHub repo in owner/repo format. Falls back to GITHUB_REPO env var." }
                },
                "required": ["query"]
            }
        }),
    ]
}

pub async fn dispatch(
    name: &str,
    params: &Value,
    scanner: &mut ProjectScanner,
    kube_client: &KubeClient,
    cfg: &Config,
) -> ToolCallResult {
    match name {
        "get_coverage_for_feature" => {
            let feature = match params.get("feature").and_then(|v| v.as_str()) {
                Some(f) => f.to_string(),
                None => return ToolCallResult::error("Missing required parameter: feature"),
            };
            let result = crate::coverage::oracle::get_coverage_for_feature(scanner, &feature);
            ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap_or_default())
        }
        "get_untested_step_driver_methods" => {
            let result = crate::coverage::oracle::get_untested_step_driver_methods(scanner);
            ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap_or_default())
        }
        "get_orphan_page_object_methods" => {
            let result = crate::coverage::oracle::get_orphan_page_object_methods(scanner);
            ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap_or_default())
        }
        "get_tier_distribution" => {
            let result = crate::coverage::oracle::get_tier_distribution(scanner);
            ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap_or_default())
        }
        "find_tests_by_jira" => {
            let ticket_id = match params.get("ticket_id").and_then(|v| v.as_str()) {
                Some(t) => t.to_string(),
                None => return ToolCallResult::error("Missing required parameter: ticket_id"),
            };
            let result = crate::coverage::oracle::find_tests_by_jira(scanner, &ticket_id);
            ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap_or_default())
        }
        "invalidate_cache" => {
            scanner.invalidate_cache();
            ToolCallResult::text("Scanner cache cleared.")
        }
        "get_cluster_info" => {
            let result = crate::coverage::cluster::get_cluster_info(kube_client).await;
            ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap_or_default())
        }
        "list_vms" => {
            let namespace = params.get("namespace").and_then(|v| v.as_str());
            let result = crate::coverage::cluster::list_vms(kube_client, namespace).await;
            ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap_or_default())
        }
        "get_vm_detail" => {
            let name = match params.get("name").and_then(|v| v.as_str()) {
                Some(n) => n.to_string(),
                None => return ToolCallResult::error("Missing required parameter: name"),
            };
            let namespace = match params.get("namespace").and_then(|v| v.as_str()) {
                Some(n) => n.to_string(),
                None => return ToolCallResult::error("Missing required parameter: namespace"),
            };
            let result = crate::coverage::cluster::get_vm_detail(kube_client, &namespace, &name).await;
            ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap_or_default())
        }
        "list_test_namespaces" => {
            let result = crate::coverage::cluster::list_test_namespaces(kube_client).await;
            ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap_or_default())
        }
        "cleanup_stale_namespaces" => {
            let hours = params.get("older_than_hours").and_then(|v| v.as_f64()).unwrap_or(4.0);
            let result = crate::coverage::cluster::cleanup_stale_namespaces(kube_client, hours).await;
            ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap_or_default())
        }
        "check_cluster_health" => {
            let result = crate::coverage::cluster::check_cluster_health(kube_client).await;
            ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap_or_default())
        }
        "scaffold_test" => {
            let result = crate::coverage::scaffolder::scaffold_test(params);
            ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap_or_default())
        }
        "scaffold_page_object" => {
            let result = crate::coverage::scaffolder::scaffold_page_object(params);
            ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap_or_default())
        }
        "scaffold_step_driver" => {
            let result = crate::coverage::scaffolder::scaffold_step_driver(params);
            ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap_or_default())
        }
        "scaffold_std" => {
            let result = crate::coverage::scaffolder::scaffold_std(params);
            ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap_or_default())
        }
        "run_tests" => {
            let result = crate::coverage::runner::run_tests(params, cfg).await;
            ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap_or_default())
        }
        "get_test_results" => {
            let result = crate::coverage::runner::get_test_results(params, cfg);
            ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap_or_default())
        }
        "get_pr_details" => {
            let pr_number = match params.get("pr_number").and_then(|v| v.as_u64()) {
                Some(n) => n,
                None => return ToolCallResult::error("Missing required parameter: pr_number"),
            };
            let repo = params.get("repo").and_then(|v| v.as_str());
            let result = crate::coverage::github::get_pr_details(repo, pr_number, &cfg.github_repo);
            ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap_or_default())
        }
        "get_pr_files_coverage" => {
            let pr_number = match params.get("pr_number").and_then(|v| v.as_u64()) {
                Some(n) => n,
                None => return ToolCallResult::error("Missing required parameter: pr_number"),
            };
            let repo = params.get("repo").and_then(|v| v.as_str());
            let result = crate::coverage::github::get_pr_files_coverage(repo, pr_number, scanner, &cfg.github_repo);
            ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap_or_default())
        }
        "get_pr_comments" => {
            let pr_number = match params.get("pr_number").and_then(|v| v.as_u64()) {
                Some(n) => n,
                None => return ToolCallResult::error("Missing required parameter: pr_number"),
            };
            let repo = params.get("repo").and_then(|v| v.as_str());
            let result = crate::coverage::github::get_pr_comments(repo, pr_number, &cfg.github_repo);
            ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap_or_default())
        }
        "list_open_prs" => {
            let repo = params.get("repo").and_then(|v| v.as_str());
            let author = params.get("author").and_then(|v| v.as_str());
            let label = params.get("label").and_then(|v| v.as_str());
            let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as u32;
            let result = crate::coverage::github::list_open_prs(repo, author, label, limit, &cfg.github_repo);
            ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap_or_default())
        }
        "search_prs" => {
            let query = match params.get("query").and_then(|v| v.as_str()) {
                Some(q) => q.to_string(),
                None => return ToolCallResult::error("Missing required parameter: query"),
            };
            let repo = params.get("repo").and_then(|v| v.as_str());
            let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as u32;
            let result = crate::coverage::github::search_prs(repo, &query, limit, &cfg.github_repo);
            ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap_or_default())
        }
        _ => ToolCallResult::error(format!("Unknown coverage tool: '{}'", name)),
    }
}
