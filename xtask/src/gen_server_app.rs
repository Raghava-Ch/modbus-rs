//! `gen-server-app` xtask sub‑command.
//!
//! Two‑mode operation:
//!
//! **Codegen mode** (default when only `--config` is given, optionally
//! `--emit-c-header` / `--out-dir`):
//!   - Parse a YAML server‑app config
//!   - Generate a C header (`mbus_server_app.h`) and/or Rust dispatcher
//!   - Used during development and by CI (`check-server-gen`)
//!
//! **Full mode** (when `--target` and `--out-dir` are supplied):
//!   - Everything from codegen mode **plus**
//!   - Parse the YAML to determine which Modbus memory‑map sections are in use
//!   - Build `mbus-ffi` with **only** the features required by the YAML config
//!     (instead of the catch‑all `c-server,full`)
//!   - Cross‑compile for the given target triple
//!   - Bundle the result into `<out-dir>/include/` and `<out-dir>/lib/` for
//!     consumption by an external C or C++ project

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

// Types are provided by mbus-codegen; re-exported here so callers can still use them.
pub use mbus_codegen::{MapEntry, ServerAppConfig};

#[derive(Debug, Clone)]
pub struct GenServerAppOptions {
    pub config_path: PathBuf,
    /// Output directory for `generated_server.rs`. Optional — when absent, no Rust
    /// file is written (build.rs handles Rust generation via MBUS_SERVER_APP_CONFIG).
    pub out_dir: Option<PathBuf>,
    pub emit_c_header: Option<PathBuf>,
    pub check: bool,
    pub dry_run: bool,
    // ── Full-mode fields ─────────────────────────────────────────────
    /// Target triple for cross‑compilation (e.g. `thumbv7em-none-eabi`).
    /// When `None`, only codegen mode runs.
    pub target: Option<String>,
    /// Output directory root — `include/` and `lib/` subdirectories are created here.
    /// Required when `target` is `Some`.
    pub output_root: Option<PathBuf>,
    /// Build profile (`release` or `debug`).  Defaults to `release`.
    pub profile: Option<String>,
    /// Whether to use Nightly Rust and build-std to aggressively optimize binary size.
    pub optimize_size: bool,
    // ── Transport feature gates ──────────────────────────────────────
    /// Enable TCP transport support (feature = "network-tcp").
    pub network_tcp: bool,
    /// Enable RTU serial transport support (feature = "serial-rtu").
    pub serial_rtu: bool,
    /// Enable ASCII serial transport support (feature = "serial-ascii").
    pub serial_ascii: bool,
}

pub fn parse_args(root: &Path, args: &[String]) -> Result<GenServerAppOptions, String> {
    let mut config_path: Option<PathBuf> = None;
    let mut out_dir: Option<PathBuf> = None;
    let mut emit_c_header: Option<PathBuf> = None;
    let mut check = false;
    let mut dry_run = false;
    // Full-mode flags
    let mut target: Option<String> = None;
    let mut output_root: Option<PathBuf> = None;
    let mut profile: Option<String> = None;
    let mut optimize_size = false;
    // Transport feature flags
    let mut network_tcp = false;
    let mut serial_rtu = false;
    let mut serial_ascii = false;

    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--config" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    return Err("missing value for --config".to_string());
                };
                config_path = Some(root.join(v));
            }
            "--out-dir" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    return Err("missing value for --out-dir".to_string());
                };
                out_dir = Some(root.join(v));
            }
            "--emit-c-header" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    return Err("missing value for --emit-c-header".to_string());
                };
                emit_c_header = Some(root.join(v));
            }
            "--target" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    return Err("missing value for --target".to_string());
                };
                target = Some(v.clone());
            }
            "--profile" => {
                i += 1;
                let Some(v) = args.get(i) else {
                    return Err("missing value for --profile".to_string());
                };
                if v != "release" && v != "debug" {
                    return Err(format!("--profile must be 'release' or 'debug', got '{v}'"));
                }
                profile = Some(v.clone());
            }
            "--network-tcp" => network_tcp = true,
            "--serial-rtu" => serial_rtu = true,
            "--serial-ascii" => serial_ascii = true,
            "--optimize-size" => optimize_size = true,
            "--check" => check = true,
            "--dry-run" => dry_run = true,
            // Positional: the first non-flag argument before --target is the output root
            other if !other.starts_with('-') => {
                if output_root.is_some() {
                    return Err(format!("unexpected positional argument: {other}"));
                }
                output_root = Some(root.join(other));
            }
            other => return Err(format!("unknown argument for gen-server-app: {other}")),
        }
        i += 1;
    }

    let config_path = config_path.ok_or_else(|| "--config is required".to_string())?;

    Ok(GenServerAppOptions {
        config_path,
        out_dir,
        emit_c_header,
        check,
        dry_run,
        target,
        output_root,
        profile,
        optimize_size,
        network_tcp,
        serial_rtu,
        serial_ascii,
    })
}

/// Run the `gen-server-app` command.
pub fn run(opts: &GenServerAppOptions) -> Result<(), String> {
    // ── 1. Parse & validate YAML ─────────────────────────────────────
    let config_text = fs::read_to_string(&opts.config_path)
        .map_err(|e| format!("failed to read {}: {e}", opts.config_path.display()))?;
    let config = mbus_codegen::parse_yaml(&config_text)
        .map_err(|e| format!("invalid yaml in {}: {e}", opts.config_path.display()))?;
    mbus_codegen::validate_config(&config)?;

    // ── 2. Codegen: Rust dispatcher ──────────────────────────────────
    let rust_text_and_out = opts.out_dir.as_ref().map(|out_dir| {
        (
            mbus_codegen::render_rust_dispatcher(&config),
            out_dir.join("generated_server.rs"),
        )
    });

    // ── 3. Codegen: C header ─────────────────────────────────────────
    let header_text_and_path = opts
        .emit_c_header
        .as_ref()
        .map(|hp| (mbus_codegen::render_c_header(&config), hp.clone()));

    // ── 4. Check / dry-run ───────────────────────────────────────────
    if opts.check {
        if let Some((rust_text, rust_out)) = &rust_text_and_out {
            check_exact(rust_out, rust_text)?;
        }
        if let Some((header_text, header_path)) = &header_text_and_path {
            check_exact(header_path, header_text)?;
        }
        println!("OK: generated server artifacts are up to date");
        return Ok(());
    }

    if opts.dry_run {
        if let Some((_, rust_out)) = &rust_text_and_out {
            println!("dry-run: would write {}", rust_out.display());
        }
        if let Some((_, header_path)) = &header_text_and_path {
            println!("dry-run: would write {}", header_path.display());
        }
        if opts.target.is_some() {
            let features = compute_features(
                &config,
                opts.network_tcp,
                opts.serial_rtu,
                opts.serial_ascii,
            );
            println!(
                "dry-run: would build with --features \"{}\"",
                features.join(",")
            );
        }
        return Ok(());
    }

    // ── 5. Write generated artifacts ─────────────────────────────────
    if let Some((rust_text, rust_out)) = &rust_text_and_out {
        let out_dir = rust_out
            .parent()
            .expect("generated_server.rs has no parent dir");
        fs::create_dir_all(out_dir)
            .map_err(|e| format!("failed to create {}: {e}", out_dir.display()))?;
        write_if_changed(rust_out, rust_text)?;
    }

    if let Some((header_text, header_path)) = &header_text_and_path {
        if let Some(parent) = header_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
        }
        write_if_changed(header_path, header_text)?;
    }

    println!(
        "Generated server app artifacts from {}",
        opts.config_path.display()
    );

    // ── 6. Full mode: cross-compile mbus-ffi ─────────────────────────
    // If no positional output root was given, fall back to --out-dir so the user
    // doesn't have to repeat the same path twice.  If neither is available, skip.
    let output_root = opts.output_root.as_ref().or(opts.out_dir.as_ref());
    if let (Some(target), Some(output_root)) = (&opts.target, output_root) {
        let profile = opts.profile.as_deref().unwrap_or("release");
        let features = compute_features(
            &config,
            opts.network_tcp,
            opts.serial_rtu,
            opts.serial_ascii,
        );

        let features_str = features.join(",");
        let nightly_msg = if opts.optimize_size {
            " with build-std size optimizations"
        } else {
            ""
        };
        println!(
            "Building mbus-ffi for target '{target}' with features: {features_str} (profile={profile}){nightly_msg}"
        );

        // 6a. Execute cargo build
        let mut cmd = Command::new("cargo");

        if opts.optimize_size {
            cmd.arg("+nightly");
        }

        cmd.args(["build", "-p", "mbus-ffi"])
            .arg("--target")
            .arg(target)
            .arg("--features")
            .arg(&features_str);

        if profile == "release" {
            cmd.arg("--release");
        }

        if opts.optimize_size {
            cmd.arg("-Z").arg("build-std=core,alloc");
            let existing_flags = std::env::var("RUSTFLAGS").unwrap_or_default();
            cmd.env(
                "RUSTFLAGS",
                format!("{existing_flags} -Zunstable-options -Cpanic=immediate-abort"),
            );
        }

        cmd.current_dir(
            // repo root is the CWD from which xtask is invoked
            Path::new("."),
        );

        // Pass MBUS_SERVER_APP_CONFIG so mbus-ffi/build.rs generates the dispatcher
        cmd.env(
            "MBUS_SERVER_APP_CONFIG",
            opts.config_path.to_string_lossy().as_ref(),
        );

        let status = cmd
            .status()
            .map_err(|e| format!("failed to launch cargo build: {e}"))?;
        if !status.success() {
            return Err("cargo build failed (see above)".to_string());
        }

        // 6b. Locate the built artifacts
        let target_dir = Path::new("target").join(target);
        let base_dir = if profile == "release" {
            target_dir.join("release")
        } else {
            target_dir.join("debug")
        };

        // 6c. Create output directories
        let include_dir = output_root.join("include");
        let lib_dir = output_root.join("lib");
        fs::create_dir_all(&include_dir)
            .map_err(|e| format!("failed to create {}: {e}", include_dir.display()))?;
        fs::create_dir_all(&lib_dir)
            .map_err(|e| format!("failed to create {}: {e}", lib_dir.display()))?;

        // Also create include/lib dirs under --out-dir if it differs from output_root,
        // so the library is always available at the --out-dir location.
        let alt_include_dir = opts
            .out_dir
            .as_ref()
            .filter(|d| *d != output_root)
            .map(|d| d.join("include"));
        let alt_lib_dir = opts
            .out_dir
            .as_ref()
            .filter(|d| *d != output_root)
            .map(|d| d.join("lib"));
        if let Some(ref dir) = alt_include_dir {
            fs::create_dir_all(dir)
                .map_err(|e| format!("failed to create {}: {e}", dir.display()))?;
        }
        if let Some(ref dir) = alt_lib_dir {
            fs::create_dir_all(dir)
                .map_err(|e| format!("failed to create {}: {e}", dir.display()))?;
        }

        // 6d. Copy C header into include/
        if let Some((_, header_path)) = &header_text_and_path {
            // If the --emit-c-header path is inside the workspace, the file was already
            // written above.  We still copy it into <output_root>/include/.
            let dest = include_dir.join("mbus_server_app.h");
            fs::copy(header_path, &dest)
                .map_err(|e| format!("failed to copy header to {}: {e}", dest.display()))?;
            println!("  copied header -> {}", dest.display());
            // Also copy to alt include dir if present
            if let Some(ref alt) = alt_include_dir {
                let alt_dest = alt.join("mbus_server_app.h");
                let _ = fs::copy(header_path, &alt_dest);
            }
        } else {
            // Even without --emit-c-header, write the header into the output include/
            let header_text = mbus_codegen::render_c_header(&config);
            let dest = include_dir.join("mbus_server_app.h");
            fs::write(&dest, &header_text)
                .map_err(|e| format!("failed to write {}: {e}", dest.display()))?;
            println!("  wrote header -> {}", dest.display());
            // Also write to alt include dir if present
            if let Some(ref alt) = alt_include_dir {
                let alt_dest = alt.join("mbus_server_app.h");
                let _ = fs::write(&alt_dest, &header_text);
            }
        }

        // 6e. Copy modbus_rs_server.h into include/
        // This header is generated by cbindgen at build time; it lives in
        // target/mbus-ffi/include/modbus_rs_server.h.  We copy it if present.
        let server_header_src = Path::new("target/mbus-ffi/include/modbus_rs_server.h");
        if server_header_src.exists() {
            let dest = include_dir.join("modbus_rs_server.h");
            fs::copy(server_header_src, &dest)
                .map_err(|e| format!("failed to copy modbus_rs_server.h: {e}",))?;
            println!("  copied modbus_rs_server.h -> {}", dest.display());
            // Also copy to alt include dir if present
            if let Some(ref alt) = alt_include_dir {
                let alt_dest = alt.join("modbus_rs_server.h");
                let _ = fs::copy(server_header_src, &alt_dest);
            }
        }

        // 6f. Copy libmbus_ffi.a into lib/
        let lib_src = base_dir.join("libmbus_ffi.a");
        if lib_src.exists() {
            let dest = lib_dir.join("libmbus_ffi.a");
            fs::copy(&lib_src, &dest)
                .map_err(|e| format!("failed to copy {}: {e}", lib_src.display()))?;
            println!("  copied library -> {}", dest.display());
            // Also copy to alt lib dir if present
            if let Some(ref alt) = alt_lib_dir {
                let alt_dest = alt.join("libmbus_ffi.a");
                let _ = fs::copy(&lib_src, &alt_dest);
            }
        } else {
            // Try the base filename for cdylib (some platforms produce .so/.dylib)
            eprintln!(
                "warning: {} not found (static library may not have been produced for target '{target}')",
                lib_src.display()
            );
        }

        // 6g. Copy generated_server.rs into output root for reference
        if let Some((rust_text, _)) = &rust_text_and_out {
            let dest = output_root.join("generated_server.rs");
            fs::write(&dest, rust_text)
                .map_err(|e| format!("failed to write {}: {e}", dest.display()))?;
            println!("  wrote generated_server.rs -> {}", dest.display());
        }

        println!(
            "Done. Server app bundle for '{target}' is ready at {}",
            output_root.display()
        );
    }

    Ok(())
}

// ── YAML-aware feature selection ──────────────────────────────────────────

/// Map of YAML memory‑map sections to mbus-server feature name.
const SECTION_TO_FEATURE: &[(&str, &str)] = &[
    ("coils", "coils"),
    ("discrete_inputs", "discrete-inputs"),
    ("holding_registers", "registers"),
    ("input_registers", "registers"),
];

/// Compute the minimal set of cargo features needed to build `mbus-ffi` for
/// the given YAML server app config.
///
/// Always includes `c-server`.  Individual mbus-server features are added
/// only for non‑empty memory‑map sections.
///
/// Transport features (`network-tcp`, `serial-rtu`, `serial-ascii`) are added
/// based on the corresponding CLI flags.
fn compute_features(
    config: &ServerAppConfig,
    network_tcp: bool,
    serial_rtu: bool,
    serial_ascii: bool,
) -> Vec<String> {
    let mut features: Vec<String> = Vec::new();

    // c-server is always needed — it enables dep:mbus-server
    features.push("c-server".to_string());

    // Transport features from CLI flags
    if network_tcp {
        features.push("network-tcp".to_string());
    }
    if serial_rtu {
        features.push("serial-rtu".to_string());
    }
    if serial_ascii {
        features.push("serial-ascii".to_string());
    }

    for (section, feature_name) in SECTION_TO_FEATURE {
        let entries: &[MapEntry] = match *section {
            "coils" => &config.memory_map.coils,
            "discrete_inputs" => &config.memory_map.discrete_inputs,
            "holding_registers" => &config.memory_map.holding_registers,
            "input_registers" => &config.memory_map.input_registers,
            _ => unreachable!(),
        };
        if !entries.is_empty() {
            // Forward the feature to mbus-server via the crate's feature forwarding.
            features.push(feature_name.to_string());
        }
    }

    features.sort();
    features.dedup();
    features
}

// ── Helpers ───────────────────────────────────────────────────────────────

fn check_exact(path: &Path, expected: &str) -> Result<(), String> {
    let actual =
        fs::read_to_string(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "generated artifact out of date: {}. Run gen-server-app without --check.",
            path.display()
        ))
    }
}

fn write_if_changed(path: &Path, contents: &str) -> Result<(), String> {
    let unchanged = fs::read_to_string(path)
        .map(|existing| existing == contents)
        .unwrap_or(false);
    if unchanged {
        println!("unchanged: {}", path.display());
        return Ok(());
    }

    fs::write(path, contents).map_err(|e| format!("failed to write {}: {e}", path.display()))?;
    println!("wrote: {}", path.display());
    Ok(())
}
