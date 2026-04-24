//! Cross-documentation link checker.
//!
//! Scans every `.md` file in the repository (or a specified subset) and
//! validates that every local Markdown link `[text](path)` resolves to a file
//! that actually exists on disk.
//!
//! External links (`http://`, `https://`, `mailto:`), anchor-only links
//! (`#section`), and links inside fenced code blocks are skipped.

use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

// ═══════════════════════════════════════════════════════════════════════
//  Terminal styling (duplicated from validate_docs to keep modules self-
//  contained — both are small xtask-internal helpers)
// ═══════════════════════════════════════════════════════════════════════

const ANSI_RESET: &str = "\x1b[0m";
const ANSI_BOLD: &str = "\x1b[1m";
const ANSI_RED: &str = "\x1b[31m";
const ANSI_GREEN: &str = "\x1b[32m";
const ANSI_YELLOW: &str = "\x1b[33m";
const ANSI_CYAN: &str = "\x1b[36m";

fn use_color() -> bool {
    std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal()
}
fn paint(s: &str, code: &str) -> String {
    if use_color() { format!("{code}{s}{ANSI_RESET}") } else { s.to_string() }
}
fn ok(s: &str) -> String    { paint(s, ANSI_GREEN) }
fn warn(s: &str) -> String  { paint(s, ANSI_YELLOW) }
fn err(s: &str) -> String   { paint(s, ANSI_RED) }
fn section(s: &str) -> String {
    if use_color() { format!("{ANSI_BOLD}{ANSI_CYAN}{s}{ANSI_RESET}") } else { s.to_string() }
}

// ═══════════════════════════════════════════════════════════════════════
//  Argument parsing
// ═══════════════════════════════════════════════════════════════════════

fn parse_args(root: &Path, args: &[String]) -> Result<Vec<PathBuf>, String> {
    let mut files: Vec<PathBuf> = Vec::new();
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--file" | "-f" => {
                i += 1;
                let raw = args
                    .get(i)
                    .ok_or_else(|| "--file requires a path argument".to_string())?;
                let p = PathBuf::from(raw);
                let resolved = if p.is_absolute() { p } else { root.join(p) };
                if !resolved.exists() {
                    return Err(format!("--file '{}' does not exist", resolved.display()));
                }
                files.push(resolved);
            }
            other => return Err(format!("unknown check-doc-links flag: {other}")),
        }
        i += 1;
    }
    Ok(files)
}

// ═══════════════════════════════════════════════════════════════════════
//  File discovery (mirrors validate_docs::scan_md_files)
// ═══════════════════════════════════════════════════════════════════════

fn scan_md_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk(root, &mut out);
    out.sort();
    out
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for entry in rd.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name();
            let s = name.to_string_lossy();
            if !matches!(s.as_ref(), "target" | ".git" | "node_modules" | "pkg") {
                walk(&path, out);
            }
        } else if path.extension().is_some_and(|e| e == "md") {
            out.push(path);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Markdown link extraction
// ═══════════════════════════════════════════════════════════════════════

/// Finds the byte index of the matching `)` that closes an opening `(` whose
/// content starts at the beginning of `s` (the `(` itself has already been
/// consumed by the caller).
fn find_paren_end(s: &str) -> Option<usize> {
    let mut depth = 0i32;
    for (i, ch) in s.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                if depth == 0 {
                    return Some(i);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

/// Strip an optional `"title"` or `'title'` suffix and angle brackets from a
/// raw href string, e.g. `<path/to/file> "My Title"` → `path/to/file`.
fn clean_href(raw: &str) -> &str {
    let h = raw.trim();
    // Strip < > angle brackets used for href delimiters
    let h = h.strip_prefix('<').and_then(|s| s.strip_suffix('>')).unwrap_or(h);
    // Strip optional title after a space: `url "title"` → `url`
    let h = if let Some(pos) = h.find(" \"").or_else(|| h.find(" '")) {
        &h[..pos]
    } else {
        h
    };
    h.trim()
}

/// Returns `(1-based line number, href string)` for every `[text](href)` link
/// found in `content` that is NOT inside a fenced code block.
fn extract_md_links(content: &str) -> Vec<(usize, String)> {
    let mut links = Vec::new();
    let mut in_code_block = false;

    for (line_idx, line) in content.lines().enumerate() {
        let trimmed = line.trim_start();
        // Track fenced code blocks (``` or ~~~)
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_code_block = !in_code_block;
            continue;
        }
        if in_code_block {
            continue;
        }

        // Scan for `](` within this line
        let mut search_from = 0usize;
        while search_from < line.len() {
            let Some(rel) = line[search_from..].find("](") else { break };
            let href_start = search_from + rel + 2; // skip `](`

            match find_paren_end(&line[href_start..]) {
                Some(paren_len) => {
                    let raw = &line[href_start..href_start + paren_len];
                    let href = clean_href(raw);
                    if !href.is_empty() {
                        links.push((line_idx + 1, href.to_string()));
                    }
                    search_from = href_start + paren_len + 1;
                }
                None => {
                    // No closing paren on this line — skip past the `](`
                    search_from = href_start;
                }
            }
        }
    }
    links
}

// ═══════════════════════════════════════════════════════════════════════
//  Link validation
// ═══════════════════════════════════════════════════════════════════════

struct BrokenLink {
    /// The .md file containing the broken link.
    file: PathBuf,
    /// 1-based line number.
    line: usize,
    /// The raw href as written in the source.
    href: String,
    /// The resolved path we tried to access.
    resolved: PathBuf,
}

/// Returns `Some(resolved)` when `href` is a local link that does not exist,
/// `None` when the link is external, anchor-only, or valid.
fn check_link(source_file: &Path, href: &str, root: &Path) -> Option<PathBuf> {
    // Skip external, anchor-only, and mailto links
    if href.starts_with("http://")
        || href.starts_with("https://")
        || href.starts_with("mailto:")
        || href.starts_with('#')
    {
        return None;
    }

    // Strip anchor fragment
    let path_part = match href.find('#') {
        Some(pos) => &href[..pos],
        None => href,
    };
    if path_part.is_empty() {
        return None;
    }

    // Resolve relative to the source file's directory
    let base = source_file.parent().unwrap_or(root);
    let resolved = base.join(path_part);

    if resolved.exists() { None } else { Some(resolved) }
}

// ═══════════════════════════════════════════════════════════════════════
//  Public entry point
// ═══════════════════════════════════════════════════════════════════════

pub fn cmd_check_doc_links(root: &Path, args: &[String]) -> Result<(), String> {
    let file_filter = parse_args(root, args)?;
    let filtered = !file_filter.is_empty();

    let files: Vec<PathBuf> = if filtered {
        file_filter
    } else {
        scan_md_files(root)
    };

    println!(
        "\n{}\n",
        section("── Check Doc Links ────────────────────────────")
    );

    if filtered {
        println!("Checking {} selected file(s):", files.len());
        for f in &files {
            println!("  {}", f.display());
        }
    } else {
        println!("Scanning {} markdown file(s)…", files.len());
    }
    println!();

    let mut broken: Vec<BrokenLink> = Vec::new();
    let mut total_links = 0usize;

    for file in &files {
        let content = fs::read_to_string(file)
            .map_err(|e| format!("cannot read {}: {e}", file.display()))?;

        for (line, href) in extract_md_links(&content) {
            total_links += 1;
            if let Some(resolved) = check_link(file, &href, root) {
                broken.push(BrokenLink {
                    file: file.clone(),
                    line,
                    href: href.clone(),
                    resolved,
                });
            }
        }
    }

    // ── Report ────────────────────────────────────────────────────────
    if broken.is_empty() {
        println!(
            "  {} All {} local link(s) in {} file(s) are valid",
            ok("✓"),
            total_links,
            files.len()
        );
        Ok(())
    } else {
        println!(
            "  {} {} broken link(s) found (checked {} total across {} file(s)):\n",
            err("✗"),
            broken.len(),
            total_links,
            files.len()
        );
        for b in &broken {
            // Display path relative to repo root when possible
            let display_file = b
                .file
                .strip_prefix(root)
                .unwrap_or(&b.file)
                .display()
                .to_string();
            let display_resolved = b
                .resolved
                .strip_prefix(root)
                .unwrap_or(&b.resolved)
                .display()
                .to_string();
            println!(
                "  {}:{} → {} {}",
                display_file,
                b.line,
                b.href,
                warn(&format!("(resolved: {display_resolved})"))
            );
        }
        println!();
        Err(format!("{} broken link(s) found", broken.len()))
    }
}
