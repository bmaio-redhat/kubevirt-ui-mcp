# kubevirt-ui-mcp

A unified [Model Context Protocol](https://modelcontextprotocol.io) server for the `kubevirt-ui` playwright test suite. It aggregates seven previously separate MCP servers into a single Rust binary, exposing **75 tools** across eight functional domains.

## Quick start

```bash
# Build
cargo build --release

# Smoke test
echo '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}' \
  | ./target/release/kubevirt-ui-mcp 2>/dev/null \
  | python3 -c "import sys,json; t=json.load(sys.stdin)['result']['tools']; print(len(t), 'tools')"

# Run tests
cargo test
```

## Configuration

All configuration is through environment variables. The binary reads them at startup; none are required — sensible defaults are used when omitted.

| Variable | Default | Purpose |
|---|---|---|
| `KUBEVIRT_PROJECT_ROOT` | `$PWD` | Root of the `kubevirt-ui` checkout |
| `PLAYWRIGHT_TESTS_ROOT` | `$KUBEVIRT_PROJECT_ROOT/playwright/tests` | Spec file tree |
| `PLAYWRIGHT_DOCS_ROOT` | `$KUBEVIRT_PROJECT_ROOT/playwright/docs` | STD markdown docs |
| `KUBECONFIG` | `~/.kube/config` | Kubeconfig path for cluster tools |
| `CLUSTER_URL` | read from kubeconfig | Override Kubernetes API server URL |
| `GITHUB_REPO` | _(empty)_ | Default repo for GitHub tools, e.g. `kubevirt-ui/kubevirt-plugin` |
| `JIRA_BASE_URL` | `https://redhat.atlassian.net` | Jira instance root |
| `OC_PATH` | `oc` | Path to the `oc` binary |
| `VIRTCTL_PATH` | `virtctl` | Path to the `virtctl` binary |
| `STORE_PATH` | `~/.cache/kubevirt-memory/store.json` | Jira/commit cache file |
| `GITHUB_COMMIT_PAGES` | `5` | Pages of commit history to fetch on refresh |
| `JENKINS_URL` | _(empty)_ | Jenkins base URL for CI triage |
| `JENKINS_USER` | _(empty)_ | Jenkins username |
| `JENKINS_TOKEN` | _(empty)_ | Jenkins API token |
| `KUBEVIRT_MCP_LOG` | `info` | `tracing`-style log filter (e.g. `debug`, `warn`) |

## Cursor MCP config

Both config examples use `$KUBEVIRT_UI_MCP_DIR` (path to this repo) and `$KUBEVIRT_PROJECT_ROOT` (path to the `kubevirt-ui` checkout). Set them before launching Cursor:

```bash
export KUBEVIRT_UI_MCP_DIR=/path/to/kubevirt-ui-mcp
export KUBEVIRT_PROJECT_ROOT=/path/to/kubevirt-ui
```

### Global (`~/.cursor/mcp.json`)

```json
{
  "mcpServers": {
    "Playwright": {
      "command": "npx",
      "args": ["@playwright/mcp@latest", "--ignore-https-errors"]
    },
    "kubevirt-ui-mcp": {
      "command": "${KUBEVIRT_UI_MCP_DIR}/target/release/kubevirt-ui-mcp",
      "args": [],
      "env": {
        "KUBEVIRT_PROJECT_ROOT": "${KUBEVIRT_PROJECT_ROOT}",
        "PLAYWRIGHT_TESTS_ROOT": "${KUBEVIRT_PROJECT_ROOT}/playwright/tests",
        "PLAYWRIGHT_DOCS_ROOT": "${KUBEVIRT_PROJECT_ROOT}/playwright/docs",
        "GITHUB_REPO": "kubevirt-ui/kubevirt-plugin",
        "JIRA_BASE_URL": "https://redhat.atlassian.net"
      }
    }
  }
}
```

### Project-local (`kubevirt-ui/.cursor/mcp.json`)

```json
{
  "mcpServers": {
    "Playwright": {
      "command": "npx",
      "args": ["@playwright/mcp@latest", "--ignore-https-errors"]
    },
    "kubevirt-ui-mcp": {
      "command": "${KUBEVIRT_UI_MCP_DIR}/target/release/kubevirt-ui-mcp",
      "args": [],
      "env": {
        "KUBEVIRT_PROJECT_ROOT": "${KUBEVIRT_PROJECT_ROOT}",
        "PLAYWRIGHT_TESTS_ROOT": "${KUBEVIRT_PROJECT_ROOT}/playwright/tests",
        "PLAYWRIGHT_DOCS_ROOT": "${KUBEVIRT_PROJECT_ROOT}/playwright/docs",
        "GITHUB_REPO": "kubevirt-ui/kubevirt-plugin",
        "JIRA_BASE_URL": "https://redhat.atlassian.net"
      }
    }
  }
}
```

---

## Tools reference

### Coverage (`src/coverage/`) — ported from `kubevirt-qe-mcp`

Coverage oracle, cluster state, GitHub integration, test runner, and scaffolding. Originally written in Node.js/TypeScript; ported to Rust.

| Tool | Description |
|---|---|
| `get_coverage_for_feature` | Find all spec files, step drivers, page objects, and STD docs covering a feature area. Returns Jira IDs. |
| `get_untested_step_driver_methods` | Public step driver methods never called from any spec or other driver. |
| `get_orphan_page_object_methods` | Public page object methods never referenced by any step driver or test. |
| `get_tier_distribution` | Breakdown of tests by tier (gating/tier1/tier2/fleet-virtualization-acm) with file counts, test counts, Jira IDs. |
| `find_tests_by_jira` | Look up which specs cover a Jira ticket via `ID(CNV-XXXXX)` annotations. |
| `invalidate_cache` | Clear the in-memory scanner cache. Call after modifying spec files. |
| `get_cluster_info` | Kubernetes/KubeVirt/CNV/CDI versions and node count. |
| `list_vms` | List VMs in a namespace (or all namespaces) with status, CPU, memory, run strategy. |
| `get_vm_detail` | Full VM spec/status, conditions, volumes, networks, VMI runtime info. |
| `list_test_namespaces` | List `pw-*` test namespaces with age and status. |
| `cleanup_stale_namespaces` | Delete `pw-*` namespaces older than N hours (default 4). |
| `check_cluster_health` | Pre-flight check: API server, CNV operator, virt-api pods, storage classes, node readiness. |
| `scaffold_test` | Generate a `.spec.ts` skeleton with fixtures, Allure, cleanup, and `ID()` annotations. Does NOT write to disk. |
| `scaffold_page_object` | Generate a page object class extending `BasePage` or `PageCommons`. Does NOT write to disk. |
| `scaffold_step_driver` | Generate a step driver class wired to a page object. Does NOT write to disk. |
| `scaffold_std` | Generate an STD markdown document from the project template. Does NOT write to disk. |
| `run_tests` | Execute `yarn test-playwright` with structured params. Supports `dry_run` to preview the command. |
| `get_test_results` | Parse latest JUnit XML or Allure results. Returns pass/fail counts and failed test details. |
| `get_pr_details` | PR metadata, changed files, CI check status (shells out to `gh`). |
| `get_pr_files_coverage` | Cross-reference PR changed files with spec/page-object/step-driver coverage. |
| `get_pr_comments` | All review and issue comments for a PR. |
| `list_open_prs` | Open PRs with optional author and label filters. |
| `search_prs` | Search PRs by keyword with GitHub search qualifier support. |

### Context (`src/context/`) — from `kubevirt-qe-context`

Token-efficient AST views of the playwright framework. Parses TypeScript with `tree-sitter` and maintains a live in-memory index with file watching.

| Tool | Description |
|---|---|
| `get_class_surface` | Public method signatures and JSDoc for any class. Compresses 4000+ line files to ~200 lines. |
| `get_selector_map` | All `data-test`, `data-test-id`, and ARIA role selectors in a page object. |
| `get_task_context` | Given a task description, returns relevant method signatures, fixture properties, and import paths. |
| `get_fixture_api` | Compressed public interface of `scenario-test-fixture.ts`. |
| `get_import_guide` | Correct relative import paths for class/function/type names. |
| `get_base_patterns` | Minimal, correct code skeleton for test creation, step driver extension, etc. |
| `search_methods` | Full-text search across all step driver and page object method names and JSDoc. |
| `refresh_index` | Force a rebuild of the AST index from disk. |

### Spec (`src/spec/`) — from `playwright-spec-mcp`

STD document access and spec file metadata surfaced as structured markdown.

| Tool | Description |
|---|---|
| `list_std_docs` | List all STD markdown documents with optional filter. |
| `get_std_doc` | Full STD content with spec metadata (Jira IDs, tags, skip status) for a doc path. |
| `list_spec_files` | All spec files by tier, annotated with STD coverage. |
| `get_spec_markdown` | STD document for a specific spec file, falling back to spec metadata only. |
| `get_all_specs_markdown` | All STDs for a tier or feature. |
| `search_tests` | Search test cases by Jira ID, name keyword, or tag. |

### Memory (`src/memory/`) — from `kubevirt-memory`

Pre-indexed CNV Jira ticket cache and `kubevirt-plugin` commit history. Refreshes in the background at startup.

| Tool | Description |
|---|---|
| `get_ticket` | Full cached record for a CNV Jira ticket: summary, status, type, labels, linked commits. |
| `search_tickets` | Full-text search across all cached CNV tickets. |
| `list_tickets` | Compact list of all cached tickets. |
| `refresh_store` | Re-fetch commits from GitHub and refresh stale Jira data. Runs in background. |

### CI Triage (`src/ci_triage/`) — from `kubevirt-ci-triage`

Parse and classify test run failures from JUnit XML, Allure results, and Jenkins.

| Tool | Description |
|---|---|
| `parse_junit_report` | Parse a JUnit XML report into structured test results with Jira IDs and error messages. |
| `parse_jenkins_report` | Fetch and parse a Jenkins test report from a build URL or local file. |
| `merge_quarantined` | Merge failed tests with matching quarantined skipped entries. |
| `classify_failures` | Classify failures as `infrastructure`, `product_bug`, `test_bug`, or `flaky`. |
| `get_reproduce_command` | Emit the exact `yarn test-playwright` command to reproduce a specific failure. |
| `get_allure_failures` | Scan `allure-results/` directory and return all failed test details. |
| `get_failure_summary` | High-level summary: total/passed/failed/skipped, top failures, per-tier breakdown. |

### Cluster Inspector (`src/cluster_inspector/`) — from `kubevirt-cluster-inspector`

Deep KubeVirt cluster state via direct Kubernetes REST API calls (no `kubectl` required).

| Tool | Description |
|---|---|
| `get_resource` | Get a specific Kubernetes resource by GVK, name, and namespace. |
| `list_resources` | List resources by GVK with optional namespace and label selector. |
| `get_hco_status` | HyperConverged operator status: conditions, component versions, related objects. |
| `get_vm_events` | Events for a specific VirtualMachine. |
| `get_storage_class_info` | All StorageClasses with provisioner, reclaim policy, binding mode. |
| `get_node_status` | All nodes: Ready condition, roles, capacity, allocatable. |
| `get_namespace_inventory` | Count of all KubeVirt-related resources in a namespace. |
| `explain_stuck_namespace` | Diagnose a `Terminating` namespace by listing resources with finalizers. |

### OC / virtctl (`src/oc/`) — from `kubevirt-oc-mcp`

Imperative cluster operations wrapping `oc` and `virtctl` CLIs.

| Tool | Description |
|---|---|
| `oc_get` | `oc get <resource>` with optional name, namespace, label selector. Returns JSON. |
| `oc_apply_yaml` | Apply a raw YAML manifest via `oc apply`. |
| `oc_delete` | Delete a resource by type/name or label selector. |
| `oc_wait` | Wait for a condition with optional timeout. |
| `oc_logs` | Pod logs with optional container, tail, and since filters. |
| `oc_exec` | Execute a command in a running pod. |
| `virtctl_migrate` | Live-migrate a VMI to another node. |
| `virtctl_pause` | Pause a VMI. |
| `virtctl_unpause` | Unpause a VMI. |
| `virtctl_ssh` | Open an SSH session to a VMI. |
| `cleanup_namespace` | Delete all KubeVirt resources in a test namespace. |

### Linter (`src/linter/`) — from `kubevirt-project-linter`

Spec convention checks, fixture metadata, and API-UI parity analysis.

| Tool | Description |
|---|---|
| `get_setup_rules` | `beforeEach`/`beforeAll` convention rules for the project. |
| `get_teardown_rules` | `afterEach`/`afterAll` teardown convention rules. |
| `get_fixture_map` | Fixture names → imports and capabilities. |
| `get_env_vars` | Environment variables used by tests with purpose and defaults. |
| `get_allure_suite_map` | Allure suite/sub-suite/story mapping for test reporting. |
| `lint_spec_file` | Check a spec file against project conventions (imports, fixture usage, cleanup, annotations). |
| `check_api_ui_parity` | Find UI features without API test coverage and vice versa. |
| `validate_std_coverage` | Flag spec files without a matching STD document. |

---

## Architecture

```
stdin (JSON-RPC 2.0)
        │
        ▼
   tools/call dispatcher  ──────────────────────────────────────────────┐
        │                                                                │
   ┌────┴──────────────────────────────────────────────────────┐       │
   │  coverage  │  context  │  spec  │  memory  │  ci_triage  │ ...   │
   │  (scanner, │ (tree-    │ (STD   │ (Jira/   │ (JUnit,     │       │
   │  oracle,   │  sitter   │  docs, │  GitHub  │  Allure,    │       │
   │  scaffoldr,│  AST      │  spec  │  cache,  │  Jenkins)   │       │
   │  runner,   │  index,   │  meta) │  refresh)│             │       │
   │  github,   │  watcher) │        │          │             │       │
   │  cluster)  │           │        │          │             │       │
   └────────────┴───────────┴────────┴──────────┴─────────────┘       │
                                                                        │
   cluster_inspector ── KubeClient (direct REST, no kubectl)           │
   oc          ── oc / virtctl CLI wrappers                            │
   linter      ── spec convention & parity checks                      │
                                                                        │
   Shared state (AppState):                                             │
     • SharedIndex  — Arc<RwLock<Arc<Index>>>  (context + linter)      │
     • SharedStore  — Arc<RwLock<Store>>       (memory Jira cache)     │
     • KubeClient   — Arc<KubeClient>          (inspector + coverage)  │
     • reqwest::Client — Arc (memory + jenkins)                        │
        │                                                               │
        ▼                                                               │
stdout (JSON-RPC 2.0) ◄─────────────────────────────────────────────┘
```

**Protocol:** JSON-RPC 2.0 over stdio. Implements `initialize`, `initialized`, `tools/list`, `tools/call`, and `ping`.

**State on startup:**
1. AST index built from `PLAYWRIGHT_TESTS_ROOT/../src/` via `tree-sitter-typescript`
2. File watcher started on `src/page-objects`, `src/step-drivers`, `src/fixtures`, `src/clients`
3. Jira/commit store loaded from `STORE_PATH` (if it exists) and background refresh started
4. `KubeClient` initialised from kubeconfig

---

## Source layout

```
src/
├── main.rs                  # stdio server, AppState, dispatch
├── config.rs                # unified Config from all env vars
├── mcp/protocol.rs          # JSON-RPC + MCP wire types
├── coverage/                # ported from kubevirt-qe-mcp (TypeScript → Rust)
│   ├── scanner.rs           # ProjectScanner: regex + walkdir test file analysis
│   ├── oracle.rs            # coverage-oracle tools
│   ├── cluster.rs           # cluster tools (k8s REST via KubeClient)
│   ├── github.rs            # GitHub tools (shells out to gh CLI)
│   ├── runner.rs            # run_tests / get_test_results
│   ├── scaffolder.rs        # scaffold_test/page_object/step_driver/std
│   └── tools.rs             # tool defs + dispatch
├── context/                 # from kubevirt-qe-context
│   ├── indexer.rs           # tree-sitter TypeScript AST indexer
│   ├── watcher.rs           # notify-based file watcher
│   └── tools/{context,patterns,search,surface}.rs
├── spec/                    # from playwright-spec-mcp
│   └── tools/{handlers,markdown,parser,std_docs}.rs
├── memory/                  # from kubevirt-memory
│   ├── store.rs             # ticket store + SharedStore type
│   ├── jira.rs              # Jira REST client
│   ├── github.rs            # GitHub commit fetcher
│   └── tools.rs             # tool implementations
├── ci_triage/               # from kubevirt-ci-triage
│   └── tools/{junit,allure,classify,jenkins}.rs
├── cluster_inspector/       # from kubevirt-cluster-inspector
│   ├── kube_client.rs       # direct Kubernetes REST client (no kube-rs)
│   └── tools/{resources,inventory}.rs
├── oc/                      # from kubevirt-oc-mcp
│   └── tools/{oc,virtctl}.rs
└── linter/                  # from kubevirt-project-linter
    └── tools/{rules,fixtures,lint,parity}.rs
```

---

## Development

```bash
# Debug build (faster iteration)
cargo build

# Run tests
cargo test

# Run a specific module's tests
cargo test coverage::

# View logs (goes to stderr, not stdout which carries JSON-RPC)
KUBEVIRT_MCP_LOG=debug ./target/release/kubevirt-ui-mcp 2>mcp.log

# Re-build after upstream MCP changes
# The individual repos (kubevirt-qe-context, etc.) are left untouched.
# Copy changed source files into the matching src/ subdirectory and rebuild.
```

### Adding a new tool

1. Implement the function in the relevant `src/<module>/` file
2. Add the tool definition to the `*_tools()` function in `src/main.rs`
3. Add a dispatch arm to the `dispatch_<module>_tool` function in `src/main.rs`
4. Add a `#[cfg(test)]` test for the new logic

### Upgrading an upstream module

The source code is a snapshot (not a git submodule). To pull in upstream changes:

```bash
cp -r ../kubevirt-qe-context/src/* src/context/
# fix any crate:: path references:
sed -i 's/use crate::indexer/use crate::context::indexer/g' src/context/tools/*.rs
cargo build
```

---

## Origin / what was merged

| Original binary | Language | Tools |
|---|---|---|
| `kubevirt-qe-mcp` | Node.js → **Rust port** | 23 tools (coverage, scaffolding, cluster, GitHub, test runner) |
| `kubevirt-qe-context` | Rust | 8 tools (AST index, surface, patterns, search) |
| `playwright-spec-mcp` | Rust | 6 tools (STD docs, spec metadata) |
| `kubevirt-memory` | Rust | 4 tools (Jira cache, commit history) |
| `kubevirt-ci-triage` | Rust | 7 tools (JUnit, Allure, Jenkins, classify) |
| `kubevirt-cluster-inspector` | Rust | 8 tools (HCO, resources, nodes, namespaces) |
| `kubevirt-oc-mcp` | Rust | 11 tools (oc/virtctl wrappers) |
| `kubevirt-project-linter` | Rust | 8 tools (conventions, parity, STD coverage) |

The original repos are left untouched and continue to build independently. `@playwright/mcp` (external package) remains as a separate MCP server entry.
