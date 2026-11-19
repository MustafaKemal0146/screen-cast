use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_LINES: usize = 350;
const TURKISH_CHARS: &[char] = &['ğ', 'ü', 'ş', 'ı', 'İ', 'ö', 'ç', 'Ğ', 'Ü', 'Ş', 'Ö', 'Ç'];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn collect_files(dir: &Path, exts: &[&str], out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name_s = name.to_string_lossy();
        if name_s == "target" || name_s == ".git" || name_s == "__pycache__" || name_s == "legacy" {
            continue;
        }
        if path.is_dir() {
            collect_files(&path, exts, out);
        } else if exts.is_empty()
            || path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| exts.contains(&e))
        {
            out.push(path);
        }
    }
}

/// Count non-test lines in a Rust source file (excluding `#[cfg(test)]` blocks).
fn count_rust_non_test_lines(src: &str) -> usize {
    let lines: Vec<&str> = src.lines().collect();
    let mut non_test = 0usize;
    let mut pending_cfg_test = false;
    let mut in_test_block = false;
    let mut brace_depth = 0isize;
    let mut in_block_comment = false;

    for line in lines {
        let trimmed = line.trim();
        if !in_test_block && trimmed.starts_with("#[cfg(test)]") {
            if let Some((opens, closes, saw_open)) =
                scan_braces_outside_literals(line, &mut in_block_comment)
            {
                if saw_open {
                    brace_depth += opens as isize - closes as isize;
                    in_test_block = brace_depth > 0;
                    pending_cfg_test = false;
                } else {
                    pending_cfg_test = true;
                }
            } else {
                pending_cfg_test = true;
            }
            continue;
        }
        if pending_cfg_test && !in_test_block {
            if let Some((opens, closes, saw_open)) =
                scan_braces_outside_literals(line, &mut in_block_comment)
            {
                let _ = (opens, closes);
                brace_depth += opens as isize - closes as isize;
                if saw_open {
                    in_test_block = true;
                    pending_cfg_test = false;
                    if brace_depth <= 0 {
                        in_test_block = false;
                        brace_depth = 0;
                    }
                }
            }
            continue;
        }
        if in_test_block {
            if let Some((opens, closes, _)) =
                scan_braces_outside_literals(line, &mut in_block_comment)
            {
                brace_depth += opens as isize - closes as isize;
                if brace_depth <= 0 {
                    in_test_block = false;
                    brace_depth = 0;
                }
            }
            continue;
        }

        // Also update comment/brace tracking only for consistency on non-test code.
        non_test += 1;
    }
    non_test
}

fn scan_braces_outside_literals(
    line: &str,
    in_block_comment: &mut bool,
) -> Option<(usize, usize, bool)> {
    let bytes = line.as_bytes();
    let mut i = 0usize;
    let mut opens = 0usize;
    let mut closes = 0usize;
    let mut saw_open = false;
    let mut in_str = false;
    let mut escaped = false;

    while i < bytes.len() {
        let b = bytes[i];
        if *in_block_comment {
            if b == b'*' && bytes.get(i + 1) == Some(&b'/') {
                *in_block_comment = false;
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }
        if in_str {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        if b == b'/' && bytes.get(i + 1) == Some(&b'/') {
            break;
        }
        if b == b'/' && bytes.get(i + 1) == Some(&b'*') {
            *in_block_comment = true;
            i += 2;
            continue;
        }
        if b == b'"' {
            // Skip char literal vs string carefully: `"` starts string unless preceded by `'`.
            let is_char_quote = i > 0 && bytes[i - 1] == b'\'' && bytes.get(i + 1) == Some(&b'\'');
            if !is_char_quote {
                in_str = true;
            }
            i += 1;
            continue;
        }
        if b == b'\'' && i + 2 < bytes.len() {
            if bytes[i + 1] != b'\\' && bytes[i + 2] == b'\'' {
                i += 3;
                continue;
            }
            if bytes[i + 1] == b'\\' && i + 3 < bytes.len() && bytes[i + 3] == b'\'' {
                i += 4;
                continue;
            }
        }
        if b == b'{' {
            opens += 1;
            saw_open = true;
        } else if b == b'}' {
            closes += 1;
        }
        i += 1;
    }
    Some((opens, closes, saw_open))
}

fn is_dedicated_tr_catalog(rel: &str) -> bool {
    rel.starts_with("crates/screencast-core/src/i18n/tr/")
        || rel == "assets/player/i18n_tr.js"
        || rel == "assets/gui/screencast_gui_i18n_tr.py"
}

fn is_unicode_test_vector_file(rel: &str) -> bool {
    rel == "crates/screencast-capture/src/control/keymap.rs"
        || rel == "crates/screencast-sinks/src/mice/mod.rs"
}

fn contains_turkish_char(s: &str) -> bool {
    s.chars().any(|c| TURKISH_CHARS.contains(&c))
}

fn strip_cfg_test_blocks(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut pending = false;
    let mut in_test = false;
    let mut depth = 0isize;
    let mut in_bc = false;
    for line in src.lines() {
        let trimmed = line.trim();
        if !in_test && trimmed.starts_with("#[cfg(test)]") {
            if let Some((o, c, saw)) = scan_braces_outside_literals(line, &mut in_bc) {
                if saw {
                    depth += o as isize - c as isize;
                    in_test = depth > 0;
                    pending = false;
                } else {
                    pending = true;
                }
            } else {
                pending = true;
            }
            continue;
        }
        if pending && !in_test {
            if let Some((o, c, saw)) = scan_braces_outside_literals(line, &mut in_bc) {
                depth += o as isize - c as isize;
                if saw {
                    in_test = true;
                    pending = false;
                    if depth <= 0 {
                        in_test = false;
                        depth = 0;
                    }
                }
            }
            continue;
        }
        if in_test {
            if let Some((o, c, _)) = scan_braces_outside_literals(line, &mut in_bc) {
                depth += o as isize - c as isize;
                if depth <= 0 {
                    in_test = false;
                    depth = 0;
                }
            }
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn find_disallowed_tr_suffix(src: &str) -> Option<String> {
    const ALLOWED: &[&str] = &[
        "i18n_tr",
        "i18n_tr_js",
        "SCREENCAST_I18N_TR",
        "screencast_gui_i18n_tr",
        "tr_TR",
    ];
    let bytes = src.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let ident = &src[start..i];
            if (ident.ends_with("_tr") || ident.ends_with("_TR"))
                && !ALLOWED.contains(&ident)
                && ident.len() > 3
            {
                return Some(ident.to_string());
            }
        } else {
            i += 1;
        }
    }
    None
}

#[test]
fn gate_all_source_and_asset_files_within_350_non_test_lines() {
    let root = repo_root();
    let mut rs_files = Vec::new();
    collect_files(&root.join("crates"), &["rs"], &mut rs_files);
    assert!(!rs_files.is_empty());

    let mut violations = Vec::new();
    for path in rs_files {
        let rel = path
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .to_string();
        if rel.contains("/tests/") {
            continue;
        }
        let src = fs::read_to_string(&path).unwrap();
        let lines = count_rust_non_test_lines(&src);
        if lines > MAX_LINES {
            violations.push(format!("{rel}: {lines} non-test lines (> {MAX_LINES})"));
        }
    }

    let mut asset_files = Vec::new();
    collect_files(
        &root.join("assets/player"),
        &["html", "css", "js"],
        &mut asset_files,
    );
    collect_files(&root.join("assets/gui"), &["py"], &mut asset_files);
    for path in asset_files {
        let rel = path
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .to_string();
        let src = fs::read_to_string(&path).unwrap();
        let lines = src.lines().count();
        if lines > MAX_LINES {
            violations.push(format!("{rel}: {lines} lines (> {MAX_LINES})"));
        }
    }

    assert!(
        violations.is_empty(),
        "350-line AI-driven modularization violations:\n{}",
        violations.join("\n")
    );
}

#[test]
fn gate_zero_turkish_chars_and_tr_suffixes_outside_catalogs_and_vectors() {
    let root = repo_root();
    let mut files = Vec::new();
    collect_files(&root.join("crates"), &["rs", "sh", "toml"], &mut files);
    collect_files(
        &root.join("assets"),
        &["html", "css", "js", "py"],
        &mut files,
    );
    collect_files(&root.join("docs"), &["md"], &mut files);
    collect_files(&root.join("packaging"), &["sh", "yml"], &mut files);
    collect_files(&root.join(".github"), &["yml"], &mut files);
    files.push(root.join("AGENTS.md"));
    files.push(root.join("README.md"));
    files.push(root.join("Cargo.toml"));

    let mut violations = Vec::new();
    for path in files {
        let rel = path
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .to_string();
        if rel == "crates/screencast-cli/tests/ai_hygiene.rs"
            || rel == "crates/screencast-cli/tests/i18n_cli.rs"
        {
            continue;
        }
        let src = fs::read_to_string(&path).unwrap();
        if let Some(bad_ident) = find_disallowed_tr_suffix(&src) {
            violations.push(format!("{rel}: disallowed `_tr` identifier `{bad_ident}`"));
        }
        if is_dedicated_tr_catalog(&rel) {
            continue;
        }
        let to_check = if is_unicode_test_vector_file(&rel) {
            strip_cfg_test_blocks(&src)
        } else {
            src
        };
        if contains_turkish_char(&to_check) {
            violations.push(format!(
                "{rel}: contains Turkish character outside dedicated `tr` i18n catalog"
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "i18n / English-only hygiene violations:\n{}",
        violations.join("\n")
    );
}

#[test]
fn gate_one_way_crate_dependency_direction_and_legacy_cleanup() {
    let root = repo_root();
    assert!(
        !root.join("internal").exists(),
        "duplicate root `internal/` directory must stay removed"
    );
    assert!(
        root.join("legacy/prototype-go/internal").exists(),
        "`legacy/prototype-go/` must remain preserved"
    );

    let middle_crates = [
        "screencast-audio",
        "screencast-capture",
        "screencast-encode",
        "screencast-sinks",
        "screencast-transport",
        "screencast-ui",
    ];

    let core_toml = fs::read_to_string(root.join("crates/screencast-core/Cargo.toml")).unwrap();
    let core_deps = parse_workspace_deps(&core_toml);
    assert!(
        core_deps.is_empty(),
        "screencast-core must have zero internal crate dependencies, found: {core_deps:?}"
    );

    for krate in middle_crates {
        let toml = fs::read_to_string(root.join(format!("crates/{krate}/Cargo.toml"))).unwrap();
        let deps = parse_workspace_deps(&toml);
        assert!(
            deps.len() == 1 && deps.contains("screencast-core"),
            "{krate} may depend ONLY on screencast-core among workspace crates, found: {deps:?}"
        );
    }

    let cli_toml = fs::read_to_string(root.join("crates/screencast-cli/Cargo.toml")).unwrap();
    let cli_deps = parse_workspace_deps(&cli_toml);
    assert!(!cli_deps.contains("screencast-cli"));
    assert!(cli_deps.contains("screencast-core"));
    for krate in middle_crates {
        assert!(
            cli_deps.contains(krate),
            "screencast-cli should wire middle crate {krate}, found: {cli_deps:?}"
        );
    }
}

fn parse_workspace_deps(cargo_toml: &str) -> BTreeSet<String> {
    let mut deps = BTreeSet::new();
    let mut in_deps = false;
    for line in cargo_toml.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_deps = t == "[dependencies]"
                || t == "[dev-dependencies]"
                || t == "[build-dependencies]"
                || (t.starts_with("[target.") && t.ends_with(".dependencies]"));
            continue;
        }
        if in_deps {
            if let Some((name, _)) = t.split_once('=') {
                let n = name.trim();
                if n.starts_with("screencast-") {
                    deps.insert(n.to_string());
                }
            }
        }
    }
    deps
}
