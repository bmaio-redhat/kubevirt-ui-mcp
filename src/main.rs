mod ci_triage;
mod cluster_inspector;
mod config;
mod context;
mod coverage;
mod linter;
mod mcp;
mod memory;
mod oc;
mod spec;

use std::sync::Arc;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{error, info, warn};

use config::Config;
use context::{new_shared_index, SharedIndex};
use mcp::protocol::{
    Capabilities, InitializeResult, Request, Response, ServerInfo, ToolCallResult, ToolDef,
};
use memory::SharedStore;

// ── AppState ──────────────────────────────────────────────────────────────────

struct AppState {
    cfg: Config,
    index: SharedIndex,
    store: SharedStore,
    kube_client: Arc<cluster_inspector::KubeClient>,
    http_client: Arc<reqwest::Client>,
}

// ── Entry point ───────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_env("KUBEVIRT_MCP_LOG")
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let cfg = Config::from_env();
    info!("kubevirt-ui-mcp starting. project root: {}", cfg.project_root.display());

    let http_client = Arc::new(
        reqwest::Client::builder()
            .user_agent("kubevirt-ui-mcp/0.1 (Cursor MCP)")
            .build()
            .expect("HTTP client"),
    );

    // Build AST index
    let playwright_root = cfg.playwright_root.clone();
    let index = {
        let indexer = context::Indexer::new(playwright_root.clone());
        let idx = tokio::task::spawn_blocking(move || indexer.build())
            .await
            .unwrap_or_default();
        info!("Context index built: {} classes", idx.classes.len());
        new_shared_index(idx)
    };

    // Start file watcher for context index
    {
        let watch_dirs = vec![
            playwright_root.join("src/page-objects"),
            playwright_root.join("src/step-drivers"),
            playwright_root.join("src/fixtures"),
            playwright_root.join("src/clients"),
        ];
        context::spawn_async_watcher(watch_dirs, playwright_root, index.clone());
    }

    // Load memory store and start background refresh
    let store = memory::new_shared_store(&cfg);
    {
        let client = Arc::clone(&http_client);
        let store2 = store.clone();
        let cfg_clone = cfg.clone();
        tokio::spawn(async move {
            memory::run_refresh(&client, &store2, &cfg_clone).await;
        });
    }

    // Build kube client
    let kube_client = cluster_inspector::build_kube_client(&cfg);

    let state = Arc::new(AppState {
        cfg,
        index,
        store,
        kube_client,
        http_client,
    });

    run_server(state).await;
}

// ── MCP stdio server ──────────────────────────────────────────────────────────

async fn run_server(state: Arc<AppState>) {
    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut lines = BufReader::new(stdin).lines();

    info!("MCP stdio server ready.");

    while let Ok(Some(line)) = lines.next_line().await {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        let req: Request = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                error!("Failed to parse request: {}", e);
                let resp = Response::err(None, -32700, format!("Parse error: {}", e));
                send_response(&mut stdout, &resp).await;
                continue;
            }
        };

        // Notifications (no id) don't get a response
        if req.id.is_none() && req.method != "initialized" {
            continue;
        }

        let resp = handle_request(req, &state).await;
        if resp.id.is_some() || resp.error.is_some() {
            send_response(&mut stdout, &resp).await;
        }
    }

    info!("stdin closed, shutting down.");
}

async fn handle_request(req: Request, state: &Arc<AppState>) -> Response {
    let id = req.id.clone();

    match req.method.as_str() {
        "initialize" => {
            let result = InitializeResult {
                protocol_version: "2024-11-05".into(),
                capabilities: Capabilities { tools: json!({}) },
                server_info: ServerInfo {
                    name: "kubevirt-ui-mcp".into(),
                    version: env!("CARGO_PKG_VERSION").into(),
                },
            };
            Response::ok(id, serde_json::to_value(result).unwrap())
        }
        "initialized" => Response::ok(id, json!({})),
        "tools/list" => Response::ok(id, json!({ "tools": all_tools() })),
        "tools/call" => {
            let params = req.params.unwrap_or(Value::Null);
            let tool_name =
                params.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let tool_params = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| Value::Object(Default::default()));

            let result = dispatch_tool(&tool_name, &tool_params, state).await;
            Response::ok(id, result.into_value())
        }
        "ping" => Response::ok(id, json!({})),
        method => {
            warn!("Unknown method: {}", method);
            Response::err(id, -32601, format!("Method not found: {}", method))
        }
    }
}

// ── Tool registry ─────────────────────────────────────────────────────────────

fn all_tools() -> Vec<Value> {
    let mut tools: Vec<Value> = Vec::new();

    // Coverage / kubevirt-qe-mcp tools
    tools.extend(coverage::tools::all_tools());

    // Context tools (kubevirt-qe-context)
    tools.extend(context_tools());

    // Spec tools (playwright-spec-mcp)
    tools.extend(spec_tools());

    // Memory tools (kubevirt-memory)
    tools.extend(memory::tools::all_tools());

    // CI triage tools
    tools.extend(ci_triage_tools());

    // Cluster inspector tools
    tools.extend(cluster_inspector_tools());

    // oc/virtctl tools
    tools.extend(oc_tools());

    // Linter tools
    tools.extend(linter_tools());

    tools
}

fn context_tools() -> Vec<Value> {
    vec![
        tool_def("get_class_surface",
            "Returns only the public method signatures and JSDoc comments for a TypeScript class — page objects, step drivers, clients, or client handlers. Compresses a 4000+ line file into ~200 lines of API surface.",
            json!({ "type": "object", "properties": { "class_name": { "type": "string", "description": "Class name to look up. Partial match supported." }, "filter": { "type": "string", "description": "Optional keyword to filter methods by name or JSDoc content." } }, "required": ["class_name"] })),
        tool_def("get_selector_map",
            "Returns all data-test, data-test-id, and ARIA role selectors defined in a page object class.",
            json!({ "type": "object", "properties": { "class_name": { "type": "string", "description": "Page object class name. Partial match supported." } }, "required": ["class_name"] })),
        tool_def("get_task_context",
            "Given a task description, returns the minimal set of relevant method signatures, fixture properties, and import paths needed.",
            json!({ "type": "object", "properties": { "task": { "type": "string", "description": "Describe what you want to do." } }, "required": ["task"] })),
        tool_def("get_fixture_api",
            "Returns the compressed public interface of the main test fixture (scenario-test-fixture.ts) — just property names and types.",
            json!({ "type": "object", "properties": {} })),
        tool_def("get_import_guide",
            "Given one or more class/function/type names from the playwright framework, returns the correct relative import paths.",
            json!({ "type": "object", "properties": { "symbols": { "type": "array", "items": { "type": "string" }, "description": "Class/function/type names to look up." } }, "required": ["symbols"] })),
        tool_def("get_base_patterns",
            "Returns a minimal, correct code skeleton for common framework operations, synthesised from actual base classes.",
            json!({ "type": "object", "properties": { "pattern_type": { "type": "string", "enum": ["test-creation", "step-driver-extension", "page-object-extension", "api-test", "gating-test"] }, "feature": { "type": "string", "description": "Optional feature name to tailor the pattern." } }, "required": ["pattern_type"] })),
        tool_def("search_methods",
            "Full-text search over method names and JSDoc across all step drivers and page objects. Returns matching method signatures only.",
            json!({ "type": "object", "properties": { "query": { "type": "string" }, "scope": { "type": "string", "enum": ["step-drivers", "page-objects", "all"], "description": "Limit search scope. Defaults to 'all'." } }, "required": ["query"] })),
        tool_def("refresh_index",
            "Manually rebuild the in-memory AST index from disk. Call after bulk file changes or if results seem stale.",
            json!({ "type": "object", "properties": {} })),
    ]
}

fn spec_tools() -> Vec<Value> {
    vec![
        tool_def("list_std_docs",
            "List all STD (Software Test Description) markdown documents.",
            json!({ "type": "object", "properties": { "docs_root": { "type": "string" }, "filter": { "type": "string", "description": "Optional path substring filter (e.g. 'tier1', 'checkups')" } } })),
        tool_def("get_std_doc",
            "Get a specific STD document by relative path. Returns the full STD content with spec metadata appended.",
            json!({ "type": "object", "properties": { "doc": { "type": "string", "description": "Relative path to the STD doc, e.g. 'tier1/checkups.md'" }, "docs_root": { "type": "string" }, "root": { "type": "string" } }, "required": ["doc"] })),
        tool_def("list_spec_files",
            "List all Playwright spec files organised by tier, annotated with whether a matching STD doc exists.",
            json!({ "type": "object", "properties": { "root": { "type": "string" }, "docs_root": { "type": "string" } } })),
        tool_def("get_spec_markdown",
            "Get the STD document for a specific spec file (if one exists), with spec metadata appended.",
            json!({ "type": "object", "properties": { "path": { "type": "string", "description": "Absolute path to the .spec.ts file" }, "docs_root": { "type": "string" } }, "required": ["path"] })),
        tool_def("get_all_specs_markdown",
            "Get all STD documents (or spec metadata if no docs root) for a tier/feature.",
            json!({ "type": "object", "properties": { "docs_root": { "type": "string" }, "root": { "type": "string" }, "tier": { "type": "string", "description": "Tier filter: 'gating', 'tier1', 'tier2'" }, "feature": { "type": "string" } } })),
        tool_def("search_tests",
            "Search test cases by Jira ID, name keyword, or tag across all spec files.",
            json!({ "type": "object", "properties": { "query": { "type": "string", "description": "Search term: Jira ID (e.g. CNV-10789), keyword, or tag (e.g. @nonpriv)" }, "root": { "type": "string" }, "docs_root": { "type": "string" } }, "required": ["query"] })),
    ]
}

fn ci_triage_tools() -> Vec<Value> {
    vec![
        tool_def("parse_junit_report",
            "Parse a local JUnit XML report. Returns structured list of test results with spec path, test name, Jira IDs, status, error message.",
            json!({ "type": "object", "properties": { "path": { "type": "string", "description": "Path to JUnit XML file. Defaults to project junit-results dir." } } })),
        tool_def("parse_jenkins_report",
            "Fetch and parse a Jenkins test report from a build URL or local JSON file.",
            json!({ "type": "object", "properties": { "source": { "type": "string", "description": "Jenkins build URL or local JSON path" } }, "required": ["source"] })),
        tool_def("merge_quarantined",
            "Merge FAILED tests with matching quarantined SKIPPED entries into a unified failure list.",
            json!({ "type": "object", "properties": { "path": { "type": "string" } } })),
        tool_def("classify_failures",
            "Classify each failure as infrastructure, product_bug, test_bug, or flaky.",
            json!({ "type": "object", "properties": { "path": { "type": "string" } } })),
        tool_def("get_reproduce_command",
            "Emit the exact yarn test-playwright command to reproduce a failure.",
            json!({ "type": "object", "properties": { "spec_path": { "type": "string" }, "test_name": { "type": "string" }, "jira_id": { "type": "string" } }, "required": ["spec_path"] })),
        tool_def("get_allure_failures",
            "Scan the allure-results directory and return all failed test details.",
            json!({ "type": "object", "properties": { "allure_dir": { "type": "string" } } })),
        tool_def("get_failure_summary",
            "High-level summary of a test run: total/passed/failed/skipped counts, top failures, per-tier breakdown.",
            json!({ "type": "object", "properties": { "path": { "type": "string" } } })),
    ]
}

fn cluster_inspector_tools() -> Vec<Value> {
    vec![
        tool_def("get_resource",
            "Get a specific Kubernetes resource by group, version, kind, name, and optional namespace.",
            json!({ "type": "object", "properties": { "group": { "type": "string" }, "version": { "type": "string" }, "kind_plural": { "type": "string" }, "name": { "type": "string" }, "namespace": { "type": "string" } }, "required": ["version", "kind_plural", "name"] })),
        tool_def("list_resources",
            "List Kubernetes resources by group, version, kind, and optional namespace/label selector.",
            json!({ "type": "object", "properties": { "group": { "type": "string" }, "version": { "type": "string" }, "kind_plural": { "type": "string" }, "namespace": { "type": "string" }, "label_selector": { "type": "string" } }, "required": ["version", "kind_plural"] })),
        tool_def("get_hco_status",
            "Get the HyperConverged operator status including conditions, versions, and related objects.",
            json!({ "type": "object", "properties": { "name": { "type": "string" }, "namespace": { "type": "string" } } })),
        tool_def("get_vm_events",
            "Get events for a specific VirtualMachine.",
            json!({ "type": "object", "properties": { "name": { "type": "string" }, "namespace": { "type": "string" } }, "required": ["name", "namespace"] })),
        tool_def("get_storage_class_info",
            "List all StorageClasses with their provisioner, reclaim policy, and volume binding mode.",
            json!({ "type": "object", "properties": {} })),
        tool_def("get_node_status",
            "Get status of all nodes: Ready condition, roles, capacity, allocatable resources.",
            json!({ "type": "object", "properties": {} })),
        tool_def("get_namespace_inventory",
            "Get a count of all KubeVirt-related resources in a namespace.",
            json!({ "type": "object", "properties": { "namespace": { "type": "string" } }, "required": ["namespace"] })),
        tool_def("explain_stuck_namespace",
            "Diagnose why a namespace is stuck in Terminating state by listing resources with finalizers.",
            json!({ "type": "object", "properties": { "namespace": { "type": "string" } }, "required": ["namespace"] })),
    ]
}

fn oc_tools() -> Vec<Value> {
    vec![
        tool_def("oc_get",
            "Run oc get for a resource type, with optional name, namespace, and label selector. Returns JSON output.",
            json!({ "type": "object", "properties": { "resource": { "type": "string" }, "name": { "type": "string" }, "namespace": { "type": "string" }, "label_selector": { "type": "string" } }, "required": ["resource"] })),
        tool_def("oc_apply_yaml",
            "Apply a YAML manifest using oc apply. Accepts the raw YAML string.",
            json!({ "type": "object", "properties": { "yaml": { "type": "string" }, "namespace": { "type": "string" } }, "required": ["yaml"] })),
        tool_def("oc_delete",
            "Delete a Kubernetes resource by type and name.",
            json!({ "type": "object", "properties": { "resource": { "type": "string" }, "name": { "type": "string" }, "namespace": { "type": "string" }, "label_selector": { "type": "string" } }, "required": ["resource"] })),
        tool_def("oc_wait",
            "Wait for a condition on a resource using oc wait.",
            json!({ "type": "object", "properties": { "resource": { "type": "string" }, "name": { "type": "string" }, "condition": { "type": "string" }, "namespace": { "type": "string" }, "timeout": { "type": "string" } }, "required": ["resource", "condition"] })),
        tool_def("oc_logs",
            "Get logs from a pod or container.",
            json!({ "type": "object", "properties": { "pod": { "type": "string" }, "container": { "type": "string" }, "namespace": { "type": "string" }, "tail": { "type": "number" }, "since": { "type": "string" } }, "required": ["pod"] })),
        tool_def("oc_exec",
            "Execute a command inside a running pod.",
            json!({ "type": "object", "properties": { "pod": { "type": "string" }, "command": { "type": "array", "items": { "type": "string" } }, "container": { "type": "string" }, "namespace": { "type": "string" } }, "required": ["pod", "command"] })),
        tool_def("virtctl_migrate",
            "Live-migrate a VirtualMachineInstance to another node.",
            json!({ "type": "object", "properties": { "vm_name": { "type": "string" }, "namespace": { "type": "string" } }, "required": ["vm_name", "namespace"] })),
        tool_def("virtctl_pause",
            "Pause a VirtualMachineInstance.",
            json!({ "type": "object", "properties": { "vm_name": { "type": "string" }, "namespace": { "type": "string" } }, "required": ["vm_name", "namespace"] })),
        tool_def("virtctl_unpause",
            "Unpause a VirtualMachineInstance.",
            json!({ "type": "object", "properties": { "vm_name": { "type": "string" }, "namespace": { "type": "string" } }, "required": ["vm_name", "namespace"] })),
        tool_def("virtctl_ssh",
            "Open an SSH connection to a VirtualMachineInstance.",
            json!({ "type": "object", "properties": { "vm_name": { "type": "string" }, "namespace": { "type": "string" }, "username": { "type": "string" } }, "required": ["vm_name", "namespace"] })),
        tool_def("cleanup_namespace",
            "Delete all KubeVirt resources in a test namespace.",
            json!({ "type": "object", "properties": { "namespace": { "type": "string" } }, "required": ["namespace"] })),
    ]
}

fn linter_tools() -> Vec<Value> {
    vec![
        tool_def("get_setup_rules",
            "Return the beforeEach/beforeAll setup convention rules for the kubevirt-ui playwright project.",
            json!({ "type": "object", "properties": {} })),
        tool_def("get_teardown_rules",
            "Return the afterEach/afterAll teardown convention rules for the kubevirt-ui playwright project.",
            json!({ "type": "object", "properties": {} })),
        tool_def("get_fixture_map",
            "Return the fixture map — which fixture names map to which imports and their capabilities.",
            json!({ "type": "object", "properties": {} })),
        tool_def("get_env_vars",
            "Return environment variables used by playwright tests with their purpose and defaults.",
            json!({ "type": "object", "properties": {} })),
        tool_def("get_allure_suite_map",
            "Return the Allure suite/sub-suite/story map for test reporting organisation.",
            json!({ "type": "object", "properties": {} })),
        tool_def("lint_spec_file",
            "Check a spec file against project conventions (imports, fixture usage, cleanup, annotations).",
            json!({ "type": "object", "properties": { "path": { "type": "string", "description": "Absolute or project-relative path to the .spec.ts file" } }, "required": ["path"] })),
        tool_def("check_api_ui_parity",
            "Check API-UI parity: find UI features without matching API test coverage and vice versa.",
            json!({ "type": "object", "properties": {} })),
        tool_def("validate_std_coverage",
            "Validate that every spec file has a matching STD document and flag gaps.",
            json!({ "type": "object", "properties": {} })),
    ]
}

/// Convenience: build a tool JSON object from name, description, inputSchema value.
fn tool_def(name: &str, description: &str, input_schema: Value) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema,
    })
}

// ── Dispatch ──────────────────────────────────────────────────────────────────

async fn dispatch_tool(name: &str, params: &Value, state: &Arc<AppState>) -> ToolCallResult {
    // Coverage tools
    let coverage_names = [
        "get_coverage_for_feature", "get_untested_step_driver_methods",
        "get_orphan_page_object_methods", "get_tier_distribution", "find_tests_by_jira",
        "invalidate_cache", "get_cluster_info", "list_vms", "get_vm_detail",
        "list_test_namespaces", "cleanup_stale_namespaces", "check_cluster_health",
        "scaffold_test", "scaffold_page_object", "scaffold_step_driver", "scaffold_std",
        "run_tests", "get_test_results", "get_pr_details", "get_pr_files_coverage",
        "get_pr_comments", "list_open_prs", "search_prs",
    ];
    if coverage_names.contains(&name) {
        let mut scanner = coverage::scanner::ProjectScanner::new(&state.cfg);
        return coverage::tools::dispatch(name, params, &mut scanner, &state.kube_client, &state.cfg).await;
    }

    // Context tools
    let context_names = [
        "get_class_surface", "get_selector_map", "get_task_context", "get_fixture_api",
        "get_import_guide", "get_base_patterns", "search_methods", "refresh_index",
    ];
    if context_names.contains(&name) {
        return dispatch_context_tool(name, params, state).await;
    }

    // Spec tools
    let spec_names = [
        "list_std_docs", "get_std_doc", "get_all_specs_markdown",
        "list_spec_files", "get_spec_markdown", "search_tests",
    ];
    if spec_names.contains(&name) {
        return dispatch_spec_tool(name, params);
    }

    // Memory tools
    let memory_names = ["get_ticket", "search_tickets", "list_tickets", "refresh_store"];
    if memory_names.contains(&name) {
        return dispatch_memory_tool(name, params, state).await;
    }

    // CI triage tools
    let ci_names = [
        "parse_junit_report", "parse_jenkins_report", "merge_quarantined",
        "classify_failures", "get_reproduce_command", "get_allure_failures", "get_failure_summary",
    ];
    if ci_names.contains(&name) {
        return dispatch_ci_tool(name, params, state).await;
    }

    // Cluster inspector tools
    let inspector_names = [
        "get_resource", "list_resources", "get_hco_status", "get_vm_events",
        "get_storage_class_info", "get_node_status", "get_namespace_inventory", "explain_stuck_namespace",
    ];
    if inspector_names.contains(&name) {
        return dispatch_inspector_tool(name, params, state).await;
    }

    // oc/virtctl tools
    let oc_names = [
        "oc_get", "oc_apply_yaml", "oc_delete", "oc_wait", "oc_logs", "oc_exec",
        "virtctl_migrate", "virtctl_pause", "virtctl_unpause", "virtctl_ssh", "cleanup_namespace",
    ];
    if oc_names.contains(&name) {
        return dispatch_oc_tool(name, params, &state.cfg).await;
    }

    // Linter tools
    let linter_names = [
        "get_setup_rules", "get_teardown_rules", "get_fixture_map", "get_env_vars",
        "get_allure_suite_map", "lint_spec_file", "check_api_ui_parity", "validate_std_coverage",
    ];
    if linter_names.contains(&name) {
        return dispatch_linter_tool(name, params, &state.cfg);
    }

    ToolCallResult::error(format!("Unknown tool: '{}'", name))
}

// ── Sub-dispatchers ───────────────────────────────────────────────────────────

async fn dispatch_context_tool(name: &str, params: &Value, state: &Arc<AppState>) -> ToolCallResult {
    let index_guard = state.index.read().await;
    let index = index_guard.clone();

    match name {
        "get_class_surface" => context::tools::surface::get_class_surface(&index, params),
        "get_selector_map" => context::tools::surface::get_selector_map(&index, params),
        "get_task_context" => context::tools::context::get_task_context(&index, params),
        "get_fixture_api" => context::tools::context::get_fixture_api(&index, params),
        "get_import_guide" => context::tools::context::get_import_guide(&index, params),
        "get_base_patterns" => context::tools::patterns::get_base_patterns(&index, params),
        "search_methods" => context::tools::search::search_methods(&index, params),
        "refresh_index" => {
            drop(index_guard);
            context::rebuild_index(state.cfg.playwright_root.clone(), state.index.clone()).await;
            ToolCallResult::text("Index rebuilt.")
        }
        _ => ToolCallResult::error(format!("Unknown context tool: '{}'", name)),
    }
}

fn dispatch_spec_tool(name: &str, params: &Value) -> ToolCallResult {
    match name {
        "list_std_docs" => spec::tools::handlers::handle_list_std_docs(params),
        "get_std_doc" => spec::tools::handlers::handle_get_std_doc(params),
        "get_all_specs_markdown" => spec::tools::handlers::handle_get_all_specs_markdown(params),
        "list_spec_files" => spec::tools::handlers::handle_list_spec_files(params),
        "get_spec_markdown" => spec::tools::handlers::handle_get_spec_markdown(params),
        "search_tests" => spec::tools::handlers::handle_search_tests(params),
        _ => ToolCallResult::error(format!("Unknown spec tool: '{}'", name)),
    }
}

async fn dispatch_memory_tool(name: &str, params: &Value, state: &Arc<AppState>) -> ToolCallResult {
    match name {
        "get_ticket" => {
            let result = memory::tools::get_ticket(&state.store, params).await;
            result.into_tool_call_result()
        }
        "search_tickets" => {
            let result = memory::tools::search_tickets(&state.store, params).await;
            result.into_tool_call_result()
        }
        "list_tickets" => {
            let result = memory::tools::list_tickets(&state.store).await;
            result.into_tool_call_result()
        }
        "refresh_store" => {
            let client = Arc::clone(&state.http_client);
            let shared = state.store.clone();
            let cfg = state.cfg.clone();
            tokio::spawn(async move {
                memory::run_refresh(&client, &shared, &cfg).await;
            });
            ToolCallResult::text("Store refresh started in background.")
        }
        _ => ToolCallResult::error(format!("Unknown memory tool: '{}'", name)),
    }
}

async fn dispatch_ci_tool(name: &str, params: &Value, state: &Arc<AppState>) -> ToolCallResult {
    match name {
        "parse_junit_report" => ci_triage::tools::junit::parse_junit(params, &state.cfg),
        "merge_quarantined" => ci_triage::tools::junit::merge_quarantined_tool(params, &state.cfg),
        "classify_failures" => ci_triage::tools::classify::classify_failures(params, &state.cfg),
        "get_reproduce_command" => ci_triage::tools::junit::get_reproduce_command(params, &state.cfg),
        "get_allure_failures" => ci_triage::tools::allure::get_allure_failures(params, &state.cfg),
        "get_failure_summary" => ci_triage::tools::junit::get_failure_summary(params, &state.cfg),
        "parse_jenkins_report" => {
            ci_triage::tools::jenkins::parse_jenkins(params, &state.cfg, &state.http_client).await
        }
        _ => ToolCallResult::error(format!("Unknown CI triage tool: '{}'", name)),
    }
}

async fn dispatch_inspector_tool(name: &str, params: &Value, state: &Arc<AppState>) -> ToolCallResult {
    match name {
        "get_resource" => cluster_inspector::tools::resources::get_resource(params, &state.kube_client).await,
        "list_resources" => cluster_inspector::tools::resources::list_resources(params, &state.kube_client).await,
        "get_hco_status" => cluster_inspector::tools::resources::get_hco_status(params, &state.kube_client).await,
        "get_vm_events" => cluster_inspector::tools::resources::get_vm_events(params, &state.kube_client).await,
        "get_storage_class_info" => cluster_inspector::tools::resources::get_storage_class_info(&state.kube_client).await,
        "get_node_status" => cluster_inspector::tools::resources::get_node_status(&state.kube_client).await,
        "get_namespace_inventory" => cluster_inspector::tools::inventory::get_namespace_inventory(params, &state.kube_client).await,
        "explain_stuck_namespace" => cluster_inspector::tools::inventory::explain_stuck_namespace(params, &state.kube_client).await,
        _ => ToolCallResult::error(format!("Unknown inspector tool: '{}'", name)),
    }
}

async fn dispatch_oc_tool(name: &str, params: &Value, cfg: &Config) -> ToolCallResult {
    match name {
        "oc_get" => oc::tools::oc::oc_get(params, cfg).await,
        "oc_apply_yaml" => oc::tools::oc::oc_apply_yaml(params, cfg).await,
        "oc_delete" => oc::tools::oc::oc_delete(params, cfg).await,
        "oc_wait" => oc::tools::oc::oc_wait(params, cfg).await,
        "oc_logs" => oc::tools::oc::oc_logs(params, cfg).await,
        "oc_exec" => oc::tools::oc::oc_exec(params, cfg).await,
        "cleanup_namespace" => oc::tools::oc::cleanup_namespace(params, cfg).await,
        "virtctl_migrate" => oc::tools::virtctl::virtctl_migrate(params, cfg).await,
        "virtctl_pause" => oc::tools::virtctl::virtctl_pause(params, cfg).await,
        "virtctl_unpause" => oc::tools::virtctl::virtctl_unpause(params, cfg).await,
        "virtctl_ssh" => oc::tools::virtctl::virtctl_ssh(params, cfg).await,
        _ => ToolCallResult::error(format!("Unknown oc tool: '{}'", name)),
    }
}

fn dispatch_linter_tool(name: &str, params: &Value, cfg: &Config) -> ToolCallResult {
    match name {
        "get_setup_rules" => linter::tools::rules::get_setup_rules(cfg),
        "get_teardown_rules" => linter::tools::rules::get_teardown_rules(cfg),
        "get_fixture_map" => linter::tools::fixtures::get_fixture_map(cfg),
        "get_env_vars" => linter::tools::fixtures::get_env_vars(cfg),
        "get_allure_suite_map" => linter::tools::fixtures::get_allure_suite_map(cfg),
        "lint_spec_file" => linter::tools::lint::lint_spec_file(params, cfg),
        "check_api_ui_parity" => linter::tools::parity::check_api_ui_parity(cfg),
        "validate_std_coverage" => linter::tools::parity::validate_std_coverage(cfg),
        _ => ToolCallResult::error(format!("Unknown linter tool: '{}'", name)),
    }
}

async fn send_response(stdout: &mut tokio::io::Stdout, response: &Response) {
    match serde_json::to_string(response) {
        Ok(json) => {
            let line = format!("{}\n", json);
            if let Err(e) = stdout.write_all(line.as_bytes()).await {
                error!("Failed to write response: {}", e);
            }
            if let Err(e) = stdout.flush().await {
                error!("Failed to flush stdout: {}", e);
            }
        }
        Err(e) => error!("Failed to serialize response: {}", e),
    }
}
