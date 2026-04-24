//! Workspace maintenance commands for header generation/verification and C smoke build checks.

mod check_doc_links;
mod demo_manifest;
mod gen_server_app;
mod validate_docs;

use std::env;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::{Command, ExitCode};

fn repo_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.parent().unwrap_or(&manifest_dir).to_path_buf()
}

fn ffi_include_dir(root: &Path) -> PathBuf {
    root.join("target/mbus-ffi/include")
}

fn run_step(program: &str, args: &[&str], cwd: &Path) -> Result<(), String> {
    let status = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .status()
        .map_err(|e| format!("failed to run {program}: {e}"))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("command failed: {} {}", program, args.join(" ")))
    }
}

fn run_step_with_env(
    program: &str,
    args: &[&str],
    cwd: &Path,
    env: &[(&str, &str)],
) -> Result<(), String> {
    let mut cmd = Command::new(program);
    cmd.args(args).current_dir(cwd);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let status = cmd
        .status()
        .map_err(|e| format!("failed to run {program}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("command failed: {} {}", program, args.join(" ")))
    }
}

fn headers_paths(root: &Path) -> (PathBuf, PathBuf) {
    let include_dir = ffi_include_dir(root);
    let base = include_dir.join("modbus_rs_client.h");
    let gated = include_dir.join("modbus_rs_client_feature_gated.h");
    (base, gated)
}

fn feature_macro_for_declaration(line: &str) -> Option<&'static str> {
    let trimmed = line.trim_start();
    let decl_like = trimmed.starts_with("typedef")
        || trimmed.starts_with("enum ")
        || trimmed.starts_with("const ")
        || trimmed.starts_with("uint")
        || trimmed.starts_with("MbusClientId ")
        || trimmed.starts_with("bool ")
        || trimmed.starts_with("void (*")
        || trimmed.starts_with('}');

    if !decl_like {
        return None;
    }

    let lower = trimmed.to_ascii_lowercase();
    if lower.contains("discrete_input") || lower.contains("discreteinputs") {
        Some("MBUS_FEATURE_DISCRETE_INPUTS")
    } else if lower.contains("file_record")
        || lower.contains("file record")
        || lower.contains("filerecord")
    {
        Some("MBUS_FEATURE_FILE_RECORD")
    } else if lower.contains("diagnostic") {
        Some("MBUS_FEATURE_DIAGNOSTICS")
    } else if lower.contains("register") {
        Some("MBUS_FEATURE_REGISTERS")
    } else if lower.contains("coils") || lower.contains("coil") {
        Some("MBUS_FEATURE_COILS")
    } else if lower.contains("fifo") {
        Some("MBUS_FEATURE_FIFO")
    } else {
        None
    }
}

fn generate_feature_gated_header(base_header: &str) -> String {
    let mut out = String::new();
    let mut current_gate: Option<&'static str> = None;

    let mut in_multiline_decl = false;
    let mut multiline_decl_gate: Option<&'static str> = None;

    let mut in_struct_block = false;
    let mut struct_block_gate: Option<&'static str> = None;

    for line in base_header.lines() {
        let trimmed = line.trim_start();

        let gate = if in_multiline_decl {
            multiline_decl_gate
        } else if in_struct_block {
            struct_block_gate
        } else {
            feature_macro_for_declaration(line)
        };

        if gate != current_gate {
            if current_gate.is_some() {
                out.push_str("#endif\n");
            }
            if let Some(m) = gate {
                out.push_str(&format!("#if defined({m})\n"));
            }
            current_gate = gate;
        }

        out.push_str(line);
        out.push('\n');

        if !in_multiline_decl
            && gate.is_some()
            && trimmed.contains("mbus_")
            && trimmed.contains('(')
            && !trimmed.ends_with(";")
        {
            in_multiline_decl = true;
            multiline_decl_gate = gate;
        }

        if in_multiline_decl && trimmed.ends_with(");") {
            in_multiline_decl = false;
            multiline_decl_gate = None;
        }

        if !in_struct_block
            && gate.is_some()
            && trimmed.starts_with("typedef struct")
            && trimmed.ends_with('{')
        {
            in_struct_block = true;
            struct_block_gate = gate;
        }

        if in_struct_block && trimmed.starts_with('}') && trimmed.ends_with(';') {
            in_struct_block = false;
            struct_block_gate = None;
        }
    }

    if current_gate.is_some() {
        out.push_str("#endif\n");
    }

    let out = out.replacen("#ifndef MODBUS_RS_CLIENT_H", "#ifndef MODBUS_RS_CLIENT_FEATURE_GATED_H", 1);
    out.replacen("#define MODBUS_RS_CLIENT_H", "#define MODBUS_RS_CLIENT_FEATURE_GATED_H", 1)
}

fn cmd_gen_feature_header(root: &Path) -> Result<(), String> {
    let (base, gated) = headers_paths(root);
    let base_header =
        fs::read_to_string(&base).map_err(|e| format!("failed to read {}: {e}", base.display()))?;
    let generated = generate_feature_gated_header(&base_header);
    if let Some(parent) = gated.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    }
    fs::write(&gated, generated)
        .map_err(|e| format!("failed to write {}: {e}", gated.display()))?;
    println!("Feature-gated header regenerated: {}", gated.display());
    Ok(())
}

fn cmd_check_feature_header(root: &Path) -> Result<(), String> {
    let (base, gated) = headers_paths(root);
    let base_header =
        fs::read_to_string(&base).map_err(|e| format!("failed to read {}: {e}", base.display()))?;
    let expected = generate_feature_gated_header(&base_header);
    if !gated.exists() {
        if let Some(parent) = gated.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
        }
        fs::write(&gated, &expected)
            .map_err(|e| format!("failed to write {}: {e}", gated.display()))?;
        println!("Feature-gated header bootstrapped: {}", gated.display());
        return Ok(());
    }
    let current = fs::read_to_string(&gated)
        .map_err(|e| format!("failed to read {}: {e}", gated.display()))?;

    if current == expected {
        println!("OK: modbus_rs_client_feature_gated.h is up to date.");
        Ok(())
    } else {
        Err(
            "modbus_rs_client_feature_gated.h is out of date. Run: cargo run -p xtask -- gen-feature-header"
                .to_string(),
        )
    }
}

fn cmd_gen_header(root: &Path) -> Result<(), String> {
    run_step("bash", &["./scripts/check_header.sh", "--fix"], root)?;
    cmd_gen_feature_header(root)
}

fn cmd_check_header(root: &Path) -> Result<(), String> {
    run_step("bash", &["./scripts/check_header.sh"], root)?;
    cmd_check_feature_header(root)
}

// ── C demo: build ─────────────────────────────────────────────────────────

struct BuildCDemoOpts {
    /// `None` = build all discovered demos.
    demo_name: Option<String>,
    link_static: bool,
    /// `None` = use each demo's own `rust_features`.
    features_override: Option<String>,
    skip_gen: bool,
    no_test: bool,
}

fn parse_build_c_demo_args(args: &[String]) -> Result<BuildCDemoOpts, String> {
    let mut demo_name = None;
    let mut link_static = false;
    let mut features_override = None;
    let mut skip_gen = false;
    let mut no_test = false;

    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--demo" => {
                i += 1;
                demo_name = Some(
                    args.get(i)
                        .ok_or_else(|| "--demo requires a value".to_string())?
                        .clone(),
                );
            }
            "--static"   => link_static = true,
            "--skip-gen" => skip_gen = true,
            "--no-test"  => no_test = true,
            "--features" => {
                i += 1;
                features_override = Some(
                    args.get(i)
                        .ok_or_else(|| "--features requires a value".to_string())?
                        .clone(),
                );
            }
            // positional: first non-flag argument is the demo name
            other if !other.starts_with('-') => {
                if demo_name.is_some() {
                    return Err(format!("unexpected positional argument: {other}"));
                }
                demo_name = Some(other.to_string());
            }
            other => return Err(format!("unknown flag for build-c-demo: {other}")),
        }
        i += 1;
    }

    Ok(BuildCDemoOpts { demo_name, link_static, features_override, skip_gen, no_test })
}

fn build_one_demo(
    root: &Path,
    demo: &demo_manifest::DemoManifest,
    opts: &BuildCDemoOpts,
) -> Result<(), String> {
    let features = opts.features_override.as_deref().unwrap_or(&demo.rust_features);

    // Step 1: optional codegen (generates the C header only; Rust dispatcher is
    // generated at compile time by build.rs via MBUS_SERVER_APP_CONFIG).
    if let (Some(cg), false) = (&demo.codegen, opts.skip_gen) {
        let mut gen_args = vec![
            "--config".to_string(), cg.config.clone(),
            "--emit-c-header".to_string(), cg.header.clone(),
        ];
        // If out_dir is still specified in demo.yaml, honour it for manual inspection.
        if let Some(out_dir) = &cg.out_dir {
            gen_args.push("--out-dir".to_string());
            gen_args.push(out_dir.clone());
        }
        cmd_gen_server_app(root, &gen_args)?;
    }

    // Step 2: compile Rust library.
    // Pass MBUS_SERVER_APP_CONFIG so build.rs can generate the server dispatcher.
    let config_abs: Option<String> = demo.codegen.as_ref().map(|cg| {
        root.join(&cg.config).to_string_lossy().into_owned()
    });
    let env_pair: Vec<(&str, &str)> = config_abs
        .as_deref()
        .map(|v| vec![("MBUS_SERVER_APP_CONFIG", v)])
        .unwrap_or_default();
    run_step_with_env(
        "cargo",
        &["build", "-p", "mbus-ffi", "--features", features],
        root,
        &env_pair,
    )?;

    // Step 3: cmake configure + build.
    let build_name  = if opts.link_static { "build-static" } else { "build" };
    let static_flag = if opts.link_static { "-DMBUS_FFI_LINK_STATIC=ON" } else { "-DMBUS_FFI_LINK_STATIC=OFF" };
    run_step("cmake", &["-S", ".", "-B", build_name, static_flag], &demo.dir)?;
    run_step("cmake", &["--build", build_name], &demo.dir)?;

    // Step 4: optional CTest.
    if !opts.no_test {
        run_step("ctest", &["--test-dir", build_name, "--output-on-failure"], &demo.dir)?;
    }

    println!(
        "Demo '{}' {} at {}",
        demo.name,
        if opts.no_test { "built" } else { "built and tested" },
        demo.binary_path(opts.link_static).display(),
    );
    Ok(())
}

fn cmd_build_c_demo(root: &Path, args: &[String]) -> Result<(), String> {
    let opts = parse_build_c_demo_args(args)?;
    let all_demos = demo_manifest::discover_demos(root)?;
    if all_demos.is_empty() {
        return Err("no demo.yaml files found under mbus-ffi/examples/".to_string());
    }
    let to_build: Vec<&demo_manifest::DemoManifest> = match &opts.demo_name {
        Some(name) => vec![demo_manifest::find_demo(&all_demos, name)?],
        None       => all_demos.iter().collect(),
    };
    for demo in to_build {
        build_one_demo(root, demo, &opts)?;
    }
    Ok(())
}

// ── C demo: run ────────────────────────────────────────────────────────────

struct RunCDemoOpts {
    demo_name:   Option<String>,
    link_static: bool,
    mode:        Option<String>,
}

fn parse_run_c_demo_args(args: &[String]) -> Result<RunCDemoOpts, String> {
    let mut demo_name   = None;
    let mut link_static = false;
    let mut mode        = None;

    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--demo" => {
                i += 1;
                demo_name = Some(
                    args.get(i)
                        .ok_or_else(|| "--demo requires a value".to_string())?
                        .clone(),
                );
            }
            "--static" => link_static = true,
            "--mode" => {
                i += 1;
                mode = Some(
                    args.get(i)
                        .ok_or_else(|| "--mode requires a value".to_string())?
                        .clone(),
                );
            }
            // positional: first non-flag argument is the demo name
            other if !other.starts_with('-') => {
                if demo_name.is_some() {
                    return Err(format!("unexpected positional argument: {other}"));
                }
                demo_name = Some(other.to_string());
            }
            other => return Err(format!("unknown flag for run-c-demo: {other}")),
        }
        i += 1;
    }

    Ok(RunCDemoOpts { demo_name, link_static, mode })
}

fn cmd_run_c_demo(root: &Path, args: &[String]) -> Result<(), String> {
    let opts      = parse_run_c_demo_args(args)?;
    let all_demos = demo_manifest::discover_demos(root)?;
    if all_demos.is_empty() {
        return Err("no demo.yaml files found under mbus-ffi/examples/".to_string());
    }

    let demo: &demo_manifest::DemoManifest = match &opts.demo_name {
        Some(name) => demo_manifest::find_demo(&all_demos, name)?,
        None => {
            if all_demos.len() == 1 {
                &all_demos[0]
            } else {
                let names: Vec<&str> = all_demos.iter().map(|d| d.name.as_str()).collect();
                return Err(format!(
                    "--demo is required when multiple demos exist. Available: {}",
                    names.join(", ")
                ));
            }
        }
    };

    let run_cfg = demo.run.as_ref().ok_or_else(|| {
        format!("demo '{}' has no run modes defined in demo.yaml", demo.name)
    })?;

    let mode_name = opts.mode.as_deref().unwrap_or(&run_cfg.default_mode);
    let run_mode  = run_cfg.modes.get(mode_name).ok_or_else(|| {
        let available: Vec<&str> = run_cfg.modes.keys().map(String::as_str).collect();
        format!(
            "mode '{mode_name}' not found in demo '{}'. Available: {}",
            demo.name,
            available.join(", ")
        )
    })?;

    let binary = demo.binary_path(opts.link_static);

    // Auto-build if the binary is missing.
    if !binary.exists() {
        println!("Binary not found at {}. Building first...", binary.display());
        let build_opts = BuildCDemoOpts {
            demo_name:         Some(demo.name.clone()),
            link_static:       opts.link_static,
            features_override: None,
            skip_gen:          false,
            no_test:           true,
        };
        build_one_demo(root, demo, &build_opts)?;
    }

    println!(
        "Running '{}' (mode: {}) — {}",
        demo.name, mode_name, run_mode.description
    );

    let run_args: Vec<&str> = run_mode.args.iter().map(String::as_str).collect();
    let status = Command::new(&binary)
        .args(&run_args)
        .current_dir(&demo.dir)
        .status()
        .map_err(|e| format!("failed to spawn {}: {e}", binary.display()))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "demo '{}' exited with status {}",
            demo.name,
            status.code().unwrap_or(-1)
        ))
    }
}

// ── C demo: list ───────────────────────────────────────────────────────────

fn cmd_list_c_demos(root: &Path) -> Result<(), String> {
    let demos = demo_manifest::discover_demos(root)?;
    if demos.is_empty() {
        println!("No demos found (add demo.yaml to mbus-ffi/examples/<demo>/).");
        return Ok(());
    }
    for demo in &demos {
        println!("{}  —  {}", demo.name, demo.description);
        println!("  rust_features : {}", demo.rust_features);
        if demo.codegen.is_some() {
            println!("  codegen       : yes");
        }
        if let Some(run) = &demo.run {
            for (name, mode) in &run.modes {
                let tag = if name == &run.default_mode { " [default]" } else { "" };
                println!("  mode '{name}'{tag}  — {}", mode.description);
            }
        }
        println!();
    }
    Ok(())
}

fn cmd_gen_server_app(root: &Path, args: &[String]) -> Result<(), String> {
    let opts = gen_server_app::parse_args(root, args)?;
    gen_server_app::run(&opts)
}

fn cmd_check_server_gen(root: &Path) -> Result<(), String> {
    // Only the C header is checked here; the Rust dispatcher is generated at
    // build time by mbus-ffi/build.rs and lives in OUT_DIR, not the source tree.
    let header_path = root.join("target/mbus-ffi/include/mbus_server_app.h");
    if !header_path.exists() {
        let bootstrap_args = vec![
            "--config".to_string(),
            "mbus-ffi/examples/c_server_demo_yaml/mbus_server_app.example.yaml".to_string(),
            "--emit-c-header".to_string(),
            "target/mbus-ffi/include/mbus_server_app.h".to_string(),
        ];
        cmd_gen_server_app(root, &bootstrap_args)?;
    }

    let args = vec![
        "--config".to_string(),
        "mbus-ffi/examples/c_server_demo_yaml/mbus_server_app.example.yaml".to_string(),
        "--emit-c-header".to_string(),
        "target/mbus-ffi/include/mbus_server_app.h".to_string(),
        "--check".to_string(),
    ];
    cmd_gen_server_app(root, &args)
}

fn cmd_check_feature_matrix(root: &Path) -> Result<(), String> {
    run_step("cargo", &["check", "--features", "full"], root)?;
    run_step("cargo", &["check", "--workspace", "--all-features"], root)?;
    run_step(
        "cargo",
        &["test", "-p", "mbus-client", "--doc", "--all-features"],
        root,
    )?;
    run_step(
        "cargo",
        &["test", "-p", "mbus-server", "--all-features"],
        root,
    )?;
    run_step(
        "cargo",
        &["test", "-p", "mbus-async", "--all-features"],
        root,
    )?;
    Ok(())
}

fn cmd_check_release(root: &Path) -> Result<(), String> {
    cmd_check_header(root)?;
    cmd_check_server_gen(root)?;
    cmd_build_c_demo(root, &["--demo".into(), "c_client_demo".into()])?;
    cmd_build_c_demo(root, &["--demo".into(), "c_server_demo".into()])?;
    cmd_check_feature_matrix(root)?;
    Ok(())
}

fn print_help() {
    println!("xtask — workspace maintenance and C demo tooling");
    println!();
    println!("USAGE:  cargo run -p xtask -- <command> [OPTIONS]");
    println!();
    println!("DEMO COMMANDS");
    println!("  list-c-demos");
    println!("      List all demos discovered from mbus-ffi/examples/*/demo.yaml.");
    println!();
    println!("  build-c-demo [OPTIONS]");
    println!("      Build one or all discovered demos.");
    println!("      --demo <name>     Demo name from demo.yaml  [default: build all]");
    println!("      --static          Link libmbus_ffi.a statically");
    println!("      --features <list> Override Rust crate features for this build");
    println!("      --skip-gen        Skip codegen step even if demo.yaml declares one");
    println!("      --no-test         Skip CTest after build");
    println!();
    println!("  run-c-demo [OPTIONS]");
    println!("      Run a demo binary (auto-builds if binary is missing).");
    println!("      --demo <name>     Demo name from demo.yaml  [required if >1 demo]");
    println!("      --mode <name>     Run mode from demo.yaml   [default: demo's default_mode]");
    println!("      --static          Use the statically-linked build");
    println!();
    println!("CODEGEN COMMANDS");
    println!("  gen-server-app --config <path> [--out-dir <path>] [--emit-c-header <path>] [--check] [--dry-run]");
    println!("      Regenerate the C header from a YAML device config.");
    println!("      The Rust dispatcher is generated at compile time by build.rs (set MBUS_SERVER_APP_CONFIG).");
    println!("      --out-dir is optional; omit it for the normal build.rs-driven workflow.");
    println!("  check-server-gen");
    println!("      Verify the generated mbus_server_app.h matches the current YAML config.");
    println!();
    println!("FFI HEADER COMMANDS");
    println!("  gen-header");
    println!("  check-header");
    println!("  gen-feature-header");
    println!("  check-feature-header");
    println!();
    println!("VALIDATION COMMANDS");
    println!("  check-feature-matrix");
    println!("  check-doc-links [--file/-f <path>] ...");
    println!("      Validate that every local markdown link resolves to an existing file.");
    println!("      --file/-f can be repeated to restrict to specific files.");
    println!("      Paths may be relative to the repo root or absolute.");
    println!("  validate-docs [--file/-f <path>] ...");
    println!("      Validate all docs, or restrict to specific files with --file.");
    println!("      --file/-f can be repeated: --file a.md --file b.md");
    println!("      Paths may be relative to the repo root or absolute.");
    println!("      Cross-reference check is skipped when --file is used.");
    println!("  check-release");
    println!();
}

fn main() -> ExitCode {
    let root = repo_root();
    let mut args = env::args();
    let _bin = args.next();
    let Some(cmd) = args.next() else {
        print_help();
        return ExitCode::from(2);
    };
    let remaining_args: Vec<String> = args.collect();

    let result = match cmd.as_str() {
        "gen-header" => cmd_gen_header(&root),
        "check-header" => cmd_check_header(&root),
        "gen-feature-header" => cmd_gen_feature_header(&root),
        "check-feature-header" => cmd_check_feature_header(&root),
        "list-c-demos"  => cmd_list_c_demos(&root),
        "build-c-demo"  => cmd_build_c_demo(&root, &remaining_args),
        "run-c-demo"    => cmd_run_c_demo(&root, &remaining_args),
        "check-server-gen" => cmd_check_server_gen(&root),
        "gen-server-app"   => cmd_gen_server_app(&root, &remaining_args),
        "check-feature-matrix" => cmd_check_feature_matrix(&root),
        "validate-docs" => validate_docs::cmd_validate_docs(&root, &remaining_args),
        "check-doc-links" => check_doc_links::cmd_check_doc_links(&root, &remaining_args),
        "check-release" => cmd_check_release(&root),
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        other => Err(format!("unknown xtask command: {other}")),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("xtask error: {err}");
            ExitCode::from(1)
        }
    }
}
