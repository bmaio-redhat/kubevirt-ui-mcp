use serde_json::{json, Value};

pub fn scaffold_test(params: &Value) -> Value {
    let feature = params.get("feature").and_then(|v| v.as_str()).unwrap_or("my-feature");
    let tier = params.get("tier").and_then(|v| v.as_str()).unwrap_or("tier1");
    let describe_name = params
        .get("describe_name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| to_pascal_case(feature));
    let jira_ids: Vec<String> = params
        .get("jira_ids")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).map(|s| s.to_string()).collect())
        .unwrap_or_default();
    let tags: Vec<String> = params
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).map(|s| s.to_string()).collect())
        .unwrap_or_default();
    let use_shared = params.get("use_shared_resources").and_then(|v| v.as_bool()).unwrap_or(false);

    let kebab = to_kebab_case(feature);
    let file_path = format!("playwright/tests/{}/{}/{}.spec.ts", tier, kebab, kebab);

    let allure_ids: String = jira_ids
        .iter()
        .enumerate()
        .map(|(_, id)| format!("  await allure.id('{}');\n", id))
        .collect();

    let tag_list = {
        let mut all_tags = vec![format!("@{}", tier)];
        all_tags.extend(tags.iter().cloned());
        all_tags.iter().map(|t| format!("'{}'", t)).collect::<Vec<_>>().join(", ")
    };

    let fixture_params = if use_shared {
        "{ sharedResources, page }"
    } else {
        "{ steps, page }"
    };

    let test_name = if jira_ids.is_empty() {
        "should perform expected behavior".to_string()
    } else {
        jira_ids.iter().map(|id| format!("ID({}) ", id)).collect::<String>() + "should perform expected behavior"
    };

    let content = format!(
        r#"import {{ allure }} from 'allure-playwright';
import {{ scenarioTest as test }} from '@/fixtures/scenario-test-fixture';

test.describe('{describe_name}', () => {{
  test(
    '{test_name}',
    {{ tag: [{tag_list}] }},
    async ({fixture_params}) => {{
{allure_ids}      await allure.feature('{describe_name}');

      // TODO: implement test
    }},
  );
}});
"#,
        describe_name = describe_name,
        test_name = test_name,
        tag_list = tag_list,
        fixture_params = fixture_params,
        allure_ids = allure_ids,
    );

    json!({ "filePath": file_path, "content": content })
}

pub fn scaffold_page_object(params: &Value) -> Value {
    let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("my-feature");
    let base_class = params.get("base_class").and_then(|v| v.as_str()).unwrap_or("PageCommons");
    let url_pattern = params.get("url_pattern").and_then(|v| v.as_str()).unwrap_or("");

    let pascal = to_pascal_case(name);
    let kebab = to_kebab_case(name);
    let class_name = if pascal.ends_with("Page") { pascal.clone() } else { format!("{}Page", pascal) };
    let file_path = format!("playwright/src/page-objects/{}-page.ts", kebab);

    let nav_method = if !url_pattern.is_empty() {
        let url = url_pattern.replace("{namespace}", "${projectName}");
        format!(
            "\n  async navigateTo(projectName: string): Promise<void> {{\n    await this.goTo(`{}`);\n  }}\n",
            url
        )
    } else {
        "\n  // TODO: Add navigation methods\n".into()
    };

    let base_import = if base_class == "BasePage" {
        "import BasePage from './base-page';".to_string()
    } else {
        "import PageCommons from './page-commons';".to_string()
    };

    let content = format!(
        r#"import {{ Page }} from '@playwright/test';
{base_import}

export default class {class_name} extends {base_class} {{
  constructor(page: Page) {{
    super(page);
  }}
{nav_method}
  // TODO: Add locators and action methods
}}
"#,
        base_import = base_import,
        class_name = class_name,
        base_class = base_class,
        nav_method = nav_method,
    );

    json!({ "filePath": file_path, "content": content })
}

pub fn scaffold_step_driver(params: &Value) -> Value {
    let feature = params.get("feature").and_then(|v| v.as_str()).unwrap_or("my-feature");
    let page_object_name = params
        .get("page_object_name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("{}Page", to_pascal_case(feature)));

    let pascal = to_pascal_case(feature);
    let kebab = to_kebab_case(feature);
    let class_name = format!("{}StepDriver", pascal);
    let po_kebab = to_kebab_case(&page_object_name.replace("Page", ""));
    let file_path = format!("playwright/src/step-drivers/{}-step-driver.ts", kebab);

    let content = format!(
        r#"import {{ Page }} from '@playwright/test';
import {po_name} from '@/page-objects/{po_kebab}-page';
import BasePageStepDriver from './base-page-step-driver';

export default class {class_name} extends BasePageStepDriver<{po_name}> {{
  constructor(page: Page) {{
    super(page, {po_name});
  }}

  // TODO: Add step methods
}}
"#,
        po_name = page_object_name,
        po_kebab = po_kebab,
        class_name = class_name,
    );

    json!({ "filePath": file_path, "content": content })
}

pub fn scaffold_std(params: &Value) -> Value {
    let feature = params.get("feature").and_then(|v| v.as_str()).unwrap_or("my-feature");
    let tier = params.get("tier").and_then(|v| v.as_str()).unwrap_or("tier1");
    let jira_ids: Vec<String> = params
        .get("jira_ids")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).map(|s| s.to_string()).collect())
        .unwrap_or_default();

    let pascal = to_pascal_case(feature);
    let kebab = to_kebab_case(feature);
    let human_name = pascal_to_words(&pascal);
    let file_path = format!("playwright/docs/{}/{}.md", tier, kebab);
    let spec_path = format!("tests/{}/{}/{}.spec.ts", tier, kebab, kebab);

    let related_ids = if jira_ids.is_empty() {
        "N/A".into()
    } else {
        jira_ids
            .iter()
            .map(|id| format!("[{}](https://issues.redhat.com/browse/{})", id, id))
            .collect::<Vec<_>>()
            .join(", ")
    };

    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

    let traceability_rows = if jira_ids.is_empty() {
        format!("| N/A | `001` | `{}` |", spec_path)
    } else {
        jira_ids
            .iter()
            .enumerate()
            .map(|(i, id)| format!("| {} | `{:03}` | `{}` |", id, i + 1, spec_path))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let content = format!(
        r#"# Software Test Description (STD): {human_name} ({tier_pascal})

## 1. Project Overview
*   **Project Name:** OpenShift Virtualization (CNV)
*   **Feature Area:** {tier_pascal} -- {human_name}
*   **Related IDs:** {related_ids}
*   **Date:** {today}
*   **Document Status:** Draft

## 2. Introduction
### 2.1 Purpose
Documents `playwright/{spec_path}`: {human_name} test scenarios.

### 2.2 Scope
*   **In-Scope:** {human_name} page functionality, CRUD operations, navigation.
*   **Out-of-Scope:** Other feature areas not covered by this spec file.

## 3. Test Environment & Prerequisites
*   **Environment:** OpenShift with OpenShift Virtualization.
*   **Configuration:** Standard test namespace with required permissions.

## 4. Test Case Definitions

*Automation:* `{spec_path}`

### `001`: [Test case title]
*   **Objective:** [Describe the specific goal.]
*   **Pre-conditions:** User is authenticated.

| Step | Action | Expected Result |
| :--- | :--- | :--- |
| 1 | [Action] | [Expected result] |

---

## 5. Requirements Traceability Matrix

| Requirement ID | Test Case ID | Automation (Spec) |
| :--- | :--- | :--- |
{traceability_rows}
"#,
        human_name = human_name,
        tier_pascal = to_pascal_case(tier),
        related_ids = related_ids,
        today = today,
        spec_path = spec_path,
        traceability_rows = traceability_rows,
    );

    json!({ "filePath": file_path, "content": content })
}

// ── String helpers ─────────────────────────────────────────────────────────

fn to_pascal_case(s: &str) -> String {
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

fn to_kebab_case(s: &str) -> String {
    s.to_lowercase().replace('_', "-").replace(' ', "-")
}

fn pascal_to_words(s: &str) -> String {
    let re = regex::Regex::new(r"([a-z])([A-Z])").unwrap();
    re.replace_all(s, "$1 $2").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── string helpers ───────────────────────────────────────────────────────

    #[test]
    fn pascal_case_from_kebab() {
        assert_eq!(to_pascal_case("storage-migration"), "StorageMigration");
        assert_eq!(to_pascal_case("vm-snapshots"), "VmSnapshots");
        assert_eq!(to_pascal_case("checkups"), "Checkups");
    }

    #[test]
    fn kebab_case_passthrough() {
        assert_eq!(to_kebab_case("storage-migration"), "storage-migration");
        assert_eq!(to_kebab_case("VM_Snapshots"), "vm-snapshots");
    }

    // ── scaffold_test ────────────────────────────────────────────────────────

    #[test]
    fn scaffold_test_produces_file_path() {
        let result = scaffold_test(&json!({ "feature": "vm-snapshots", "tier": "tier1" }));
        assert_eq!(result["filePath"], "playwright/tests/tier1/vm-snapshots/vm-snapshots.spec.ts");
    }

    #[test]
    fn scaffold_test_embeds_jira_ids() {
        let result = scaffold_test(&json!({
            "feature": "checkups",
            "tier": "tier1",
            "jira_ids": ["CNV-11111", "CNV-22222"]
        }));
        let content = result["content"].as_str().unwrap();
        assert!(content.contains("ID(CNV-11111)"));
        assert!(content.contains("ID(CNV-22222)"));
        assert!(content.contains("allure.id('CNV-11111')"));
        assert!(content.contains("allure.id('CNV-22222')"));
    }

    #[test]
    fn scaffold_test_tier_tag_always_present() {
        let result = scaffold_test(&json!({ "feature": "networking", "tier": "gating" }));
        let content = result["content"].as_str().unwrap();
        assert!(content.contains("@gating"));
    }

    #[test]
    fn scaffold_test_shared_resources_fixture() {
        let result = scaffold_test(&json!({
            "feature": "bootable-volumes",
            "tier": "tier2",
            "use_shared_resources": true
        }));
        let content = result["content"].as_str().unwrap();
        assert!(content.contains("sharedResources"));
        assert!(!content.contains("{ steps, page }"));
    }

    #[test]
    fn scaffold_test_custom_describe_name() {
        let result = scaffold_test(&json!({
            "feature": "vm-actions",
            "tier": "tier1",
            "describe_name": "My Custom Title"
        }));
        let content = result["content"].as_str().unwrap();
        assert!(content.contains("'My Custom Title'"));
    }

    // ── scaffold_page_object ─────────────────────────────────────────────────

    #[test]
    fn scaffold_page_object_adds_page_suffix() {
        let result = scaffold_page_object(&json!({ "name": "storage-migration" }));
        let content = result["content"].as_str().unwrap();
        assert!(content.contains("class StorageMigrationPage"));
        assert_eq!(result["filePath"], "playwright/src/page-objects/storage-migration-page.ts");
    }

    #[test]
    fn scaffold_page_object_doesnt_double_page_suffix() {
        let result = scaffold_page_object(&json!({ "name": "vm-snapshots-page" }));
        let content = result["content"].as_str().unwrap();
        // Should not produce VmSnapshotsPagePage
        assert!(!content.contains("PagePage"));
    }

    #[test]
    fn scaffold_page_object_with_url_pattern() {
        let result = scaffold_page_object(&json!({
            "name": "checkups",
            "url_pattern": "/k8s/ns/{namespace}/checkups"
        }));
        let content = result["content"].as_str().unwrap();
        assert!(content.contains("navigateTo"));
        assert!(content.contains("${projectName}"));
    }

    #[test]
    fn scaffold_page_object_base_page_import() {
        let result = scaffold_page_object(&json!({
            "name": "my-feature",
            "base_class": "BasePage"
        }));
        let content = result["content"].as_str().unwrap();
        assert!(content.contains("import BasePage from './base-page'"));
        assert!(content.contains("extends BasePage"));
    }

    // ── scaffold_step_driver ─────────────────────────────────────────────────

    #[test]
    fn scaffold_step_driver_file_path_and_class() {
        let result = scaffold_step_driver(&json!({ "feature": "vm-snapshots" }));
        assert_eq!(result["filePath"], "playwright/src/step-drivers/vm-snapshots-step-driver.ts");
        let content = result["content"].as_str().unwrap();
        assert!(content.contains("class VmSnapshotsStepDriver"));
        assert!(content.contains("VmSnapshotsPage"));
    }

    #[test]
    fn scaffold_step_driver_custom_page_object_name() {
        let result = scaffold_step_driver(&json!({
            "feature": "checkups",
            "page_object_name": "NetworkCheckupsPage"
        }));
        let content = result["content"].as_str().unwrap();
        assert!(content.contains("NetworkCheckupsPage"));
    }

    // ── scaffold_std ─────────────────────────────────────────────────────────

    #[test]
    fn scaffold_std_file_path() {
        let result = scaffold_std(&json!({ "feature": "vm-snapshots", "tier": "tier1" }));
        assert_eq!(result["filePath"], "playwright/docs/tier1/vm-snapshots.md");
    }

    #[test]
    fn scaffold_std_embeds_jira_links() {
        let result = scaffold_std(&json!({
            "feature": "checkups",
            "tier": "tier1",
            "jira_ids": ["CNV-99999"]
        }));
        let content = result["content"].as_str().unwrap();
        assert!(content.contains("CNV-99999"));
        assert!(content.contains("issues.redhat.com"));
    }

    #[test]
    fn scaffold_std_traceability_rows_per_jira_id() {
        let result = scaffold_std(&json!({
            "feature": "networking",
            "tier": "tier2",
            "jira_ids": ["CNV-1", "CNV-2", "CNV-3"]
        }));
        let content = result["content"].as_str().unwrap();
        assert!(content.contains("CNV-1"));
        assert!(content.contains("CNV-2"));
        assert!(content.contains("CNV-3"));
    }
}
