use std::collections::HashMap;
use regex::Regex;

/// Attributes injected by the build system (not present in common-attributes.adoc).
pub fn build_default_attributes() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("product-title".into(), "OpenShift Container Platform".into());
    m.insert("product-version".into(), "4.22".into());
    m.insert("context".into(), "".into());
    m
}

/// Parse attribute definitions from common-attributes.adoc content.
/// Handles `:attr-name: value` lines, ignoring ifdef/endif blocks for non-default variants.
pub fn parse_attributes(content: &str) -> HashMap<String, String> {
    let mut attrs = build_default_attributes();
    let mut skip_depth = 0u32;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("ifdef::openshift-origin")
            || trimmed.starts_with("ifdef::openshift-rosa")
            || trimmed.starts_with("ifdef::openshift-dedicated")
            || trimmed.starts_with("ifdef::openshift-rosa-hcp")
            || trimmed.starts_with("ifdef::telco-")
        {
            skip_depth += 1;
            continue;
        }
        if skip_depth > 0 {
            if trimmed.starts_with("endif::") {
                skip_depth -= 1;
            } else if trimmed.starts_with("ifdef::") || trimmed.starts_with("ifndef::") {
                skip_depth += 1;
            }
            continue;
        }
        if trimmed.starts_with("ifndef::openshift-origin") {
            // We ARE the default (non-origin), so enter this block
            continue;
        }
        if trimmed.starts_with("endif::") {
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix(':') {
            if let Some(colon_pos) = rest.find(':') {
                let name = rest[..colon_pos].trim();
                let value = rest[colon_pos + 1..].trim();
                if !name.is_empty()
                    && !name.starts_with('_')
                    && !name.contains(' ')
                {
                    attrs.insert(name.to_string(), value.to_string());
                }
            }
        }
    }

    attrs
}

/// Convert AsciiDoc content (with includes already resolved) to compact markdown.
pub fn to_markdown(content: &str, attrs: &HashMap<String, String>) -> (String, String) {
    let lines: Vec<&str> = content.lines().collect();
    let mut out = Vec::new();
    let mut title = String::new();
    let mut skip_depth = 0u32;
    let mut in_listing = false;
    let mut in_table = false;
    let mut table_rows: Vec<Vec<String>> = Vec::new();
    let mut in_admonition = false;
    let mut admonition_type = String::new();
    let mut admonition_lines: Vec<String> = Vec::new();

    let attr_re = Regex::new(r"\{([a-zA-Z0-9_-]+)\}").unwrap();
    let xref_re = Regex::new(r"xref:[^\[]*\[([^\]]*)\]").unwrap();
    let link_re = Regex::new(r"link:([^\[]+)\[([^\]]*)\]").unwrap();
    let pass_re = Regex::new(r"pass:quotes\[([^\]]*)\]").unwrap();

    let substitute = |text: &str| -> String {
        let mut result = text.to_string();
        result = attr_re.replace_all(&result, |caps: &regex::Captures| {
            let name = &caps[1];
            attrs.get(name).cloned().unwrap_or_else(|| format!("{{{}}}", name))
        }).to_string();
        result = xref_re.replace_all(&result, "$1").to_string();
        result = link_re.replace_all(&result, "[$2]($1)").to_string();
        result = pass_re.replace_all(&result, "$1").to_string();
        result = result.replace("{nbsp}", " ");
        result
    };

    for line in &lines {
        let trimmed = line.trim();

        // Conditional block handling
        if trimmed.starts_with("ifdef::openshift-origin")
            || trimmed.starts_with("ifdef::openshift-rosa")
            || trimmed.starts_with("ifdef::openshift-dedicated")
            || trimmed.starts_with("ifdef::openshift-rosa-hcp")
        {
            skip_depth += 1;
            continue;
        }
        if trimmed.starts_with("ifndef::openshift-origin") && !trimmed.contains(',') {
            continue;
        }
        // ifndef with multiple: ifndef::openshift-rosa,openshift-dedicated[] — keep content
        if trimmed.starts_with("ifndef::") {
            let has_origin = trimmed.contains("openshift-origin");
            if has_origin {
                skip_depth += 1;
                continue;
            }
            // Otherwise we're not any of those variants, so keep the block
            continue;
        }
        if skip_depth > 0 {
            if trimmed.starts_with("endif::") {
                skip_depth -= 1;
            } else if trimmed.starts_with("ifdef::") || trimmed.starts_with("ifndef::") {
                skip_depth += 1;
            }
            continue;
        }
        if trimmed.starts_with("endif::") {
            continue;
        }

        // Skip metadata lines
        if trimmed.starts_with(":_mod-docs-content-type:")
            || trimmed.starts_with(":context:")
            || trimmed.starts_with(":toc:")
            || trimmed.starts_with(":imagesdir:")
            || trimmed.starts_with(":prewrap")
            || trimmed.starts_with(":data-uri")
            || trimmed.starts_with(":icons:")
            || trimmed.starts_with(":experimental:")
            || trimmed.starts_with(":toc-title:")
            || trimmed == "toc::[]"
            || trimmed.starts_with("[id=")
            || trimmed.starts_with("[role=")
            || trimmed.starts_with("[discrete]")
            || trimmed.starts_with("include::_attributes/")
            || (trimmed.starts_with('[') && trimmed.ends_with(']') && trimmed.contains("options="))
            || (trimmed.starts_with('[') && trimmed.ends_with(']') && trimmed.contains("cols="))
        {
            continue;
        }

        // Listing/code blocks
        if trimmed == "----" || trimmed == "...." {
            if in_listing {
                out.push("```".to_string());
                in_listing = false;
            } else {
                in_listing = true;
                out.push("```".to_string());
            }
            continue;
        }
        if in_listing {
            out.push(substitute(trimmed));
            continue;
        }

        // Admonition blocks: [NOTE], [IMPORTANT], [WARNING], [TIP], [CAUTION]
        if matches!(trimmed, "[NOTE]" | "[IMPORTANT]" | "[WARNING]" | "[TIP]" | "[CAUTION]") {
            admonition_type = trimmed[1..trimmed.len()-1].to_string();
            continue;
        }
        if trimmed == "====" {
            if in_admonition {
                for al in &admonition_lines {
                    out.push(format!("> **{}**: {}", admonition_type, al));
                }
                admonition_lines.clear();
                in_admonition = false;
                admonition_type.clear();
            } else if !admonition_type.is_empty() {
                in_admonition = true;
            }
            continue;
        }
        if in_admonition {
            let subst = substitute(trimmed);
            if !subst.is_empty() {
                admonition_lines.push(subst);
            }
            continue;
        }

        // Table handling
        if trimmed == "|===" {
            if in_table {
                flush_table(&table_rows, &mut out);
                table_rows.clear();
                in_table = false;
            } else {
                in_table = true;
            }
            continue;
        }
        if in_table {
            if trimmed.starts_with('|') {
                let cells: Vec<String> = trimmed
                    .split('|')
                    .skip(1)
                    .map(|c| substitute(c.trim()))
                    .collect();
                if !cells.is_empty() {
                    table_rows.push(cells);
                }
            }
            continue;
        }

        // Headings: = Title, == Section, etc.
        if trimmed.starts_with("= ") && !trimmed.starts_with("== ") {
            let heading = substitute(&trimmed[2..]);
            if title.is_empty() {
                title = heading.clone();
            }
            out.push(format!("# {}", heading));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("== ") {
            out.push(format!("## {}", substitute(rest)));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("=== ") {
            out.push(format!("### {}", substitute(rest)));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("==== ") {
            out.push(format!("#### {}", substitute(rest)));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("===== ") {
            out.push(format!("##### {}", substitute(rest)));
            continue;
        }

        // Block title: .Title
        if trimmed.starts_with('.') && !trimmed.starts_with("..") && trimmed.len() > 1 {
            let ch = trimmed.chars().nth(1).unwrap_or(' ');
            if ch.is_uppercase() || ch == '{' {
                out.push(format!("**{}**", substitute(&trimmed[1..])));
                continue;
            }
        }

        // Unordered list
        if trimmed.starts_with("* ") {
            out.push(format!("- {}", substitute(&trimmed[2..])));
            continue;
        }
        if trimmed.starts_with("** ") {
            out.push(format!("  - {}", substitute(&trimmed[3..])));
            continue;
        }
        // Ordered list
        if trimmed.starts_with(". ") && trimmed.len() > 2 {
            out.push(format!("1. {}", substitute(&trimmed[2..])));
            continue;
        }

        // Continuation (+)
        if trimmed == "+" {
            continue;
        }

        // Skip image references, comments
        if trimmed.starts_with("image:") || trimmed.starts_with("//") {
            continue;
        }

        // Skip include directives that weren't resolved
        if trimmed.starts_with("include::") {
            continue;
        }

        // Empty line
        if trimmed.is_empty() {
            if out.last().map(|l| !l.is_empty()).unwrap_or(false) {
                out.push(String::new());
            }
            continue;
        }

        // Regular paragraph text
        out.push(substitute(trimmed));
    }

    // Collapse multiple blank lines
    let mut result = Vec::new();
    let mut prev_blank = false;
    for line in out {
        if line.is_empty() {
            if !prev_blank {
                result.push(line);
            }
            prev_blank = true;
        } else {
            result.push(line);
            prev_blank = false;
        }
    }

    // Trim trailing blank lines
    while result.last().map(|l| l.is_empty()).unwrap_or(false) {
        result.pop();
    }

    (title, result.join("\n"))
}

fn flush_table(rows: &[Vec<String>], out: &mut Vec<String>) {
    if rows.is_empty() {
        return;
    }

    let col_count = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    if col_count == 0 {
        return;
    }

    for (i, row) in rows.iter().enumerate() {
        let mut cells: Vec<String> = row.clone();
        while cells.len() < col_count {
            cells.push(String::new());
        }
        out.push(format!("| {} |", cells.join(" | ")));
        if i == 0 {
            let sep: Vec<&str> = (0..col_count).map(|_| "---").collect();
            out.push(format!("| {} |", sep.join(" | ")));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_attributes() {
        let content = r#"
:VirtProductName: OpenShift Virtualization
:product-title: OpenShift Container Platform
ifdef::openshift-origin[]
:VirtProductName: OKD Virtualization
endif::[]
:sno: single-node OpenShift
"#;
        let attrs = parse_attributes(content);
        assert_eq!(attrs.get("VirtProductName").unwrap(), "OpenShift Virtualization");
        assert_eq!(attrs.get("sno").unwrap(), "single-node OpenShift");
    }

    #[test]
    fn test_heading_conversion() {
        let mut attrs = HashMap::new();
        attrs.insert("VirtProductName".to_string(), "OpenShift Virtualization".to_string());

        let content = "= About {VirtProductName}\n\n== What you can do\n\nSome text here.";
        let (title, md) = to_markdown(content, &attrs);
        assert_eq!(title, "About OpenShift Virtualization");
        assert!(md.contains("# About OpenShift Virtualization"));
        assert!(md.contains("## What you can do"));
        assert!(md.contains("Some text here."));
    }

    #[test]
    fn test_list_conversion() {
        let attrs = HashMap::new();
        let content = "* First item\n* Second item\n** Nested item";
        let (_, md) = to_markdown(content, &attrs);
        assert!(md.contains("- First item"));
        assert!(md.contains("- Second item"));
        assert!(md.contains("  - Nested item"));
    }
}
