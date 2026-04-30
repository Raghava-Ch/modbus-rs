//! Feature-subset validation matrix.
//!
//! Runs `cargo check`, `cargo clippy -D warnings`, `cargo build`, and
//! `cargo test` over every meaningful per-feature slice defined in the
//! workspace's feature-flag command reference.
//!
//! Invoked via: `cargo run -p xtask -- check-feature-subsets [--fast]`
//!
//! `--fast` skips `cargo build`, `cargo test`, and example checks —
//! useful for a quick local pre-push lint sweep.

use std::path::Path;
use std::process::Command;

// ─── Step description ─────────────────────────────────────────────────────────

struct Step {
    /// Short human-readable label printed before the command runs.
    label: &'static str,
    /// cargo subcommand, e.g. "check", "clippy", "build", "test".
    sub: &'static str,
    /// Arguments inserted after the subcommand (before `--`).
    args: &'static [&'static str],
    /// Arguments appended after `--` (used for clippy `-D warnings`).
    trailing: &'static [&'static str],
    /// Only run when `--fast` is NOT passed.
    slow: bool,
}

impl Step {
    const fn check(label: &'static str, args: &'static [&'static str]) -> Self {
        Self { label, sub: "check", args, trailing: &[], slow: false }
    }

    const fn clippy(label: &'static str, args: &'static [&'static str]) -> Self {
        Self { label, sub: "clippy", args, trailing: &["-D", "warnings"], slow: false }
    }

    const fn build(label: &'static str, args: &'static [&'static str]) -> Self {
        Self { label, sub: "build", args, trailing: &[], slow: true }
    }

    const fn test(label: &'static str, args: &'static [&'static str]) -> Self {
        Self { label, sub: "test", args, trailing: &[], slow: true }
    }
}

// ─── Full matrix ─────────────────────────────────────────────────────────────

static STEPS: &[Step] = &[
    // ── mbus-core ────────────────────────────────────────────────────────────
    Step::check("core / no-features",
        &["-p", "mbus-core", "--no-default-features"]),
    Step::check("core / coils",
        &["-p", "mbus-core", "--no-default-features", "--features", "coils"]),
    Step::check("core / registers",
        &["-p", "mbus-core", "--no-default-features", "--features", "registers"]),
    Step::check("core / diagnostics",
        &["-p", "mbus-core", "--no-default-features", "--features", "diagnostics"]),
    Step::check("core / coils+registers",
        &["-p", "mbus-core", "--no-default-features", "--features", "coils,registers"]),
    Step::build("core / coils+registers build",
        &["-p", "mbus-core", "--no-default-features", "--features", "coils,registers"]),

    // ── mbus-client ──────────────────────────────────────────────────────────
    Step::check("client / coils",
        &["-p", "mbus-client", "--no-default-features", "--features", "coils"]),
    Step::check("client / registers",
        &["-p", "mbus-client", "--no-default-features", "--features", "registers"]),
    Step::check("client / diagnostics",
        &["-p", "mbus-client", "--no-default-features", "--features", "diagnostics"]),
    Step::check("client / coils+registers+traffic",
        &["-p", "mbus-client", "--no-default-features", "--features", "coils,registers,traffic"]),
    Step::clippy("client / coils (clippy)",
        &["-p", "mbus-client", "--no-default-features", "--features", "coils"]),
    Step::clippy("client / registers (clippy)",
        &["-p", "mbus-client", "--no-default-features", "--features", "registers"]),
    Step::clippy("client / coils+registers (clippy)",
        &["-p", "mbus-client", "--no-default-features", "--features", "coils,registers"]),
    Step::build("client / coils+registers build",
        &["-p", "mbus-client", "--no-default-features", "--features", "coils,registers"]),

    // ── mbus-server ──────────────────────────────────────────────────────────
    Step::check("server / coils",
        &["-p", "mbus-server", "--no-default-features", "--features", "coils"]),
    Step::check("server / holding-registers",
        &["-p", "mbus-server", "--no-default-features", "--features", "holding-registers"]),
    Step::check("server / input-registers",
        &["-p", "mbus-server", "--no-default-features", "--features", "input-registers"]),
    Step::check("server / discrete-inputs",
        &["-p", "mbus-server", "--no-default-features", "--features", "discrete-inputs"]),
    Step::check("server / fifo",
        &["-p", "mbus-server", "--no-default-features", "--features", "fifo"]),
    Step::check("server / file-record",
        &["-p", "mbus-server", "--no-default-features", "--features", "file-record"]),
    Step::check("server / diagnostics",
        &["-p", "mbus-server", "--no-default-features", "--features", "diagnostics"]),
    Step::check("server / coils+holding-registers",
        &["-p", "mbus-server", "--no-default-features", "--features", "coils,holding-registers"]),
    Step::clippy("server / coils+holding-registers (clippy)",
        &["-p", "mbus-server", "--no-default-features", "--features", "coils,holding-registers"]),
    Step::clippy("server / discrete-inputs (clippy)",
        &["-p", "mbus-server", "--no-default-features", "--features", "discrete-inputs"]),
    Step::clippy("server / fifo (clippy)",
        &["-p", "mbus-server", "--no-default-features", "--features", "fifo"]),
    Step::clippy("server / file-record (clippy)",
        &["-p", "mbus-server", "--no-default-features", "--features", "file-record"]),
    Step::clippy("server / diagnostics (clippy)",
        &["-p", "mbus-server", "--no-default-features", "--features", "diagnostics"]),
    Step::build("server / diagnostics build",
        &["-p", "mbus-server", "--no-default-features", "--features", "diagnostics"]),
    // examples
    Step::check("server / example broadcast",
        &["-p", "mbus-server", "--example", "broadcast",
          "--no-default-features", "--features", "coils,holding-registers"]),
    Step::check("server / example diagnostics",
        &["-p", "mbus-server", "--example", "diagnostics",
          "--no-default-features", "--features", "diagnostics"]),
    Step::check("server / example device_id",
        &["-p", "mbus-server", "--example", "device_id",
          "--no-default-features", "--features", "diagnostics"]),
    Step::check("server / example discrete_inputs",
        &["-p", "mbus-server", "--example", "discrete_inputs",
          "--no-default-features", "--features", "discrete-inputs"]),
    Step::check("server / example discrete_inputs_read",
        &["-p", "mbus-server", "--example", "discrete_inputs_read",
          "--no-default-features", "--features", "discrete-inputs"]),
    Step::check("server / example fifo",
        &["-p", "mbus-server", "--example", "fifo",
          "--no-default-features", "--features", "fifo"]),
    Step::check("server / example file_record",
        &["-p", "mbus-server", "--example", "file_record",
          "--no-default-features", "--features", "file-record"]),
    Step::check("server / example holding_registers_read",
        &["-p", "mbus-server", "--example", "holding_registers_read",
          "--no-default-features", "--features", "holding-registers"]),
    Step::check("server / example read_write_registers",
        &["-p", "mbus-server", "--example", "read_write_registers",
          "--no-default-features", "--features", "holding-registers"]),
    Step::check("server / example write_hooks",
        &["-p", "mbus-server", "--example", "write_hooks",
          "--no-default-features", "--features", "coils,holding-registers"]),
    // integration tests
    Step::test("server / test fc01 (coils)",
        &["-p", "mbus-server", "--test", "fc01_fc05_fc15_integration",
          "--no-default-features", "--features", "coils"]),
    Step::test("server / test fc03 (holding-registers)",
        &["-p", "mbus-server", "--test", "fc03_validation_guards",
          "--no-default-features", "--features", "holding-registers"]),
    Step::test("server / test fc04/fc06/fc16 (input+holding-registers)",
        &["-p", "mbus-server", "--test", "fc04_fc06_fc16_integration",
          "--no-default-features", "--features", "input-registers,holding-registers"]),
    Step::test("server / test fc17 (holding-registers)",
        &["-p", "mbus-server", "--test", "fc17_integration",
          "--no-default-features", "--features", "holding-registers"]),
    Step::test("server / test fc18 (fifo)",
        &["-p", "mbus-server", "--test", "fc18_integration",
          "--no-default-features", "--features", "fifo"]),
    Step::test("server / test fc14/fc15 (file-record)",
        &["-p", "mbus-server", "--test", "fc14_fc15_integration",
          "--no-default-features", "--features", "file-record"]),
    Step::test("server / test fc07 (diagnostics)",
        &["-p", "mbus-server", "--test", "fc07_integration",
          "--no-default-features", "--features", "diagnostics"]),
    Step::test("server / test fc0b/fc0c/fc11 (diagnostics)",
        &["-p", "mbus-server", "--test", "fc0b_fc0c_fc11_integration",
          "--no-default-features", "--features", "diagnostics"]),
    Step::test("server / test fc2b (diagnostics)",
        &["-p", "mbus-server", "--test", "fc2b_integration",
          "--no-default-features", "--features", "diagnostics"]),

    // ── mbus-async ───────────────────────────────────────────────────────────
    Step::check("async / network-tcp+coils",
        &["-p", "mbus-async", "--no-default-features", "--features", "network-tcp,coils"]),
    Step::check("async / network-tcp+registers",
        &["-p", "mbus-async", "--no-default-features", "--features", "network-tcp,registers"]),
    Step::check("async / network-tcp+coils+registers",
        &["-p", "mbus-async", "--no-default-features", "--features", "network-tcp,coils,registers"]),
    Step::check("async / network-tcp+coils+traffic",
        &["-p", "mbus-async", "--no-default-features", "--features", "network-tcp,coils,traffic"]),
    Step::check("async / network-tcp+registers+traffic",
        &["-p", "mbus-async", "--no-default-features", "--features", "network-tcp,registers,traffic"]),
    Step::clippy("async / network-tcp+coils (clippy)",
        &["-p", "mbus-async", "--no-default-features", "--features", "network-tcp,coils"]),
    Step::clippy("async / network-tcp+registers (clippy)",
        &["-p", "mbus-async", "--no-default-features", "--features", "network-tcp,registers"]),
    Step::clippy("async / network-tcp+coils+registers (clippy)",
        &["-p", "mbus-async", "--no-default-features", "--features", "network-tcp,coils,registers"]),
    Step::build("async / network-tcp+coils+registers build",
        &["-p", "mbus-async", "--no-default-features", "--features", "network-tcp,coils,registers"]),

    // ── mbus-gateway ─────────────────────────────────────────────────────────
    Step::check("gateway / no-features",
        &["-p", "mbus-gateway", "--no-default-features"]),
    Step::check("gateway / network",
        &["-p", "mbus-gateway", "--no-default-features", "--features", "network"]),
    Step::check("gateway / serial-rtu",
        &["-p", "mbus-gateway", "--no-default-features", "--features", "serial-rtu"]),
    Step::check("gateway / network+serial-rtu",
        &["-p", "mbus-gateway", "--no-default-features", "--features", "network,serial-rtu"]),
    Step::check("gateway / async",
        &["-p", "mbus-gateway", "--no-default-features", "--features", "async"]),
    Step::check("gateway / ws-server",
        &["-p", "mbus-gateway", "--no-default-features", "--features", "ws-server"]),
    Step::clippy("gateway / network+serial-rtu (clippy)",
        &["-p", "mbus-gateway", "--no-default-features", "--features", "network,serial-rtu"]),
    Step::clippy("gateway / async (clippy)",
        &["-p", "mbus-gateway", "--no-default-features", "--features", "async"]),

    // ── mbus-ffi ─────────────────────────────────────────────────────────────
    Step::check("ffi / c+coils+registers",
        &["-p", "mbus-ffi", "--no-default-features", "--features", "c,coils,registers"]),
    Step::check("ffi / c+c-server+server-traffic+full",
        &["-p", "mbus-ffi", "--no-default-features", "--features", "c,c-server,server-traffic,full"]),
    Step::check("ffi / c+c-gateway+full",
        &["-p", "mbus-ffi", "--no-default-features", "--features", "c,c-gateway,full"]),
];

// ─── Runner ───────────────────────────────────────────────────────────────────

pub struct Opts {
    pub fast: bool,
}

pub fn parse_args(args: &[String]) -> Result<Opts, String> {
    let mut fast = false;
    for arg in args {
        match arg.as_str() {
            "--fast" => fast = true,
            other => return Err(format!("unknown flag for check-feature-subsets: {other}")),
        }
    }
    Ok(Opts { fast })
}

pub fn cmd_check_feature_subsets(root: &Path, opts: &Opts) -> Result<(), String> {
    let total = STEPS.iter().filter(|s| !s.slow || !opts.fast).count();
    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut failures: Vec<String> = Vec::new();

    let mode = if opts.fast { " [fast mode — build/test skipped]" } else { "" };
    println!("Running feature-subset matrix ({total} steps){mode}");
    println!("{}", "─".repeat(60));

    for step in STEPS {
        if step.slow && opts.fast {
            continue;
        }
        let label = format!("cargo {} {}", step.sub, step.label);
        print!("  {label} … ");

        let mut cmd = Command::new("cargo");
        cmd.arg(step.sub);
        cmd.args(step.args);
        cmd.current_dir(root);
        if !step.trailing.is_empty() {
            cmd.arg("--");
            cmd.args(step.trailing);
        }

        let output = cmd
            .output()
            .map_err(|e| format!("failed to spawn cargo: {e}"))?;

        if output.status.success() {
            println!("OK");
            passed += 1;
        } else {
            println!("FAIL");
            failed += 1;
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Collect first error line for the summary.
            let first_error = stderr
                .lines()
                .find(|l| l.starts_with("error"))
                .unwrap_or("(no error line found)")
                .to_string();
            failures.push(format!("{label}\n    {first_error}"));
        }
    }

    println!("{}", "─".repeat(60));
    println!("Result: {passed} passed, {failed} failed");

    if !failures.is_empty() {
        println!("\nFailed steps:");
        for f in &failures {
            println!("  ✗ {f}");
        }
        return Err(format!("{failed} feature-subset step(s) failed"));
    }

    Ok(())
}
