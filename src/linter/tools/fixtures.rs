use std::collections::BTreeMap;

use regex::Regex;
use walkdir::WalkDir;

use crate::config::Config;
use crate::mcp::protocol::ToolCallResult;

/// For every folder under playwright/tests/, find which fixture file it imports.
pub fn get_fixture_map(cfg: &Config) -> ToolCallResult {
    let tests_dir = cfg.playwright_root().join("tests");

    if !tests_dir.exists() {
        return ToolCallResult::error(format!(
            "playwright/tests/ not found at {}",
            tests_dir.display()
        ));
    }

    let fixture_import_re =
        Regex::new(r#"from\s+['"]([^'"]*fixture[s]?[^'"]*)['"]\s*;"#).unwrap();

    // Map: test folder path → (fixture import path, fixture function)
    let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for entry in WalkDir::new(&tests_dir)
        .max_depth(4)
        .into_iter()
        .flatten()
        .filter(|e| {
            e.file_type().is_file() && e.file_name().to_string_lossy().ends_with(".spec.ts")
        })
    {
        let content = match std::fs::read_to_string(entry.path()) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let fixtures: Vec<String> = fixture_import_re
            .captures_iter(&content)
            .map(|c| c[1].to_string())
            .collect();

        if !fixtures.is_empty() {
            // Key by the folder relative to playwright/tests/
            let folder = entry
                .path()
                .parent()
                .and_then(|p| p.strip_prefix(&tests_dir).ok())
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| "(root)".into());

            let entry_fixtures = map.entry(folder).or_default();
            for f in &fixtures {
                if !entry_fixtures.contains(f) {
                    entry_fixtures.push(f.clone());
                }
            }
        }
    }

    if map.is_empty() {
        return ToolCallResult::text("No fixture imports found in playwright/tests/.");
    }

    let mut out = format!(
        "## Fixture map for playwright/tests/\n\n({} folder(s) with fixture imports)\n\n",
        map.len()
    );

    for (folder, fixtures) in &map {
        out.push_str(&format!(
            "### tests/{}\n{}\n\n",
            folder,
            fixtures.iter().map(|f| format!("  - {}", f)).collect::<Vec<_>>().join("\n")
        ));
    }

    ToolCallResult::text(out)
}

/// Parse EnvVariables class usages.
pub fn get_env_vars(cfg: &Config) -> ToolCallResult {
    // Find the env variables file
    let env_files: Vec<_> = WalkDir::new(cfg.playwright_root().join("src"))
        .max_depth(5)
        .into_iter()
        .flatten()
        .filter(|e| {
            e.file_type().is_file()
                && (e.file_name().to_string_lossy().contains("env")
                    || e.file_name().to_string_lossy().contains("config"))
                && e.file_name().to_string_lossy().ends_with(".ts")
        })
        .collect();

    // Also look for .env.example or any .env files
    let env_example = cfg.project_root.join(".env.example");
    let env_file = cfg.project_root.join(".env");

    let mut out = String::from("## Environment Variables\n\n");

    // Parse .env.example first (most reliable docs)
    if env_example.exists() {
        if let Ok(content) = std::fs::read_to_string(&env_example) {
            out.push_str("### From .env.example\n\n");
            out.push_str("```\n");
            out.push_str(&content);
            out.push_str("```\n\n");
        }
    } else if env_file.exists() {
        out.push_str("(No .env.example found — showing variable names from .env)\n\n");
        if let Ok(content) = std::fs::read_to_string(&env_file) {
            let var_re = Regex::new(r"^([A-Z_]+)\s*=").unwrap();
            for line in content.lines() {
                if let Some(cap) = var_re.captures(line.trim()) {
                    out.push_str(&format!("  - {}\n", &cap[1]));
                }
            }
        }
    }

    // Parse EnvVariables class from TypeScript source
    let env_class_re = Regex::new(r"(?:static|readonly|get)\s+([A-Z_]+)(?:\s*[:=][^;]*)?").unwrap();

    for ts_entry in &env_files {
        let content = match std::fs::read_to_string(ts_entry.path()) {
            Ok(c) => c,
            Err(_) => continue,
        };

        if content.contains("EnvVariables") || content.contains("process.env") {
            let vars: Vec<&str> = env_class_re
                .captures_iter(&content)
                .filter_map(|c| {
                    let name = c.get(1)?.as_str();
                    if name.len() > 2 && name == name.to_uppercase() {
                        Some(name)
                    } else {
                        None
                    }
                })
                .collect();

            if !vars.is_empty() {
                out.push_str(&format!(
                    "### From {}\n",
                    ts_entry
                        .path()
                        .strip_prefix(&cfg.project_root)
                        .ok()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|| ts_entry.path().to_string_lossy().to_string())
                ));
                for var in vars {
                    out.push_str(&format!("  - {}\n", var));
                }
                out.push('\n');
            }
        }
    }

    ToolCallResult::text(out)
}

/// Parse allure-constants.ts to return feature → allure tags mapping.
pub fn get_allure_suite_map(cfg: &Config) -> ToolCallResult {
    let allure_files: Vec<_> = WalkDir::new(cfg.playwright_root().join("src"))
        .max_depth(5)
        .into_iter()
        .flatten()
        .filter(|e| {
            e.file_type().is_file()
                && e.file_name().to_string_lossy().contains("allure")
                && e.file_name().to_string_lossy().ends_with(".ts")
        })
        .collect();

    if allure_files.is_empty() {
        return ToolCallResult::text("No allure-constants.ts file found in playwright/src/.");
    }

    let mut out = String::from("## Allure Suite Map\n\n");

    for file in &allure_files {
        let content = match std::fs::read_to_string(file.path()) {
            Ok(c) => c,
            Err(_) => continue,
        };

        out.push_str(&format!(
            "### {}\n\n```typescript\n{}\n```\n\n",
            file.path()
                .strip_prefix(&cfg.project_root)
                .ok()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default(),
            content.chars().take(3000).collect::<String>()
        ));
    }

    ToolCallResult::text(out)
}
