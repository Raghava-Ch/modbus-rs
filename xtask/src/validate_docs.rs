//! Automatic documentation example validation.
//!
//! Scans every `.md` file in the repository and:
//!
//! 1. Extracts `cargo run/check/build --example` commands from bash/shell blocks.
//! 2. Extracts Rust fenced code blocks and compile-checks complete programs.
//! 3. Cross-references documented examples against `[[example]]` entries in Cargo.toml.
//!
//! ## Markdown conventions
//!
//! Place an HTML comment **on the line immediately before** a code fence to control
//! validation behaviour:
//!
//! | Marker | Effect |
//! |---|---|
//! | `<!-- validate: run -->` | Execute the command instead of just compile‐checking |
//! | `<!-- validate: skip -->` | Skip the block entirely |
//! | `<!-- validate: compile -->` | Force compile‐check (useful for snippets without `fn main`) |
//!
//! Rust fence modifiers (after the language tag):
//!
//! | Tag | Effect |
//! |---|---|
//! | ` ```rust ` | Compile‐check **only** if the block contains `fn main` |
//! | ` ```rust,no_run ` | Always compile‐check, never run |
//! | ` ```rust,ignore ` | Skip entirely |

use std::collections::HashSet;
use std::fs;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

// ═══════════════════════════════════════════════════════════════════════
//  Output styling
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
    if use_color() {
        format!("{code}{s}{ANSI_RESET}")
    } else {
        s.to_string()
    }
}

fn ok(s: &str) -> String {
    paint(s, ANSI_GREEN)
}

fn warn(s: &str) -> String {
    paint(s, ANSI_YELLOW)
}

fn err(s: &str) -> String {
    paint(s, ANSI_RED)
}

fn section(s: &str) -> String {
    if use_color() {
        format!("{ANSI_BOLD}{ANSI_CYAN}{s}{ANSI_RESET}")
    } else {
        s.to_string()
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Types
// ═══════════════════════════════════════════════════════════════════════

/// A fenced code block parsed from a Markdown file.
struct CodeBlock {
    file: PathBuf,
    line: usize,
    lang: String,
    tags: Vec<String>,
    code: String,
    /// A `<!-- validate: XXX -->` on the line immediately before the fence.
    marker: Option<String>,
}

/// A `cargo run/check/build --example …` command extracted from a bash block.
struct CargoCmd {
    file: PathBuf,
    line: usize,
    example: String,
    package: String,
    features: Vec<String>,
    no_default_features: bool,
    should_run: bool,
}

// ═══════════════════════════════════════════════════════════════════════
//  Phase 1 — Scan
// ═══════════════════════════════════════════════════════════════════════

fn scan_md_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk_dir(root, &mut out);
    out.sort();
    out
}

fn walk_dir(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for entry in rd.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name();
            let s = name.to_string_lossy();
            if !matches!(s.as_ref(), "target" | ".git" | "node_modules" | "pkg") {
                walk_dir(&path, out);
            }
        } else if path.extension().is_some_and(|e| e == "md") {
            out.push(path);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Phase 2 — Parse fenced code blocks
// ═══════════════════════════════════════════════════════════════════════

fn parse_code_blocks(file: &Path, content: &str) -> Vec<CodeBlock> {
    let mut blocks = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;
    let mut pending_marker: Option<String> = None;

    while i < lines.len() {
        let trimmed = lines[i].trim();

        // Check for a validate marker
        if let Some(m) = extract_marker(trimmed) {
            pending_marker = Some(m);
            i += 1;
            continue;
        }

        // Try to detect an opening fence
        if let Some((fence_ch, fence_n, info)) = detect_open_fence(trimmed) {
            let block_line = i + 1; // 1-based
            let (lang, tags) = parse_info_string(info);
            let marker = pending_marker.take();
            let mut code = String::new();
            i += 1;

            // Collect lines until the matching closing fence
            while i < lines.len() {
                if is_close_fence(lines[i].trim(), fence_ch, fence_n) {
                    break;
                }
                code.push_str(lines[i]);
                code.push('\n');
                i += 1;
            }

            blocks.push(CodeBlock {
                file: file.to_path_buf(),
                line: block_line,
                lang,
                tags,
                code,
                marker,
            });
        } else if !trimmed.is_empty() {
            // Non-empty, non-fence, non-marker line → discard stale marker
            pending_marker = None;
        }

        i += 1;
    }

    blocks
}

/// Parse `<!-- validate: run|skip|compile -->` from a line.
fn extract_marker(line: &str) -> Option<String> {
    let inner = line.strip_prefix("<!--")?.strip_suffix("-->")?.trim();
    let value = inner.strip_prefix("validate:")?.trim();
    if value.is_empty() {
        return None;
    }
    Some(value.to_lowercase())
}

/// Detect an opening fence (``` or ~~~, ≥ 3 chars) and return (char, count, info_string).
fn detect_open_fence(line: &str) -> Option<(char, usize, &str)> {
    let ch = line.chars().next()?;
    if ch != '`' && ch != '~' {
        return None;
    }
    let n = line.chars().take_while(|&c| c == ch).count();
    if n < 3 {
        return None;
    }
    Some((ch, n, line[n..].trim()))
}

/// Check whether a line closes a fence opened with `fence_ch` repeated `min` times.
fn is_close_fence(line: &str, fence_ch: char, min: usize) -> bool {
    let first = line.chars().next().unwrap_or(' ');
    if first != fence_ch {
        return false;
    }
    let n = line.chars().take_while(|&c| c == fence_ch).count();
    n >= min && line[n..].trim().is_empty()
}

/// Split `"rust,no_run"` → `("rust", ["no_run"])`.
fn parse_info_string(info: &str) -> (String, Vec<String>) {
    if info.is_empty() {
        return (String::new(), Vec::new());
    }
    let parts: Vec<&str> = info.split(',').map(str::trim).collect();
    let lang = parts[0].to_lowercase();
    let tags = parts[1..].iter().map(|s| s.trim().to_lowercase()).collect();
    (lang, tags)
}

// ═══════════════════════════════════════════════════════════════════════
//  Helper — Extract warnings from compiler output
// ═══════════════════════════════════════════════════════════════════════

fn extract_warnings(stderr: &str) -> Vec<String> {
    stderr
        .lines()
        .filter(|l| l.contains("warning:"))
        .map(|l| l.trim().to_string())
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════
//  Phase 3 — Extract and validate cargo example commands
// ═══════════════════════════════════════════════════════════════════════

fn extract_cargo_commands(blocks: &[CodeBlock]) -> Vec<CargoCmd> {
    let mut cmds = Vec::new();

    for block in blocks {
        // Only bash/shell blocks
        if !matches!(block.lang.as_str(), "bash" | "shell" | "sh" | "") {
            continue;
        }
        if block.marker.as_deref() == Some("skip") {
            continue;
        }
        let should_run = block.marker.as_deref() == Some("run");

        // Join line continuations (backslash at end-of-line)
        let joined = block.code.replace("\\\n", " ");

        for raw_line in joined.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            // Strip leading env vars / shell prompt to find `cargo`
            let Some(cargo_pos) = line.find("cargo ") else {
                continue;
            };
            let cmd_str = &line[cargo_pos..];

            if !cmd_str.contains("--example") {
                continue;
            }

            if let Some(parsed) = parse_cargo_cmd(cmd_str, &block.file, block.line, should_run) {
                cmds.push(parsed);
            }
        }
    }

    cmds
}

/// Parse a single `cargo run/check/build --example NAME …` string.
fn parse_cargo_cmd(cmd: &str, file: &Path, line: usize, should_run: bool) -> Option<CargoCmd> {
    // Truncate at pipe, redirect, semicolon, or `&&`
    let cmd = cmd
        .split('|')
        .next()
        .unwrap_or(cmd)
        .split(';')
        .next()
        .unwrap_or(cmd);
    let cmd = if let Some(pos) = cmd.find("&&") {
        &cmd[..pos]
    } else {
        cmd
    };

    let tokens: Vec<&str> = cmd.split_whitespace().collect();
    if tokens.len() < 3 || tokens[0] != "cargo" {
        return None;
    }
    if !matches!(tokens[1], "run" | "check" | "build") {
        return None;
    }

    let mut example = None;
    let mut package = String::from("modbus-rs");
    let mut features = Vec::new();
    let mut no_default_features = false;

    let mut i = 2;
    while i < tokens.len() {
        match tokens[i] {
            "--" => break, // everything after `--` is program args
            "--example" => {
                example = tokens.get(i + 1).map(|s| s.to_string());
                i += 2;
            }
            "-p" | "--package" => {
                if let Some(p) = tokens.get(i + 1) {
                    package = p.to_string();
                }
                i += 2;
            }
            "--features" => {
                if let Some(f) = tokens.get(i + 1) {
                    features.extend(f.split(',').map(String::from));
                }
                i += 2;
            }
            "--no-default-features" => {
                no_default_features = true;
                i += 1;
            }
            t if t.starts_with("--features=") => {
                let f = t.strip_prefix("--features=").unwrap();
                features.extend(f.split(',').map(String::from));
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }

    Some(CargoCmd {
        file: file.to_path_buf(),
        line,
        example: example?,
        package,
        features,
        no_default_features,
        should_run,
    })
}

/// Validate each unique `cargo --example` command by running `cargo check`
/// (or `cargo run` if `<!-- validate: run -->` was set).
/// Returns: (passed, failed, failures, warnings).
fn validate_cargo_commands(root: &Path, cmds: &[CargoCmd]) -> (u32, u32, Vec<String>, Vec<String>) {
    let mut seen = HashSet::new();
    let mut unique: Vec<&CargoCmd> = Vec::new();

    for cmd in cmds {
        let key = format!(
            "{}:{}:{}:{}",
            cmd.package,
            cmd.example,
            cmd.features.join(","),
            cmd.no_default_features
        );
        if seen.insert(key) {
            unique.push(cmd);
        }
    }

    let mut passed = 0u32;
    let mut failed = 0u32;
    let mut failures = Vec::new();
    let mut warnings = Vec::new();

    for cmd in &unique {
        let feat_display = if cmd.features.is_empty() {
            "default".into()
        } else {
            cmd.features.join(",")
        };
        let rel = cmd.file.strip_prefix(root).unwrap_or(&cmd.file);
        let verb = if cmd.should_run { "run " } else { "check" };

        print!(
            "  {} -p {} --example {} [{}] ({}:{})… ",
            verb,
            cmd.package,
            cmd.example,
            feat_display,
            rel.display(),
            cmd.line,
        );
        std::io::stdout().flush().ok();

        let mut cargo = Command::new("cargo");
        cargo.current_dir(root);

        if cmd.should_run {
            cargo.arg("run");
        } else {
            cargo.arg("check");
        }
        cargo.args(["-p", &cmd.package, "--example", &cmd.example]);

        if cmd.no_default_features {
            cargo.arg("--no-default-features");
        }
        if !cmd.features.is_empty() {
            cargo.args(["--features", &cmd.features.join(",")]);
        }

        match cargo.output() {
            Ok(o) if o.status.success() => {
                println!("{}", ok("✓"));
                passed += 1;
                
                // Capture warnings even on success
                let stderr = String::from_utf8_lossy(&o.stderr);
                let block_warnings = extract_warnings(&stderr);
                for w in block_warnings {
                    warnings.push(format!(
                        "{} ({}:{}): {}",
                        cmd.example,
                        rel.display(),
                        cmd.line,
                        w,
                    ));
                }
            }
            Ok(o) => {
                println!("{}", err("✗"));
                let stderr = String::from_utf8_lossy(&o.stderr);
                let first = stderr
                    .lines()
                    .find(|l| l.contains("error"))
                    .unwrap_or("unknown error")
                    .trim();
                failures.push(format!(
                    "{} ({}:{}): {}",
                    cmd.example,
                    rel.display(),
                    cmd.line,
                    first,
                ));
                failed += 1;
            }
            Err(e) => {
                println!("{}", err("✗"));
                failures.push(format!("{}: {}", cmd.example, e));
                failed += 1;
            }
        }
    }

    (passed, failed, failures, warnings)
}

// ═══════════════════════════════════════════════════════════════════════
//  Phase 4 — Extract and compile-check Rust code blocks
// ═══════════════════════════════════════════════════════════════════════

/// Classify Rust blocks into compilable (has `fn main` or forced) vs skipped.
fn classify_rust_blocks(blocks: &[CodeBlock]) -> (Vec<&CodeBlock>, usize) {
    let mut compilable: Vec<&CodeBlock> = Vec::new();
    let mut skipped = 0usize;

    for block in blocks {
        if block.lang != "rust" {
            continue;
        }
        if block.tags.iter().any(|t| t == "ignore") {
            skipped += 1;
            continue;
        }
        if block.marker.as_deref() == Some("skip") {
            skipped += 1;
            continue;
        }

        let has_main = block.code.contains("fn main");
        let force =
            block.tags.iter().any(|t| t == "no_run") || block.marker.as_deref() == Some("compile");

        if has_main || force {
            compilable.push(block);
        } else {
            skipped += 1;
        }
    }

    (compilable, skipped)
}

/// Compile-check a list of Rust code blocks by writing temp files into
/// `modbus-rs/examples/` and running individual `cargo check` calls.
///
/// Temp files are cleaned up even on failure.
/// Returns: (passed, failed, failures, warnings).
fn validate_rust_blocks(root: &Path, blocks: &[&CodeBlock]) -> (u32, u32, Vec<String>, Vec<String>) {
    if blocks.is_empty() {
        return (0, 0, Vec::new(), Vec::new());
    }

    let examples_dir = root.join("modbus-rs").join("examples");
    let mut temp_files: Vec<PathBuf> = Vec::new();
    let mut passed = 0u32;
    let mut failed = 0u32;
    let mut failures: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    // Ensure cleanup runs no matter what
    let result = (|| -> Result<(), String> {
        for (i, block) in blocks.iter().enumerate() {
            let name = format!("_dv_{:03}", i);
            let path = examples_dir.join(format!("{name}.rs"));

            // Build the temp file content
            let mut content = String::from(
                "#![allow(unused_imports, unused_variables, dead_code, unused_mut, unreachable_code, unused_assignments)]\n",
            );
            content.push_str(&block.code);
            if !block.code.contains("fn main") {
                content.push_str("\n#[allow(dead_code)]\nfn main() {}\n");
            }

            fs::write(&path, &content).map_err(|e| format!("write {}: {e}", path.display()))?;
            temp_files.push(path);

            let rel = block.file.strip_prefix(root).unwrap_or(&block.file);
            print!("  [{}:{:>3}] _dv_{:03}… ", rel.display(), block.line, i,);
            std::io::stdout().flush().ok();

            let output = Command::new("cargo")
                .current_dir(root)
                .args([
                    "check",
                    "-p",
                    "modbus-rs",
                    "--example",
                    &name,
                    "--all-features",
                ])
                .output();

            match output {
                Ok(o) if o.status.success() => {
                    println!("{}", ok("✓"));
                    passed += 1;
                    
                    // Capture warnings even on success
                    let stderr = String::from_utf8_lossy(&o.stderr);
                    let block_warnings = extract_warnings(&stderr);
                    for w in block_warnings {
                        warnings.push(format!("{}:{}: {}", rel.display(), block.line, w));
                    }
                }
                Ok(o) => {
                    println!("{}", err("✗"));
                    let stderr = String::from_utf8_lossy(&o.stderr);
                    // Extract first real error line
                    let first_err = stderr
                        .lines()
                        .find(|l| l.starts_with("error"))
                        .unwrap_or("compilation failed")
                        .trim();
                    failures.push(format!("{}:{}: {}", rel.display(), block.line, first_err,));
                    failed += 1;
                }
                Err(e) => {
                    println!("{}", err("✗"));
                    failures.push(format!("{}:{}: {}", rel.display(), block.line, e));
                    failed += 1;
                }
            }
        }
        Ok(())
    })();

    // Cleanup temp files unconditionally
    for path in &temp_files {
        let _ = fs::remove_file(path);
    }

    if let Err(e) = result {
        failures.push(format!("internal error: {e}"));
    }

    (passed, failed, failures, warnings)
}

// ═══════════════════════════════════════════════════════════════════════
//  Phase 5 — Cross-reference
// ═══════════════════════════════════════════════════════════════════════

/// Read all `[[example]] name = "…"` entries from `modbus-rs/Cargo.toml`.
fn parse_cargo_toml_examples(root: &Path) -> Vec<String> {
    let path = root.join("modbus-rs/Cargo.toml");
    let Ok(content) = fs::read_to_string(path) else {
        return Vec::new();
    };

    let mut names = Vec::new();
    let mut in_example_block = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "[[example]]" {
            in_example_block = true;
            continue;
        }
        if in_example_block && trimmed.starts_with("name") {
            if let Some(name) = trimmed.split('"').nth(1) {
                names.push(name.to_string());
            }
            in_example_block = false;
        }
        // A blank line or new section header ends the block
        if in_example_block && (trimmed.is_empty() || trimmed.starts_with('[')) {
            in_example_block = false;
        }
    }

    names
}

/// Compare Cargo.toml examples against the set of examples mentioned in docs.
/// Returns `(undocumented, phantom)` where:
/// - **undocumented**: in Cargo.toml but never referenced in any markdown
/// - **phantom**: referenced in docs but absent from Cargo.toml
fn cross_reference(
    toml_examples: &[String],
    doc_examples: &HashSet<String>,
) -> (Vec<String>, Vec<String>) {
    let toml_set: HashSet<&str> = toml_examples.iter().map(String::as_str).collect();
    let doc_set: HashSet<&str> = doc_examples.iter().map(String::as_str).collect();

    let mut undocumented: Vec<String> = toml_set
        .difference(&doc_set)
        .map(|s| s.to_string())
        .collect();
    undocumented.sort();

    let mut phantom: Vec<String> = doc_set
        .difference(&toml_set)
        .map(|s| s.to_string())
        .collect();
    phantom.sort();

    (undocumented, phantom)
}

// ═══════════════════════════════════════════════════════════════════════
//  Orchestrator
// ═══════════════════════════════════════════════════════════════════════

pub fn cmd_validate_docs(root: &Path) -> Result<(), String> {
    println!("\n╔═══════════════════════════════════════════╗");
    println!(
        "║   {}        ║",
        section("Documentation Example Validation")
    );
    println!("╚═══════════════════════════════════════════╝\n");

    // ── Phase 1: Scan ────────────────────────────────────────────────
    let md_files = scan_md_files(root);
    println!("Scanned {} markdown files\n", md_files.len());

    // ── Phase 2: Parse ───────────────────────────────────────────────
    let mut all_blocks: Vec<CodeBlock> = Vec::new();
    for file in &md_files {
        let Ok(content) = fs::read_to_string(file) else {
            continue;
        };
        all_blocks.extend(parse_code_blocks(file, &content));
    }

    let rust_n = all_blocks.iter().filter(|b| b.lang == "rust").count();
    let bash_n = all_blocks
        .iter()
        .filter(|b| matches!(b.lang.as_str(), "bash" | "shell" | "sh"))
        .count();
    println!(
        "Found {} code blocks ({} Rust, {} bash/shell)\n",
        all_blocks.len(),
        rust_n,
        bash_n
    );

    let mut total_fail = 0u32;

    // ── Phase 3: Cargo example commands ──────────────────────────────
    println!(
        "{}\n",
        section("── Cargo Example Commands ──────────────────────")
    );

    let cargo_cmds = extract_cargo_commands(&all_blocks);
    let doc_example_names: HashSet<String> = cargo_cmds.iter().map(|c| c.example.clone()).collect();

    println!(
        "Extracted {} commands ({} unique examples)\n",
        cargo_cmds.len(),
        doc_example_names.len()
    );

    let (cargo_pass, cargo_fail, cargo_failures, cargo_warnings) = validate_cargo_commands(root, &cargo_cmds);
    total_fail += cargo_fail;

    // ── Phase 4: Rust code blocks ────────────────────────────────────
    println!(
        "\n{}\n",
        section("── Rust Code Blocks ───────────────────────────")
    );

    let (compilable, skipped_n) = classify_rust_blocks(&all_blocks);
    println!(
        "Compilable: {} (fn main / no_run / forced), Skipped: {}\n",
        compilable.len(),
        skipped_n
    );

    let (rust_pass, rust_fail, rust_failures, rust_warnings) = validate_rust_blocks(root, &compilable);
    total_fail += rust_fail;

    // ── Phase 5: Cross-reference ─────────────────────────────────────
    println!(
        "\n{}\n",
        section("── Cross-Reference ────────────────────────────")
    );

    let toml_examples = parse_cargo_toml_examples(root);
    println!(
        "Cargo.toml has {} [[example]] entries, docs reference {} unique examples\n",
        toml_examples.len(),
        doc_example_names.len()
    );

    let (undocumented, phantom) = cross_reference(&toml_examples, &doc_example_names);

    if !undocumented.is_empty() {
        println!(
            "  {} Undocumented examples (in Cargo.toml but not in any .md):",
            warn("⚠")
        );
        for name in &undocumented {
            println!("    - {name}");
        }
    }
    if !phantom.is_empty() {
        println!(
            "  {} Phantom examples (in docs but not in Cargo.toml):",
            warn("⚠")
        );
        for name in &phantom {
            println!("    - {name}");
        }
    }
    if undocumented.is_empty() && phantom.is_empty() {
        println!(
            "  {} All examples are documented and all doc references are valid",
            ok("✓")
        );
    }

    // ── Summary ──────────────────────────────────────────────────────
    println!("\n╔═══════════════════════════════════════════╗");
    println!(
        "║              {}                      ║",
        section("Summary")
    );
    println!("╚═══════════════════════════════════════════╝\n");

    println!(
        "  Cargo examples : {} passed, {} failed",
        cargo_pass, cargo_fail
    );
    println!(
        "  Rust blocks    : {} passed, {} failed, {} skipped",
        rust_pass, rust_fail, skipped_n
    );
    println!(
        "  Cross-reference: {} undocumented, {} phantom",
        undocumented.len(),
        phantom.len()
    );

    if !cargo_failures.is_empty() {
        println!("\n  {} Cargo failures:", err("✗"));
        for f in &cargo_failures {
            println!("    {} {f}", err("✗"));
        }
    }
    if !rust_failures.is_empty() {
        println!("\n  {} Rust block failures:", err("✗"));
        for f in &rust_failures {
            println!("    {} {f}", err("✗"));
        }
    }

    // ── Warnings section (bold) ──────────────────────────────────────
    let total_warnings = cargo_warnings.len() + rust_warnings.len();
    if total_warnings > 0 {
        println!("\n");
        println!(
            "  {} {} documentation examples have warnings:",
            warn("⚠"),
            warn(&format!("BOLD: {} total", total_warnings))
        );
        
        if !cargo_warnings.is_empty() {
            println!("\n    {} Cargo example warnings ({}):",
                warn("⚠"),
                cargo_warnings.len()
            );
            for w in &cargo_warnings {
                println!("      {} {w}", warn("⚠"));
            }
        }
        
        if !rust_warnings.is_empty() {
            println!("\n    {} Rust block warnings ({}):",
                warn("⚠"),
                rust_warnings.len()
            );
            for w in &rust_warnings {
                println!("      {} {w}", warn("⚠"));
            }
        }
    }

    println!();

    if total_fail > 0 {
        Err(format!("{total_fail} validation(s) failed"))
    } else {
        println!("{} All validations passed! ({} warnings found)\n", ok("✓"), total_warnings);
        Ok(())
    }
}
