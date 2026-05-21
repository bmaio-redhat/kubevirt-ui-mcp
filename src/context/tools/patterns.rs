use serde_json::Value;

use crate::context::indexer::Index;
use crate::mcp::protocol::ToolCallResult;

pub fn get_base_patterns(index: &Index, params: &Value) -> ToolCallResult {
    let pattern_type = match params.get("pattern_type").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return ToolCallResult::error("Missing required parameter: pattern_type"),
    };
    let feature = params
        .get("feature")
        .and_then(|v| v.as_str())
        .unwrap_or("my-feature");

    // Convert kebab-case feature to PascalCase class name prefix
    let pascal = to_pascal_case(feature);
    // kebab-case file name
    let kebab = to_kebab_case(feature);

    let pattern = match pattern_type {
        "test-creation" => test_creation_pattern(&pascal, &kebab, index),
        "step-driver-extension" => step_driver_extension_pattern(&pascal, &kebab, index),
        "page-object-extension" => page_object_extension_pattern(&pascal, &kebab, index),
        "api-test" => api_test_pattern(&pascal, &kebab),
        "gating-test" => gating_test_pattern(&pascal, &kebab),
        other => return ToolCallResult::error(format!(
            "Unknown pattern_type '{}'. Valid values: test-creation, step-driver-extension, page-object-extension, api-test, gating-test",
            other
        )),
    };

    ToolCallResult::text(pattern)
}

fn test_creation_pattern(pascal: &str, kebab: &str, _index: &Index) -> String {
    format!(r#"// ── Tier1 test — {pascal} ─────────────────────────────────────────────────
// File: playwright/tests/tier1/{kebab}/{kebab}.spec.ts

import {{ scenarioTest as test }} from '@/fixtures/scenario-test-fixture';
import {{ allure }} from 'allure-playwright';

test.describe('{pascal}', () => {{
  test(
    'should perform expected behavior',
    {{ tag: ['@tier1'] }},
    async ({{ steps, page }}) => {{
      await allure.id('CNV-XXXXX');
      await allure.feature('{pascal}');

      // Use step driver methods — run get_class_surface('VirtualMachinesStepDriver')
      // to see available methods, or get_task_context('{kebab} test') for focused context.
      //
      // Example: await steps.virtualMachines.navigateToVirtualMachines();
    }},
  );
}});
"#)
}

fn step_driver_extension_pattern(pascal: &str, kebab: &str, index: &Index) -> String {
    // Try to find the base step driver class signature
    let base_sig = index
        .find_class("BasePageStepDriver")
        .first()
        .map(|c| format!("// BasePageStepDriver is at: {}\n", c.relative_path))
        .unwrap_or_default();

    let page_class = format!("{}Page", pascal);
    let page_import_hint = index
        .find_class(&page_class)
        .first()
        .map(|c| {
            format!(
                "// Found existing page object: {} at {}\n",
                c.name, c.relative_path
            )
        })
        .unwrap_or_else(|| format!("// No existing page object found for '{}'. Create one first with pattern_type='page-object-extension'.\n", pascal));

    format!(r#"// ── StepDriver extension — {pascal} ───────────────────────────────────────
// File: playwright/src/step-drivers/{kebab}-step-driver.ts
{base_sig}{page_import_hint}
import {{ BasePageStepDriver }} from '@/step-drivers/base-page-step-driver';
import {{ {pascal}Page }} from '@/page-objects/{kebab}-page';

export class {pascal}StepDriver extends BasePageStepDriver<{pascal}Page> {{
  constructor(page: import('@playwright/test').Page) {{
    super(new {pascal}Page(page));
  }}

  // Add your step driver methods below.
  // Each method should orchestrate one user-facing action via this.page.*
  // Use `public async` for async methods, `public` for sync helpers.

  async navigate(): Promise<void> {{
    await this.page.navigate();
  }}

  // Example of a more complex action:
  // async createItem(name: string): Promise<void> {{
  //   await this.page.clickCreateButton();
  //   await this.page.fillName(name);
  //   await this.page.clickSubmit();
  // }}
}}
"#)
}

fn page_object_extension_pattern(pascal: &str, kebab: &str, index: &Index) -> String {
    let base_matches_pc = index.find_class("PageCommons");
    let base_matches_bp = index.find_class("BasePage");
    let base_sig = base_matches_pc
        .first()
        .or_else(|| base_matches_bp.first())
        .map(|c| format!("// Base class at: {}\n", c.relative_path))
        .unwrap_or_default();

    format!(r#"// ── Page Object — {pascal} ────────────────────────────────────────────────
// File: playwright/src/page-objects/{kebab}-page.ts
{base_sig}
import {{ PageCommons }} from '@/page-objects/page-commons';

export class {pascal}Page extends PageCommons {{
  // ── Navigation ─────────────────────────────────────────────────────────

  async navigate(): Promise<void> {{
    await this.page.goto(`${{this.baseURL}}/k8s/ns/${{this.namespace}}/{kebab}`);
    await this.waitForPageLoad();
  }}

  // ── Locators ───────────────────────────────────────────────────────────
  // Prefer data-test selectors. Use getByTestId() or locator('[data-test="..."]').

  get createButton() {{
    return this.page.locator('[data-test="create-{kebab}-button"]');
  }}

  // ── Actions ────────────────────────────────────────────────────────────

  async clickCreateButton(): Promise<void> {{
    await this.createButton.click();
  }}

  // ── Assertions ─────────────────────────────────────────────────────────

  async expectItemVisible(name: string): Promise<void> {{
    await this.page.locator(`[data-test="${{name}}"]`).waitFor({{ state: 'visible' }});
  }}
}}
"#)
}

fn api_test_pattern(pascal: &str, kebab: &str) -> String {
    format!(r#"// ── API test — {pascal} ───────────────────────────────────────────────────
// File: playwright/tests/api/{kebab}.spec.ts

import {{ apiTest as test }} from '@/fixtures/api-test-fixture';
import {{ allure }} from 'allure-playwright';

test.describe('{pascal} API', () => {{
  test(
    'should return expected response',
    async ({{ kubernetesClient, request }}) => {{
      await allure.id('CNV-XXXXX');
      await allure.feature('{pascal}');

      // kubernetesClient — KubernetesClient for direct K8s API calls
      // request — Playwright APIRequestContext for console proxy endpoints
      //
      // Example:
      // const response = await request.get('/api/kubernetes/apis/kubevirt.io/v1/virtualmachines');
      // expect(response.status()).toBe(200);
    }},
  );
}});
"#)
}

fn gating_test_pattern(pascal: &str, kebab: &str) -> String {
    format!(r#"// ── Gating test — {pascal} ───────────────────────────────────────────────
// File: playwright/tests/gating/scenario-{kebab}.spec.ts
// Gating tests verify core functionality — keep them fast and reliable.

import {{ scenarioTest as test }} from '@/fixtures/scenario-test-fixture';
import {{ allure }} from 'allure-playwright';

test.describe.serial('{pascal} — Gating', () => {{
  test.beforeAll(async ({{ steps }}) => {{
    // Setup shared resources for the scenario
  }});

  test.afterAll(async ({{ steps, cleanupManager }}) => {{
    await cleanupManager.cleanup();
  }});

  test(
    'scenario step 1 — basic creation',
    {{ tag: ['@gating'] }},
    async ({{ steps }}) => {{
      await allure.id('CNV-XXXXX');
      // ...
    }},
  );

  test(
    'scenario step 2 — verify state',
    {{ tag: ['@gating'] }},
    async ({{ steps }}) => {{
      await allure.id('CNV-XXXXX');
      // ...
    }},
  );
}});
"#)
}

// ── String helpers (pub(crate) for tests) ────────────────────────────────────

pub(crate) fn to_pascal_case(s: &str) -> String {
    s.split(|c: char| c == '-' || c == '_' || c == ' ')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => {
                    let upper: String = first.to_uppercase().collect();
                    upper + chars.as_str()
                }
            }
        })
        .collect()
}

pub(crate) fn to_kebab_case(s: &str) -> String {
    // Already kebab-case in most cases; just lowercase it
    s.to_lowercase().replace('_', "-").replace(' ', "-")
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::indexer::{Index, Indexer};
    use serde_json::json;
    use std::path::PathBuf;

    fn empty_index() -> Index {
        Index::default()
    }

    fn index_with_base_classes() -> Index {
        let indexer = Indexer::new(PathBuf::from("/fake/playwright"));
        let src = r#"
export class BasePageStepDriver<T> {
  protected page: T;
  constructor(page: T) { this.page = page; }
}
"#;
        indexer.parse_source(src, "step-drivers/base-page-step-driver")
    }

    // ── to_pascal_case ───────────────────────────────────────────────────────

    #[test]
    fn pascal_from_kebab() {
        assert_eq!(to_pascal_case("vm-snapshots"), "VmSnapshots");
    }

    #[test]
    fn pascal_from_snake() {
        assert_eq!(to_pascal_case("storage_migration"), "StorageMigration");
    }

    #[test]
    fn pascal_single_word() {
        assert_eq!(to_pascal_case("catalog"), "Catalog");
    }

    #[test]
    fn pascal_already_pascal() {
        assert_eq!(to_pascal_case("CatalogPage"), "CatalogPage");
    }

    // ── to_kebab_case ────────────────────────────────────────────────────────

    #[test]
    fn kebab_from_snake() {
        assert_eq!(to_kebab_case("storage_migration"), "storage-migration");
    }

    #[test]
    fn kebab_lowercase() {
        assert_eq!(to_kebab_case("VmSnapshots"), "vmsnapshots");
    }

    #[test]
    fn kebab_from_space() {
        assert_eq!(to_kebab_case("vm snapshots"), "vm-snapshots");
    }

    // ── get_base_patterns ────────────────────────────────────────────────────

    #[test]
    fn test_creation_pattern_contains_imports() {
        let idx = empty_index();
        let result = get_base_patterns(&idx, &json!({"pattern_type": "test-creation", "feature": "vm-snapshots"}));
        let text = &result.content[0].text;
        assert!(text.contains("import { scenarioTest as test }"), "missing fixture import");
        assert!(text.contains("allure"), "missing allure import");
        assert!(text.contains("VmSnapshots"), "missing PascalCase feature name");
        assert!(text.contains("vm-snapshots"), "missing kebab feature name");
    }

    #[test]
    fn test_creation_pattern_has_test_structure() {
        let idx = empty_index();
        let result = get_base_patterns(&idx, &json!({"pattern_type": "test-creation"}));
        let text = &result.content[0].text;
        assert!(text.contains("test.describe"), "missing describe block");
        assert!(text.contains("@tier1"), "missing tier tag");
        assert!(text.contains("CNV-XXXXX"), "missing jira placeholder");
    }

    #[test]
    fn step_driver_extension_has_correct_structure() {
        let idx = index_with_base_classes();
        let result = get_base_patterns(&idx, &json!({"pattern_type": "step-driver-extension", "feature": "storage-migration"}));
        let text = &result.content[0].text;
        assert!(text.contains("StorageMigrationStepDriver"), "missing class name");
        assert!(text.contains("extends BasePageStepDriver"), "missing extends");
        assert!(text.contains("StorageMigrationPage"), "missing page type");
        assert!(text.contains("@/step-drivers/base-page-step-driver"), "missing import");
    }

    #[test]
    fn page_object_extension_has_correct_structure() {
        let idx = empty_index();
        let result = get_base_patterns(&idx, &json!({"pattern_type": "page-object-extension", "feature": "vm-snapshots"}));
        let text = &result.content[0].text;
        assert!(text.contains("VmSnapshotsPage"), "missing page class name");
        assert!(text.contains("extends PageCommons"), "missing extends");
        assert!(text.contains("navigate()"), "missing navigate method");
    }

    #[test]
    fn api_test_pattern_has_api_fixture() {
        let idx = empty_index();
        let result = get_base_patterns(&idx, &json!({"pattern_type": "api-test", "feature": "catalog"}));
        let text = &result.content[0].text;
        assert!(text.contains("apiTest"), "missing api test fixture");
        assert!(text.contains("kubernetesClient"), "missing k8s client param");
    }

    #[test]
    fn gating_test_has_serial_describe() {
        let idx = empty_index();
        let result = get_base_patterns(&idx, &json!({"pattern_type": "gating-test"}));
        let text = &result.content[0].text;
        assert!(text.contains("test.describe.serial"), "gating tests must be serial");
        assert!(text.contains("@gating"), "missing gating tag");
        assert!(text.contains("beforeAll"), "missing beforeAll");
    }

    #[test]
    fn unknown_pattern_type_returns_error() {
        let idx = empty_index();
        let result = get_base_patterns(&idx, &json!({"pattern_type": "nonexistent"}));
        assert_eq!(result.is_error, Some(true));
    }

    #[test]
    fn missing_pattern_type_returns_error() {
        let idx = empty_index();
        let result = get_base_patterns(&idx, &json!({}));
        assert_eq!(result.is_error, Some(true));
    }
}
