//! Workspace maintenance commands for header generation/verification and C smoke build checks.

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

fn headers_paths(root: &Path) -> (PathBuf, PathBuf) {
    let base = root.join("mbus-ffi/include/mbus_ffi.h");
    let gated = root.join("mbus-ffi/include/mbus_ffi_feature_gated.h");
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

    let out = out.replacen("#ifndef MBUS_FFI_H", "#ifndef MBUS_FFI_FEATURE_GATED_H", 1);
    out.replacen("#define MBUS_FFI_H", "#define MBUS_FFI_FEATURE_GATED_H", 1)
}

fn cmd_gen_feature_header(root: &Path) -> Result<(), String> {
    let (base, gated) = headers_paths(root);
    let base_header =
        fs::read_to_string(&base).map_err(|e| format!("failed to read {}: {e}", base.display()))?;
    let generated = generate_feature_gated_header(&base_header);
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
    let current = fs::read_to_string(&gated)
        .map_err(|e| format!("failed to read {}: {e}", gated.display()))?;

    if current == expected {
        println!("OK: mbus_ffi_feature_gated.h is up to date.");
        Ok(())
    } else {
        Err(
            "mbus_ffi_feature_gated.h is out of date. Run: cargo run -p xtask -- gen-feature-header"
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

fn cmd_build_c_smoke(root: &Path) -> Result<(), String> {
    run_step(
        "cargo",
        &["build", "-p", "mbus-ffi", "--features", "c,full"],
        root,
    )?;

    let smoke_dir = root.join("mbus-ffi/examples/c_smoke_cmake");
    let build_dir = smoke_dir.join("build");

    run_step("cmake", &["-S", ".", "-B", "build"], &smoke_dir)?;
    run_step("cmake", &["--build", "build"], &smoke_dir)?;
    run_step(
        "ctest",
        &["--test-dir", "build", "--output-on-failure"],
        &smoke_dir,
    )?;

    println!(
        "C smoke binary built and tested at {}",
        build_dir.join("c_smoke_test").display()
    );
    Ok(())
}

fn print_help() {
    println!("xtask commands:");
    println!("  cargo run -p xtask -- gen-header");
    println!("  cargo run -p xtask -- check-header");
    println!("  cargo run -p xtask -- gen-feature-header");
    println!("  cargo run -p xtask -- check-feature-header");
    println!("  cargo run -p xtask -- build-c-smoke");
}

fn main() -> ExitCode {
    let root = repo_root();
    let mut args = env::args();
    let _bin = args.next();
    let Some(cmd) = args.next() else {
        print_help();
        return ExitCode::from(2);
    };

    let result = match cmd.as_str() {
        "gen-header" => cmd_gen_header(&root),
        "check-header" => cmd_check_header(&root),
        "gen-feature-header" => cmd_gen_feature_header(&root),
        "check-feature-header" => cmd_check_feature_header(&root),
        "build-c-smoke" => cmd_build_c_smoke(&root),
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
