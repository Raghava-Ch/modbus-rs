//! Workspace maintenance commands for header generation/verification and C smoke build checks.

mod check_doc_links;
mod check_feature_subsets;
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

struct GenClientHeaderOpts {
    features: Option<String>,
    out_dir: PathBuf,
    target: Option<String>,
    profile: Option<String>,
}

fn parse_gen_client_header_opts(
    root: &Path,
    args: &[String],
) -> Result<GenClientHeaderOpts, String> {
    let mut features = None;
    let mut out_dir = None;
    let mut target = None;
    let mut profile = None;
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--features" => {
                i += 1;
                features = Some(
                    args.get(i)
                        .ok_or_else(|| "--features requires a value".to_string())?
                        .clone(),
                );
            }
            "--out-dir" => {
                i += 1;
                let path_str = args
                    .get(i)
                    .ok_or_else(|| "--out-dir requires a value".to_string())?;
                out_dir = Some(root.join(path_str));
            }
            "--target" => {
                i += 1;
                target = Some(
                    args.get(i)
                        .ok_or_else(|| "--target requires a value".to_string())?
                        .clone(),
                );
            }
            "--profile" => {
                i += 1;
                let val = args
                    .get(i)
                    .ok_or_else(|| "--profile requires a value".to_string())?
                    .clone();
                if val != "release" && val != "debug" {
                    return Err(format!(
                        "--profile must be 'release' or 'debug', got '{val}'"
                    ));
                }
                profile = Some(val);
            }
            other => {
                return Err(format!("unknown flag for gen-header-lib: {other}"));
            }
        }
        i += 1;
    }
    let out_dir = out_dir.unwrap_or_else(|| root.join("target/mbus-ffi"));
    Ok(GenClientHeaderOpts {
        features,
        out_dir,
        target,
        profile,
    })
}

fn cmd_gen_client_header(root: &Path, args: &[String]) -> Result<(), String> {
    let opts = parse_gen_client_header_opts(root, args)?;
    let mut script_args = vec!["./scripts/check_header.sh", "--fix"];
    let features_arg;
    if let Some(feats) = &opts.features {
        features_arg = format!("--features={feats}");
        script_args.push(&features_arg);
    }
    run_step("bash", &script_args, root)?;

    let profile = opts.profile.as_deref().unwrap_or("release");

    // Build mbus-ffi in selected mode & target
    let mut build_args = vec!["build", "-p", "mbus-ffi"];
    if profile == "release" {
        build_args.push("--release");
    }

    let mut is_bare_metal = false;
    if let Some(target) = &opts.target {
        build_args.push("--target");
        build_args.push(target);
        if target.contains("thumb") || target.contains("none") || target.contains("wasm") {
            is_bare_metal = true;
        }
    }

    if !is_bare_metal {
        // Build as cdylib for standard OSes
        build_args.push("--crate-type=cdylib");
        build_args.push("--crate-type=staticlib");
        build_args.push("--crate-type=rlib");
    }

    let features_str;
    if let Some(feats) = &opts.features {
        features_str = format!("c-client,{feats}");
        build_args.push("--features");
        build_args.push(&features_str);
    } else {
        features_str = "full".to_string();
        build_args.push("--features");
        build_args.push(&features_str);
    }

    println!(
        "Building mbus-ffi for FFI bundling with features: {} (profile={}, target={:?}) ...",
        features_str, profile, opts.target
    );
    run_step("cargo", &build_args, root)?;

    // Create target folders
    let include_dir = opts.out_dir.join("include");
    let library_dir = opts.out_dir.join("library");
    fs::create_dir_all(&include_dir)
        .map_err(|e| format!("failed to create {}: {e}", include_dir.display()))?;
    fs::create_dir_all(&library_dir)
        .map_err(|e| format!("failed to create {}: {e}", library_dir.display()))?;

    // Copy modbus_rs.h
    let src_header = root.join("target/mbus-ffi/include/modbus_rs.h");
    if !src_header.exists() {
        return Err(
            "Generated header modbus_rs.h not found in target/mbus-ffi/include/".to_string(),
        );
    }
    let dest_header = include_dir.join("modbus_rs.h");
    fs::copy(&src_header, &dest_header)
        .map_err(|e| format!("failed to copy header to {}: {e}", dest_header.display()))?;
    println!("  copied header -> {}", dest_header.display());

    // Copy built libraries
    let target_dir = if let Some(target) = &opts.target {
        root.join("target").join(target).join(profile)
    } else {
        root.join("target").join(profile)
    };

    let mut copied_any = false;
    let lib_filenames = &[
        "libmbus_ffi.a",
        "mbus_ffi.lib",
        "libmbus_ffi.so",
        "libmbus_ffi.dylib",
        "mbus_ffi.dll",
    ];
    for filename in lib_filenames {
        let src = target_dir.join(filename);
        if src.exists() {
            let dest = library_dir.join(filename);
            fs::copy(&src, &dest).map_err(|e| {
                format!(
                    "failed to copy library {} to {}: {e}",
                    filename,
                    dest.display()
                )
            })?;
            println!("  copied library -> {}", dest.display());
            copied_any = true;
        }
    }

    if !copied_any {
        return Err(format!(
            "No built library files found in {}",
            target_dir.display()
        ));
    }

    println!("Client FFI bundle is ready at {}", opts.out_dir.display());
    Ok(())
}

fn cmd_check_client_header(root: &Path, args: &[String]) -> Result<(), String> {
    let opts = parse_gen_client_header_opts(root, args)?;
    let mut script_args = vec!["./scripts/check_header.sh"];
    let features_arg;
    if let Some(feats) = &opts.features {
        features_arg = format!("--features={feats}");
        script_args.push(&features_arg);
    }
    run_step("bash", &script_args, root)
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
            "--static" => link_static = true,
            "--skip-gen" => skip_gen = true,
            "--no-test" => no_test = true,
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

    Ok(BuildCDemoOpts {
        demo_name,
        link_static,
        features_override,
        skip_gen,
        no_test,
    })
}

fn build_one_demo(
    root: &Path,
    demo: &demo_manifest::DemoManifest,
    opts: &BuildCDemoOpts,
) -> Result<(), String> {
    let features = opts
        .features_override
        .as_deref()
        .unwrap_or(&demo.rust_features);

    // Step 1: optional codegen (generates the C header only; Rust dispatcher is
    // generated at compile time by build.rs via MBUS_SERVER_APP_CONFIG).
    if let (Some(cg), false) = (&demo.codegen, opts.skip_gen) {
        let mut gen_args = vec![
            "--config".to_string(),
            cg.config.clone(),
            "--emit-c-header".to_string(),
            cg.header.clone(),
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
    let config_abs: Option<String> = demo
        .codegen
        .as_ref()
        .map(|cg| root.join(&cg.config).to_string_lossy().into_owned());
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
    let build_name = if opts.link_static {
        "build-static"
    } else {
        "build"
    };
    let static_flag = if opts.link_static {
        "-DMBUS_FFI_LINK_STATIC=ON"
    } else {
        "-DMBUS_FFI_LINK_STATIC=OFF"
    };
    run_step(
        "cmake",
        &["-S", ".", "-B", build_name, static_flag],
        &demo.dir,
    )?;
    run_step("cmake", &["--build", build_name], &demo.dir)?;

    // Step 4: optional CTest.
    if !opts.no_test {
        run_step(
            "ctest",
            &["--test-dir", build_name, "--output-on-failure"],
            &demo.dir,
        )?;
    }

    println!(
        "Demo '{}' {} at {}",
        demo.name,
        if opts.no_test {
            "built"
        } else {
            "built and tested"
        },
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
        None => all_demos.iter().collect(),
    };
    for demo in to_build {
        build_one_demo(root, demo, &opts)?;
    }
    Ok(())
}

// ── C demo: run ────────────────────────────────────────────────────────────

struct RunCDemoOpts {
    demo_name: Option<String>,
    link_static: bool,
    mode: Option<String>,
}

fn parse_run_c_demo_args(args: &[String]) -> Result<RunCDemoOpts, String> {
    let mut demo_name = None;
    let mut link_static = false;
    let mut mode = None;

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

    Ok(RunCDemoOpts {
        demo_name,
        link_static,
        mode,
    })
}

fn cmd_run_c_demo(root: &Path, args: &[String]) -> Result<(), String> {
    let opts = parse_run_c_demo_args(args)?;
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

    let run_cfg = demo
        .run
        .as_ref()
        .ok_or_else(|| format!("demo '{}' has no run modes defined in demo.yaml", demo.name))?;

    let mode_name = opts.mode.as_deref().unwrap_or(&run_cfg.default_mode);
    let run_mode = run_cfg.modes.get(mode_name).ok_or_else(|| {
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
        println!(
            "Binary not found at {}. Building first...",
            binary.display()
        );
        let build_opts = BuildCDemoOpts {
            demo_name: Some(demo.name.clone()),
            link_static: opts.link_static,
            features_override: None,
            skip_gen: false,
            no_test: true,
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
                let tag = if name == &run.default_mode {
                    " [default]"
                } else {
                    ""
                };
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
    cmd_check_client_header(root, &[])?;
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
    println!(
        "  gen-server-app --config <path> [--out-dir <path>] [--emit-c-header <path>] [--check] [--dry-run]"
    );
    println!("      Regenerate the C header from a YAML device config.");
    println!(
        "      The Rust dispatcher is generated at compile time by build.rs (set MBUS_SERVER_APP_CONFIG)."
    );
    println!("      --out-dir is optional; omit it for the normal build.rs-driven workflow.");
    println!("  FULL MODE (cross-compile + bundle):");
    println!(
        "    gen-server-app --config <path> --target <triple> <output-dir> [--profile release|debug] [--optimize-size]"
    );
    println!("      Parse the YAML, generate artifacts, cross-compile mbus-ffi with");
    println!("      only the features required by the config, and bundle into");
    println!("      <output-dir>/include/ and <output-dir>/lib/.");
    println!("      --target        Target triple (e.g. thumbv7em-none-eabi)");
    println!("      --profile       Build profile: release (default) or debug");
    println!(
        "      --optimize-size Automatically use Nightly Rust and build-std to aggressively shrink binary size"
    );
    println!("      --network-tcp   Enable TCP transport support (feature = \"network-tcp\")");
    println!(
        "      --serial-rtu    Enable RTU serial transport support (feature = \"serial-rtu\")"
    );
    println!(
        "      --serial-ascii  Enable ASCII serial transport support (feature = \"serial-ascii\")"
    );
    println!();
    println!("      <output-dir>    Output directory root (positional argument)");
    println!("  check-server-gen");
    println!("      Verify the generated mbus_server_app.h matches the current YAML config.");
    println!();
    println!("FFI HEADER COMMANDS");
    println!("  gen-header-lib [OPTIONS]");
    println!("      Regenerate modbus_rs.h.");
    println!("      --features <list> Select a custom Rust feature set to expose in the C header.");
    println!(
        "      --out-dir <path>  Output directory root (creates include/ and library/ subdirectories)."
    );
    println!("                        [default: target/mbus-ffi]");
    println!(
        "      --target <triple> Target triple for cross-compilation (e.g. thumbv7em-none-eabi)."
    );
    println!("      --profile <mode>  Build profile: release (default) or debug.");
    println!("  check-client-header [OPTIONS]");
    println!("      Verify that modbus_rs.h is up to date.");
    println!("      --features <list> Select a custom Rust feature set to verify.");
    println!("      --target <triple> Target triple (accepted for compatibility/verification).");
    println!("      --profile <mode>  Build profile (accepted for compatibility/verification).");
    println!();
    println!("VALIDATION COMMANDS");
    println!("  check-feature-matrix");
    println!("  check-feature-subsets [--fast]");
    println!("      Run cargo check / clippy / build / test over every per-feature subset.");
    println!("      --fast  Skip slow steps (build + test); check + clippy only.");
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
        "gen-header-lib" | "gen-header" => cmd_gen_client_header(&root, &remaining_args),
        "check-client-header" | "check-header" => cmd_check_client_header(&root, &remaining_args),
        "list-c-demos" => cmd_list_c_demos(&root),
        "build-c-demo" => cmd_build_c_demo(&root, &remaining_args),
        "run-c-demo" => cmd_run_c_demo(&root, &remaining_args),
        "check-server-gen" => cmd_check_server_gen(&root),
        "gen-server-app" => cmd_gen_server_app(&root, &remaining_args),
        "check-feature-matrix" => cmd_check_feature_matrix(&root),
        "check-feature-subsets" => check_feature_subsets::parse_args(&remaining_args)
            .and_then(|opts| check_feature_subsets::cmd_check_feature_subsets(&root, &opts)),
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
