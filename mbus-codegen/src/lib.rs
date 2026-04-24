//! Code-generation utilities for the mbus-ffi server-app layer.
//!
//! Used by both `mbus-ffi/build.rs` (as a build-dependency) and `xtask`
//! (as a regular dependency) so the logic lives in exactly one place.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

// ── Config types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerAppConfig {
    pub schema_version: u32,
    pub device: DeviceConfig,
    #[serde(default)]
    pub hooks: HooksConfig,
    pub memory_map: MemoryMap,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DeviceConfig {
    pub name: String,
    pub unit_id: u8,
    pub response_timeout_ms: Option<u32>,
}

/// Optional global write-notification hook fallbacks.
/// Per-entry `on_write` takes priority over these.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct HooksConfig {
    /// Global coil write-notification fallback (used when no per-entry on_write).
    pub on_write_coil: Option<String>,
    /// Global holding-register write-notification fallback.
    pub on_write_holding: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct MemoryMap {
    #[serde(default)]
    pub coils: Vec<MapEntry>,
    #[serde(default)]
    pub discrete_inputs: Vec<MapEntry>,
    #[serde(default)]
    pub holding_registers: Vec<MapEntry>,
    #[serde(default)]
    pub input_registers: Vec<MapEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MapEntry {
    pub name: String,
    pub address: u16,
    pub access: Access,
    /// Write-notification callback. Called BEFORE the value is stored in Rust.
    /// Return `MBUS_HOOK_OK` to allow the write; anything else rejects it.
    /// If absent, Modbus writes to this address succeed silently.
    #[serde(default)]
    pub on_write: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Access {
    Ro,
    Wo,
    Rw,
}

impl Access {
    pub fn is_readable(self) -> bool {
        matches!(self, Access::Ro | Access::Rw)
    }

    pub fn is_writable(self) -> bool {
        matches!(self, Access::Wo | Access::Rw)
    }
}

// ── Public API ────────────────────────────────────────────────────────────

/// Parse a YAML server-app config.
pub fn parse_yaml(text: &str) -> Result<ServerAppConfig, String> {
    serde_yaml::from_str(text).map_err(|e| format!("invalid YAML: {e}"))
}

/// Validate a parsed config, returning a human-readable error on failure.
pub fn validate_config(config: &ServerAppConfig) -> Result<(), String> {
    if config.schema_version != 1 {
        return Err(format!(
            "unsupported schema_version {} (expected 1)",
            config.schema_version
        ));
    }

    validate_entries_unique("coils", &config.memory_map.coils)?;
    validate_entries_unique("discrete_inputs", &config.memory_map.discrete_inputs)?;
    validate_entries_unique("holding_registers", &config.memory_map.holding_registers)?;
    validate_entries_unique("input_registers", &config.memory_map.input_registers)?;

    for e in &config.memory_map.discrete_inputs {
        if e.access.is_writable() {
            return Err(format!(
                "discrete_input '{}' at address {} cannot be writable (Modbus read-only)",
                e.name, e.address
            ));
        }
    }
    for e in &config.memory_map.input_registers {
        if e.access.is_writable() {
            return Err(format!(
                "input_register '{}' at address {} cannot be writable (Modbus read-only)",
                e.name, e.address
            ));
        }
    }

    Ok(())
}

/// Generate the Rust dispatcher source that implements the server app model.
pub fn render_rust_dispatcher(config: &ServerAppConfig) -> String {
    let coil_read_entries: Vec<&MapEntry> =
        config.memory_map.coils.iter().filter(|e| e.access.is_readable()).collect();
    let coil_write_entries: Vec<&MapEntry> =
        config.memory_map.coils.iter().filter(|e| e.access.is_writable()).collect();
    let discrete_read_entries: Vec<&MapEntry> =
        config.memory_map.discrete_inputs.iter().filter(|e| e.access.is_readable()).collect();
    let holding_read_entries: Vec<&MapEntry> =
        config.memory_map.holding_registers.iter().filter(|e| e.access.is_readable()).collect();
    let holding_write_entries: Vec<&MapEntry> =
        config.memory_map.holding_registers.iter().filter(|e| e.access.is_writable()).collect();
    let input_read_entries: Vec<&MapEntry> =
        config.memory_map.input_registers.iter().filter(|e| e.access.is_readable()).collect();

    let has_coils = !config.memory_map.coils.is_empty();
    let has_discrete = !config.memory_map.discrete_inputs.is_empty();
    let has_holding = !config.memory_map.holding_registers.is_empty();
    let has_input = !config.memory_map.input_registers.is_empty();

    // Collect extern "C" declarations for write-notification hooks only.
    let mut extern_lines: BTreeSet<String> = BTreeSet::new();
    for entry in &coil_write_entries {
        if let Some(sym) = resolve_on_write(entry, &config.hooks.on_write_coil) {
            extern_lines.insert(format!(
                "    fn {sym}(ctx: *mut c_void, address: u16, value: u8) -> MbusHookStatus;"
            ));
        }
    }
    for entry in &holding_write_entries {
        if let Some(sym) = resolve_on_write(entry, &config.hooks.on_write_holding) {
            extern_lines.insert(format!(
                "    fn {sym}(ctx: *mut c_void, address: u16, value: u16) -> MbusHookStatus;"
            ));
        }
    }

    let mut out = String::new();

    // File header
    out.push_str("// @generated by mbus-codegen via build.rs. Do not edit manually.\n");
    out.push_str("// Rust owns all register/coil state. C receives write-notification callbacks.\n");
    out.push_str("// Uses the hand-written server FFI infrastructure (pool, transport, config).\n\n");

    // use imports (only what is needed)
    out.push_str("use core::ffi::c_void;\n");
    out.push_str("use core::ptr;\n");
    out.push_str("use crate::c::server::callbacks::*;\n");
    let mut imports: Vec<&str> = vec!["modbus_app"];
    if has_coils   { imports.push("CoilMap"); imports.push("CoilsModel"); }
    if has_discrete { imports.push("DiscreteInputMap"); imports.push("DiscreteInputsModel"); }
    if has_holding { imports.push("HoldingRegisterMap"); imports.push("HoldingRegistersModel"); }
    if has_input   { imports.push("InputRegisterMap"); imports.push("InputRegistersModel"); }
    imports.sort_unstable();
    imports.dedup();
    out.push_str(&format!("use mbus_server::{{{}}};\n\n", imports.join(", ")));

    // MbusHookStatus enum
    out.push_str("/// Status returned by write-notification hooks and FFI accessors.\n");
    out.push_str("#[repr(C)]\n");
    out.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n");
    out.push_str("pub enum MbusHookStatus {\n");
    out.push_str("    MbusHookOk = 0,\n");
    out.push_str("    MbusHookIllegalDataAddress = 1,\n");
    out.push_str("    MbusHookIllegalDataValue = 2,\n");
    out.push_str("    MbusHookDeviceFailure = 3,\n");
    out.push_str("}\n\n");

    // extern "C" block
    out.push_str("unsafe extern \"C\" {\n");
    for line in &extern_lines {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str("    /// Acquire your RTOS mutex / critical section before Rust state is mutated.\n");
    out.push_str("    /// Single-threaded bare-metal: implement as an empty function.\n");
    out.push_str("    fn mbus_app_lock();\n");
    out.push_str("    /// Release the lock acquired by mbus_app_lock().\n");
    out.push_str("    fn mbus_app_unlock();\n");
    out.push_str("}\n\n");

    // Model structs
    if has_coils {
        out.push_str(&render_coils_model(&config.memory_map.coils));
        out.push('\n');
    }
    if has_discrete {
        out.push_str(&render_discrete_inputs_model(&config.memory_map.discrete_inputs));
        out.push('\n');
    }
    if has_holding {
        out.push_str(&render_holding_registers_model(&config.memory_map.holding_registers));
        out.push('\n');
    }
    if has_input {
        out.push_str(&render_input_registers_model(&config.memory_map.input_registers));
        out.push('\n');
    }

    // AppModel struct with #[modbus_app]
    out.push_str(&render_app_struct(config));
    out.push_str("\n\n");

    // Static state — APP_MODEL is initialised in mbus_server_model_init.
    out.push_str("static mut APP_MODEL: Option<AppModel> = None;\n\n");

    // Write dispatch helpers
    if !coil_write_entries.is_empty() {
        out.push_str(&render_write_dispatch_u8(
            "dispatch_write_coil",
            &coil_write_entries,
            &config.hooks.on_write_coil,
        ));
        out.push('\n');
    }
    if !holding_write_entries.is_empty() {
        out.push_str(&render_write_dispatch_u16(
            "dispatch_write_holding",
            &holding_write_entries,
            &config.hooks.on_write_holding,
        ));
        out.push('\n');
    }

    // Address-based FFI exports
    out.push_str(&render_get_coil_ffi(&coil_read_entries, has_coils));
    out.push('\n');
    out.push_str(&render_set_coil_ffi(&coil_write_entries, has_coils));
    out.push('\n');
    out.push_str(&render_get_discrete_input_ffi(&discrete_read_entries, has_discrete));
    out.push('\n');
    out.push_str(&render_get_holding_ffi(&holding_read_entries, has_holding));
    out.push('\n');
    out.push_str(&render_set_holding_ffi(&holding_write_entries, has_holding));
    out.push('\n');
    out.push_str(&render_get_input_ffi(&input_read_entries, has_input));
    out.push('\n');

    // Hook-to-exception-code helper
    out.push_str(&render_hook_to_exception_code());
    out.push('\n');

    // Handler callback functions matching MbusServerHandlers signatures
    out.push_str(&render_server_handler_callbacks(config));
    out.push('\n');

    // Model init + default handlers convenience
    out.push_str(&render_model_init_and_default_handlers(config));
    out.push('\n');

    // Named field FFI exports
    out.push_str(&render_named_ffi(config));

    out
}

/// Generate the C header declaring the app-layer API.
pub fn render_c_header(config: &ServerAppConfig) -> String {
    let coil_write_entries: Vec<&MapEntry> =
        config.memory_map.coils.iter().filter(|e| e.access.is_writable()).collect();
    let holding_write_entries: Vec<&MapEntry> =
        config.memory_map.holding_registers.iter().filter(|e| e.access.is_writable()).collect();
    let has_coils   = !config.memory_map.coils.is_empty();
    let has_discrete = !config.memory_map.discrete_inputs.is_empty();
    let has_holding = !config.memory_map.holding_registers.is_empty();
    let has_input   = !config.memory_map.input_registers.is_empty();

    let mut write_hook_decls: BTreeSet<String> = BTreeSet::new();
    for entry in &coil_write_entries {
        if let Some(sym) = resolve_on_write(entry, &config.hooks.on_write_coil) {
            write_hook_decls.insert(format!(
                "MbusHookStatus {sym}(void* ctx, uint16_t address, uint8_t value);"
            ));
        }
    }
    for entry in &holding_write_entries {
        if let Some(sym) = resolve_on_write(entry, &config.hooks.on_write_holding) {
            write_hook_decls.insert(format!(
                "MbusHookStatus {sym}(void* ctx, uint16_t address, uint16_t value);"
            ));
        }
    }

    let mut out = String::new();
    out.push_str("/* @generated by mbus-codegen via xtask gen-server-app. Do not edit manually. */\n");
    out.push_str("#ifndef MBUS_SERVER_APP_H\n");
    out.push_str("#define MBUS_SERVER_APP_H\n\n");
    out.push_str("#include <stdbool.h>\n");
    out.push_str("#include <stdint.h>\n");
    out.push_str("#include \"modbus_rs_server.h\"\n\n");
    out.push_str("#ifdef __cplusplus\nextern \"C\" {\n#endif\n\n");

    out.push_str("typedef enum MbusHookStatus {\n");
    out.push_str("    MBUS_HOOK_OK = 0,\n");
    out.push_str("    MBUS_HOOK_ILLEGAL_DATA_ADDRESS = 1,\n");
    out.push_str("    MBUS_HOOK_ILLEGAL_DATA_VALUE = 2,\n");
    out.push_str("    MBUS_HOOK_DEVICE_FAILURE = 3\n");
    out.push_str("} MbusHookStatus;\n\n");

    out.push_str("/*\n");
    out.push_str(" * mbus_app_lock() / mbus_app_unlock()\n");
    out.push_str(" *\n");
    out.push_str(" * Protect concurrent access to the Modbus register state owned by Rust.\n");
    out.push_str(" * Called around every state mutation (Modbus writes and named push APIs).\n");
    out.push_str(" *\n");
    out.push_str(" * Implementation guidance:\n");
    out.push_str(" *   RTOS / multi-threaded : acquire/release a mutex or binary semaphore.\n");
    out.push_str(" *   Single-threaded bare-metal : leave both functions empty.\n");
    out.push_str(" *   Bare-metal with interrupts : enter/exit a critical section (disable/enable IRQ).\n");
    out.push_str(" *\n");
    out.push_str(" * Constraints:\n");
    out.push_str(" *   - NOT called before write-notification hooks (hooks run outside the lock).\n");
    out.push_str(" *     Do NOT call mbus_server_get_* or mbus_server_set_* from inside a hook.\n");
    out.push_str(" *   - Must not be called recursively (not reentrant).\n");
    out.push_str(" */\n");
    out.push_str("void mbus_app_lock(void);\n");
    out.push_str("void mbus_app_unlock(void);\n\n");

    if !write_hook_decls.is_empty() {
        out.push_str("/*\n");
        out.push_str(" * Write-notification hooks\n");
        out.push_str(" *\n");
        out.push_str(" * Called BEFORE the value is stored in Rust state.\n");
        out.push_str(" * Return MBUS_HOOK_OK to allow the write; any other value rejects it\n");
        out.push_str(" * and leaves the Rust state unchanged.\n");
        out.push_str(" * Called WITHOUT the lock — do not call mbus_server_get_* / set_* from here.\n");
        out.push_str(" */\n");
        for decl in &write_hook_decls {
            out.push_str(decl);
            out.push('\n');
        }
        out.push('\n');
    }

    out.push_str("/*\n");
    out.push_str(" * Address-based register/coil access.\n");
    out.push_str(" * Useful for direct reads/writes outside the normal server flow.\n");
    out.push_str(" * Set functions call the write-notification hook before storing.\n");
    out.push_str(" */\n");
    out.push_str("MbusHookStatus mbus_server_get_coil(void* ctx, uint16_t address, uint8_t* out_value);\n");
    out.push_str("MbusHookStatus mbus_server_set_coil(void* ctx, uint16_t address, uint8_t value);\n");
    out.push_str("MbusHookStatus mbus_server_get_discrete_input(void* ctx, uint16_t address, uint8_t* out_value);\n");
    out.push_str("MbusHookStatus mbus_server_get_holding_register(void* ctx, uint16_t address, uint16_t* out_value);\n");
    out.push_str("MbusHookStatus mbus_server_set_holding_register(void* ctx, uint16_t address, uint16_t value);\n");
    out.push_str("MbusHookStatus mbus_server_get_input_register(void* ctx, uint16_t address, uint16_t* out_value);\n\n");

    // Server model init + default handlers
    out.push_str("/*\n");
    out.push_str(" * Server lifecycle — uses the standard mbus-ffi server infrastructure\n");
    out.push_str(" * (MbusTransportCallbacks, MbusServerHandlers, MbusServerConfig).\n");
    out.push_str(" *\n");
    out.push_str(" * 1. mbus_server_model_init()           — init Rust-owned register model\n");
    out.push_str(" * 2. mbus_server_default_handlers(ud)   — get populated MbusServerHandlers struct\n");
    out.push_str(" * 3. mbus_tcp_server_new(&t, &h, &cfg)  — create server via standard pool API\n");
    out.push_str(" * 4. mbus_tcp_server_connect(id)         — open transport\n");
    out.push_str(" * 5. mbus_tcp_server_poll(id)            — drive server state machine\n");
    out.push_str(" * 6. mbus_tcp_server_disconnect(id)      — close transport\n");
    out.push_str(" * 7. mbus_tcp_server_free(id)            — release pool slot\n");
    out.push_str(" */\n");
    out.push_str("void mbus_server_model_init(void);\n");
    out.push_str("struct MbusServerHandlers mbus_server_default_handlers(void *userdata);\n\n");

    // Generated handler callbacks (for users who want to build MbusServerHandlers manually)
    out.push_str("/*\n");
    out.push_str(" * Generated handler callbacks.\n");
    out.push_str(" * These are automatically wired by mbus_server_default_handlers().\n");
    out.push_str(" * Declared here for users who want to build MbusServerHandlers manually.\n");
    out.push_str(" */\n");
    if has_coils {
        out.push_str("enum MbusServerExceptionCode mbus_gen_on_read_coils(\n");
        out.push_str("    struct MbusServerReadCoilsReq *req, void *userdata);\n");
    }
    if !coil_write_entries.is_empty() {
        out.push_str("enum MbusServerExceptionCode mbus_gen_on_write_single_coil(\n");
        out.push_str("    const struct MbusServerWriteSingleCoilReq *req, void *userdata);\n");
        out.push_str("enum MbusServerExceptionCode mbus_gen_on_write_multiple_coils(\n");
        out.push_str("    const struct MbusServerWriteMultipleCoilsReq *req, void *userdata);\n");
    }
    if has_discrete {
        out.push_str("enum MbusServerExceptionCode mbus_gen_on_read_discrete_inputs(\n");
        out.push_str("    struct MbusServerReadDiscreteInputsReq *req, void *userdata);\n");
    }
    if has_holding {
        out.push_str("enum MbusServerExceptionCode mbus_gen_on_read_holding_registers(\n");
        out.push_str("    struct MbusServerReadHoldingRegistersReq *req, void *userdata);\n");
    }
    if !holding_write_entries.is_empty() {
        out.push_str("enum MbusServerExceptionCode mbus_gen_on_write_single_register(\n");
        out.push_str("    const struct MbusServerWriteSingleRegisterReq *req, void *userdata);\n");
        out.push_str("enum MbusServerExceptionCode mbus_gen_on_write_multiple_registers(\n");
        out.push_str("    const struct MbusServerWriteMultipleRegistersReq *req, void *userdata);\n");
    }
    if has_input {
        out.push_str("enum MbusServerExceptionCode mbus_gen_on_read_input_registers(\n");
        out.push_str("    struct MbusServerReadInputRegistersReq *req, void *userdata);\n");
    }
    out.push('\n');

    out.push_str("/*\n");
    out.push_str(" * Named field accessors — push/pull values from your application code.\n");
    out.push_str(" * Bypass write-notification hooks (app-side push, not a Modbus write).\n");
    out.push_str(" * Use to seed initial sensor values or read coil state from application code.\n");
    out.push_str(" */\n");
    for entry in &config.memory_map.coils {
        let name = &entry.name;
        out.push_str(&format!("void mbus_server_set_{name}(uint8_t value);\n"));
        out.push_str(&format!("MbusHookStatus mbus_server_get_{name}(uint8_t* out_value);\n"));
    }
    for entry in &config.memory_map.discrete_inputs {
        let name = &entry.name;
        out.push_str(&format!("void mbus_server_set_{name}(uint8_t value);\n"));
        out.push_str(&format!("MbusHookStatus mbus_server_get_{name}(uint8_t* out_value);\n"));
    }
    for entry in &config.memory_map.holding_registers {
        let name = &entry.name;
        out.push_str(&format!("void mbus_server_set_{name}(uint16_t value);\n"));
        out.push_str(&format!("MbusHookStatus mbus_server_get_{name}(uint16_t* out_value);\n"));
    }
    for entry in &config.memory_map.input_registers {
        let name = &entry.name;
        out.push_str(&format!("void mbus_server_set_{name}(uint16_t value);\n"));
        out.push_str(&format!("MbusHookStatus mbus_server_get_{name}(uint16_t* out_value);\n"));
    }

    out.push('\n');
    out.push_str(&format!(
        "/* Device: {} | Unit ID: {} */\n\n",
        config.device.name, config.device.unit_id
    ));
    out.push_str("#ifdef __cplusplus\n}\n#endif\n\n");
    out.push_str("#endif /* MBUS_SERVER_APP_H */\n");
    out
}

// ── Private rendering helpers ─────────────────────────────────────────────

fn validate_entries_unique(section: &str, entries: &[MapEntry]) -> Result<(), String> {
    let mut seen = std::collections::BTreeSet::new();
    for entry in entries {
        if !seen.insert(entry.address) {
            return Err(format!(
                "duplicate address {} in section {}",
                entry.address, section
            ));
        }
    }
    Ok(())
}

fn resolve_on_write<'a>(entry: &'a MapEntry, global: &'a Option<String>) -> Option<&'a str> {
    entry.on_write.as_deref().or(global.as_deref())
}

fn render_coils_model(entries: &[MapEntry]) -> String {
    let mut out = String::new();
    out.push_str("#[derive(Debug, Default, CoilsModel)]\n");
    out.push_str("pub struct AppCoils {\n");
    for e in entries {
        out.push_str(&format!("    #[coil(addr = {})]\n", e.address));
        out.push_str(&format!("    pub {}: bool,\n", e.name));
    }
    out.push_str("}\n");
    out
}

fn render_discrete_inputs_model(entries: &[MapEntry]) -> String {
    let mut out = String::new();
    out.push_str("#[derive(Debug, Default, DiscreteInputsModel)]\n");
    out.push_str("pub struct AppDiscreteInputs {\n");
    for e in entries {
        out.push_str(&format!("    #[discrete_input(addr = {})]\n", e.address));
        out.push_str(&format!("    pub {}: bool,\n", e.name));
    }
    out.push_str("}\n");
    out
}

fn render_holding_registers_model(entries: &[MapEntry]) -> String {
    let mut out = String::new();
    out.push_str("#[derive(Debug, Default, HoldingRegistersModel)]\n");
    out.push_str("pub struct AppHolding {\n");
    for e in entries {
        out.push_str(&format!("    #[reg(addr = {})]\n", e.address));
        out.push_str(&format!("    pub {}: u16,\n", e.name));
    }
    out.push_str("}\n");
    out
}

fn render_input_registers_model(entries: &[MapEntry]) -> String {
    let mut out = String::new();
    out.push_str("#[derive(Debug, Default, InputRegistersModel)]\n");
    out.push_str("pub struct AppInput {\n");
    for e in entries {
        out.push_str(&format!("    #[reg(addr = {})]\n", e.address));
        out.push_str(&format!("    pub {}: u16,\n", e.name));
    }
    out.push_str("}\n");
    out
}

fn render_app_struct(config: &ServerAppConfig) -> String {
    let has_coils = !config.memory_map.coils.is_empty();
    let has_discrete = !config.memory_map.discrete_inputs.is_empty();
    let has_holding = !config.memory_map.holding_registers.is_empty();
    let has_input = !config.memory_map.input_registers.is_empty();

    let mut macro_args: Vec<String> = vec![];
    if has_coils { macro_args.push("coils(coils)".to_string()); }
    if has_discrete { macro_args.push("discrete_inputs(discrete_inputs)".to_string()); }
    if has_holding { macro_args.push("holding_registers(holding)".to_string()); }
    if has_input { macro_args.push("input_registers(input)".to_string()); }

    let mut out = String::new();
    out.push_str(&format!("/// Generated server app for: {}\n", config.device.name));
    out.push_str("#[derive(Debug, Default)]\n");
    out.push_str(&format!("#[modbus_app({})]\n", macro_args.join(", ")));
    out.push_str("pub struct AppModel {\n");
    if has_coils { out.push_str("    pub coils: AppCoils,\n"); }
    if has_discrete { out.push_str("    pub discrete_inputs: AppDiscreteInputs,\n"); }
    if has_holding { out.push_str("    pub holding: AppHolding,\n"); }
    if has_input { out.push_str("    pub input: AppInput,\n"); }
    out.push('}');
    out
}

fn render_write_dispatch_u8(name: &str, entries: &[&MapEntry], global: &Option<String>) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "#[inline(always)]\nfn {name}(ctx: *mut c_void, address: u16, value: u8) -> MbusHookStatus {{\n"
    ));
    out.push_str("    match address {\n");
    for entry in entries {
        match resolve_on_write(entry, global) {
            Some(sym) => {
                out.push_str(&format!(
                    "        {} => unsafe {{ {sym}(ctx, address, value) }},\n",
                    entry.address
                ));
            }
            None => {
                out.push_str(&format!("        {} => MbusHookStatus::MbusHookOk,\n", entry.address));
            }
        }
    }
    out.push_str("        _ => MbusHookStatus::MbusHookIllegalDataAddress,\n");
    out.push_str("    }\n}\n");
    out
}

fn render_write_dispatch_u16(name: &str, entries: &[&MapEntry], global: &Option<String>) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "#[inline(always)]\nfn {name}(ctx: *mut c_void, address: u16, value: u16) -> MbusHookStatus {{\n"
    ));
    out.push_str("    match address {\n");
    for entry in entries {
        match resolve_on_write(entry, global) {
            Some(sym) => {
                out.push_str(&format!(
                    "        {} => unsafe {{ {sym}(ctx, address, value) }},\n",
                    entry.address
                ));
            }
            None => {
                out.push_str(&format!("        {} => MbusHookStatus::MbusHookOk,\n", entry.address));
            }
        }
    }
    out.push_str("        _ => MbusHookStatus::MbusHookIllegalDataAddress,\n");
    out.push_str("    }\n}\n");
    out
}

fn render_get_coil_ffi(entries: &[&MapEntry], has_coils: bool) -> String {
    let mut out = String::new();
    out.push_str("#[unsafe(no_mangle)]\n");
    out.push_str("pub extern \"C\" fn mbus_server_get_coil(\n");
    out.push_str("    _ctx: *mut c_void,\n");
    out.push_str("    address: u16,\n");
    out.push_str("    out_value: *mut u8,\n");
    out.push_str(") -> MbusHookStatus {\n");
    if !has_coils || entries.is_empty() {
        out.push_str("    let _ = (address, out_value);\n");
        out.push_str("    MbusHookStatus::MbusHookIllegalDataAddress\n");
    } else {
        out.push_str("    if out_value.is_null() { return MbusHookStatus::MbusHookDeviceFailure; }\n");
        out.push_str("    unsafe {\n");
        out.push_str("        mbus_app_lock();\n");
        out.push_str("        let val = (&*ptr::addr_of!(APP_MODEL)).as_ref().and_then(|app| match address {\n");
        for entry in entries {
            out.push_str(&format!(
                "            {} => Some(app.coils.{} as u8),\n",
                entry.address, entry.name
            ));
        }
        out.push_str("            _ => None,\n");
        out.push_str("        });\n");
        out.push_str("        mbus_app_unlock();\n");
        out.push_str("        match val {\n");
        out.push_str("            Some(v) => { *out_value = v; MbusHookStatus::MbusHookOk }\n");
        out.push_str("            None => MbusHookStatus::MbusHookIllegalDataAddress,\n");
        out.push_str("        }\n");
        out.push_str("    }\n");
    }
    out.push_str("}\n");
    out
}

fn render_set_coil_ffi(entries: &[&MapEntry], has_coils: bool) -> String {
    let mut out = String::new();
    out.push_str("#[unsafe(no_mangle)]\n");
    out.push_str("pub extern \"C\" fn mbus_server_set_coil(\n");
    out.push_str("    ctx: *mut c_void,\n");
    out.push_str("    address: u16,\n");
    out.push_str("    value: u8,\n");
    out.push_str(") -> MbusHookStatus {\n");
    if !has_coils || entries.is_empty() {
        out.push_str("    let _ = (ctx, address, value);\n");
        out.push_str("    MbusHookStatus::MbusHookIllegalDataAddress\n");
    } else {
        out.push_str("    let st = dispatch_write_coil(ctx, address, value);\n");
        out.push_str("    if st != MbusHookStatus::MbusHookOk { return st; }\n");
        out.push_str("    unsafe {\n");
        out.push_str("        mbus_app_lock();\n");
        out.push_str("        if let Some(app) = (&mut *ptr::addr_of_mut!(APP_MODEL)).as_mut() {\n");
        out.push_str("            match address {\n");
        for entry in entries {
            out.push_str(&format!(
                "                {} => app.coils.{} = value != 0,\n",
                entry.address, entry.name
            ));
        }
        out.push_str("                _ => {}\n");
        out.push_str("            }\n");
        out.push_str("        }\n");
        out.push_str("        mbus_app_unlock();\n");
        out.push_str("    }\n");
        out.push_str("    MbusHookStatus::MbusHookOk\n");
    }
    out.push_str("}\n");
    out
}

fn render_get_discrete_input_ffi(entries: &[&MapEntry], has_discrete: bool) -> String {
    let mut out = String::new();
    out.push_str("#[unsafe(no_mangle)]\n");
    out.push_str("pub extern \"C\" fn mbus_server_get_discrete_input(\n");
    out.push_str("    _ctx: *mut c_void,\n");
    out.push_str("    address: u16,\n");
    out.push_str("    out_value: *mut u8,\n");
    out.push_str(") -> MbusHookStatus {\n");
    if !has_discrete || entries.is_empty() {
        out.push_str("    let _ = (address, out_value);\n");
        out.push_str("    MbusHookStatus::MbusHookIllegalDataAddress\n");
    } else {
        out.push_str("    if out_value.is_null() { return MbusHookStatus::MbusHookDeviceFailure; }\n");
        out.push_str("    unsafe {\n");
        out.push_str("        mbus_app_lock();\n");
        out.push_str("        let val = (&*ptr::addr_of!(APP_MODEL)).as_ref().and_then(|app| match address {\n");
        for entry in entries {
            out.push_str(&format!(
                "            {} => Some(app.discrete_inputs.{} as u8),\n",
                entry.address, entry.name
            ));
        }
        out.push_str("            _ => None,\n");
        out.push_str("        });\n");
        out.push_str("        mbus_app_unlock();\n");
        out.push_str("        match val {\n");
        out.push_str("            Some(v) => { *out_value = v; MbusHookStatus::MbusHookOk }\n");
        out.push_str("            None => MbusHookStatus::MbusHookIllegalDataAddress,\n");
        out.push_str("        }\n");
        out.push_str("    }\n");
    }
    out.push_str("}\n");
    out
}

fn render_get_holding_ffi(entries: &[&MapEntry], has_holding: bool) -> String {
    let mut out = String::new();
    out.push_str("#[unsafe(no_mangle)]\n");
    out.push_str("pub extern \"C\" fn mbus_server_get_holding_register(\n");
    out.push_str("    _ctx: *mut c_void,\n");
    out.push_str("    address: u16,\n");
    out.push_str("    out_value: *mut u16,\n");
    out.push_str(") -> MbusHookStatus {\n");
    if !has_holding || entries.is_empty() {
        out.push_str("    let _ = (address, out_value);\n");
        out.push_str("    MbusHookStatus::MbusHookIllegalDataAddress\n");
    } else {
        out.push_str("    if out_value.is_null() { return MbusHookStatus::MbusHookDeviceFailure; }\n");
        out.push_str("    unsafe {\n");
        out.push_str("        mbus_app_lock();\n");
        out.push_str("        let val = (&*ptr::addr_of!(APP_MODEL)).as_ref().and_then(|app| match address {\n");
        for entry in entries {
            out.push_str(&format!(
                "            {} => Some(app.holding.{}()),\n",
                entry.address, entry.name
            ));
        }
        out.push_str("            _ => None,\n");
        out.push_str("        });\n");
        out.push_str("        mbus_app_unlock();\n");
        out.push_str("        match val {\n");
        out.push_str("            Some(v) => { *out_value = v; MbusHookStatus::MbusHookOk }\n");
        out.push_str("            None => MbusHookStatus::MbusHookIllegalDataAddress,\n");
        out.push_str("        }\n");
        out.push_str("    }\n");
    }
    out.push_str("}\n");
    out
}

fn render_set_holding_ffi(entries: &[&MapEntry], has_holding: bool) -> String {
    let mut out = String::new();
    out.push_str("#[unsafe(no_mangle)]\n");
    out.push_str("pub extern \"C\" fn mbus_server_set_holding_register(\n");
    out.push_str("    ctx: *mut c_void,\n");
    out.push_str("    address: u16,\n");
    out.push_str("    value: u16,\n");
    out.push_str(") -> MbusHookStatus {\n");
    if !has_holding || entries.is_empty() {
        out.push_str("    let _ = (ctx, address, value);\n");
        out.push_str("    MbusHookStatus::MbusHookIllegalDataAddress\n");
    } else {
        out.push_str("    let st = dispatch_write_holding(ctx, address, value);\n");
        out.push_str("    if st != MbusHookStatus::MbusHookOk { return st; }\n");
        out.push_str("    unsafe {\n");
        out.push_str("        mbus_app_lock();\n");
        out.push_str("        if let Some(app) = (&mut *ptr::addr_of_mut!(APP_MODEL)).as_mut() {\n");
        out.push_str("            match address {\n");
        for entry in entries {
            out.push_str(&format!(
                "                {} => app.holding.set_{}(value),\n",
                entry.address, entry.name
            ));
        }
        out.push_str("                _ => {}\n");
        out.push_str("            }\n");
        out.push_str("        }\n");
        out.push_str("        mbus_app_unlock();\n");
        out.push_str("    }\n");
        out.push_str("    MbusHookStatus::MbusHookOk\n");
    }
    out.push_str("}\n");
    out
}

fn render_get_input_ffi(entries: &[&MapEntry], has_input: bool) -> String {
    let mut out = String::new();
    out.push_str("#[unsafe(no_mangle)]\n");
    out.push_str("pub extern \"C\" fn mbus_server_get_input_register(\n");
    out.push_str("    _ctx: *mut c_void,\n");
    out.push_str("    address: u16,\n");
    out.push_str("    out_value: *mut u16,\n");
    out.push_str(") -> MbusHookStatus {\n");
    if !has_input || entries.is_empty() {
        out.push_str("    let _ = (address, out_value);\n");
        out.push_str("    MbusHookStatus::MbusHookIllegalDataAddress\n");
    } else {
        out.push_str("    if out_value.is_null() { return MbusHookStatus::MbusHookDeviceFailure; }\n");
        out.push_str("    unsafe {\n");
        out.push_str("        mbus_app_lock();\n");
        out.push_str("        let val = (&*ptr::addr_of!(APP_MODEL)).as_ref().and_then(|app| match address {\n");
        for entry in entries {
            out.push_str(&format!(
                "            {} => Some(app.input.{}()),\n",
                entry.address, entry.name
            ));
        }
        out.push_str("            _ => None,\n");
        out.push_str("        });\n");
        out.push_str("        mbus_app_unlock();\n");
        out.push_str("        match val {\n");
        out.push_str("            Some(v) => { *out_value = v; MbusHookStatus::MbusHookOk }\n");
        out.push_str("            None => MbusHookStatus::MbusHookIllegalDataAddress,\n");
        out.push_str("        }\n");
        out.push_str("    }\n");
    }
    out.push_str("}\n");
    out
}

fn render_hook_to_exception_code() -> String {
    let mut out = String::new();
    out.push_str("#[inline(always)]\n");
    out.push_str("fn hook_to_exception(st: MbusHookStatus) -> MbusServerExceptionCode {\n");
    out.push_str("    match st {\n");
    out.push_str("        MbusHookStatus::MbusHookOk => MbusServerExceptionCode::Ok,\n");
    out.push_str("        MbusHookStatus::MbusHookIllegalDataAddress => MbusServerExceptionCode::IllegalDataAddress,\n");
    out.push_str("        MbusHookStatus::MbusHookIllegalDataValue => MbusServerExceptionCode::IllegalDataValue,\n");
    out.push_str("        _ => MbusServerExceptionCode::ServerDeviceFailure,\n");
    out.push_str("    }\n");
    out.push_str("}\n");
    out
}

fn render_server_handler_callbacks(config: &ServerAppConfig) -> String {
    let coil_write_entries: Vec<&MapEntry> =
        config.memory_map.coils.iter().filter(|e| e.access.is_writable()).collect();
    let holding_write_entries: Vec<&MapEntry> =
        config.memory_map.holding_registers.iter().filter(|e| e.access.is_writable()).collect();
    let has_coils   = !config.memory_map.coils.is_empty();
    let has_discrete = !config.memory_map.discrete_inputs.is_empty();
    let has_holding = !config.memory_map.holding_registers.is_empty();
    let has_input   = !config.memory_map.input_registers.is_empty();

    let mut out = String::new();
    out.push_str("// ---------------------------------------------------------------------------\n");
    out.push_str("// Handler callbacks matching MbusServerHandlers signatures.\n");
    out.push_str("// Wire these into a MbusServerHandlers struct or use mbus_server_default_handlers().\n");
    out.push_str("// ---------------------------------------------------------------------------\n\n");

    // FC01 — Read Coils
    if has_coils {
        out.push_str("/// Handler for FC 0x01 — Read Coils.  Reads from model state.\n");
        out.push_str("#[unsafe(no_mangle)]\n");
        out.push_str("pub unsafe extern \"C\" fn mbus_gen_on_read_coils(\n");
        out.push_str("    req: *mut MbusServerReadCoilsReq,\n");
        out.push_str("    _userdata: *mut c_void,\n");
        out.push_str(") -> MbusServerExceptionCode {\n");
        out.push_str("    if req.is_null() { return MbusServerExceptionCode::ServerDeviceFailure; }\n");
        out.push_str("    let req = unsafe { &mut *req };\n");
        out.push_str("    unsafe {\n");
        out.push_str("        mbus_app_lock();\n");
        out.push_str("        let result = (&*ptr::addr_of!(APP_MODEL))\n");
        out.push_str("            .as_ref()\n");
        out.push_str("            .and_then(|app| {\n");
        out.push_str("                let out = core::slice::from_raw_parts_mut(req.out_data, req.out_data_len);\n");
        out.push_str("                app.coils.encode(req.address, req.quantity, out).ok()\n");
        out.push_str("            });\n");
        out.push_str("        mbus_app_unlock();\n");
        out.push_str("        match result {\n");
        out.push_str("            Some(n) => { req.out_byte_count = n; MbusServerExceptionCode::Ok }\n");
        out.push_str("            None => MbusServerExceptionCode::IllegalDataAddress,\n");
        out.push_str("        }\n");
        out.push_str("    }\n");
        out.push_str("}\n\n");
    }

    // FC05 — Write Single Coil
    if !coil_write_entries.is_empty() {
        out.push_str("/// Handler for FC 0x05 — Write Single Coil.\n");
        out.push_str("#[unsafe(no_mangle)]\n");
        out.push_str("pub unsafe extern \"C\" fn mbus_gen_on_write_single_coil(\n");
        out.push_str("    req: *const MbusServerWriteSingleCoilReq,\n");
        out.push_str("    userdata: *mut c_void,\n");
        out.push_str(") -> MbusServerExceptionCode {\n");
        out.push_str("    if req.is_null() { return MbusServerExceptionCode::ServerDeviceFailure; }\n");
        out.push_str("    let req = unsafe { &*req };\n");
        out.push_str("    let st = dispatch_write_coil(userdata, req.address, req.value as u8);\n");
        out.push_str("    if st != MbusHookStatus::MbusHookOk { return hook_to_exception(st); }\n");
        out.push_str("    unsafe {\n");
        out.push_str("        mbus_app_lock();\n");
        out.push_str("        if let Some(app) = (&mut *ptr::addr_of_mut!(APP_MODEL)).as_mut() {\n");
        out.push_str("            let _ = app.coils.write_single(req.address, req.value);\n");
        out.push_str("        }\n");
        out.push_str("        mbus_app_unlock();\n");
        out.push_str("    }\n");
        out.push_str("    MbusServerExceptionCode::Ok\n");
        out.push_str("}\n\n");

        // FC0F — Write Multiple Coils
        out.push_str("/// Handler for FC 0x0F — Write Multiple Coils.\n");
        out.push_str("#[unsafe(no_mangle)]\n");
        out.push_str("pub unsafe extern \"C\" fn mbus_gen_on_write_multiple_coils(\n");
        out.push_str("    req: *const MbusServerWriteMultipleCoilsReq,\n");
        out.push_str("    userdata: *mut c_void,\n");
        out.push_str(") -> MbusServerExceptionCode {\n");
        out.push_str("    if req.is_null() { return MbusServerExceptionCode::ServerDeviceFailure; }\n");
        out.push_str("    let req = unsafe { &*req };\n");
        out.push_str("    let values = unsafe { core::slice::from_raw_parts(req.values, req.values_len) };\n");
        out.push_str("    for i in 0..req.quantity {\n");
        out.push_str("        let byte_idx = i as usize / 8;\n");
        out.push_str("        let bit = if byte_idx < values.len() { (values[byte_idx] >> (i % 8)) & 1 } else { 0 };\n");
        out.push_str("        let st = dispatch_write_coil(userdata, req.starting_address.wrapping_add(i), bit);\n");
        out.push_str("        if st != MbusHookStatus::MbusHookOk { return hook_to_exception(st); }\n");
        out.push_str("    }\n");
        out.push_str("    unsafe {\n");
        out.push_str("        mbus_app_lock();\n");
        out.push_str("        if let Some(app) = (&mut *ptr::addr_of_mut!(APP_MODEL)).as_mut() {\n");
        out.push_str("            let _ = app.coils.write_many_from_packed(\n");
        out.push_str("                req.starting_address, req.quantity, values, 0,\n");
        out.push_str("            );\n");
        out.push_str("        }\n");
        out.push_str("        mbus_app_unlock();\n");
        out.push_str("    }\n");
        out.push_str("    MbusServerExceptionCode::Ok\n");
        out.push_str("}\n\n");
    }

    // FC02 — Read Discrete Inputs
    if has_discrete {
        out.push_str("/// Handler for FC 0x02 — Read Discrete Inputs.\n");
        out.push_str("#[unsafe(no_mangle)]\n");
        out.push_str("pub unsafe extern \"C\" fn mbus_gen_on_read_discrete_inputs(\n");
        out.push_str("    req: *mut MbusServerReadDiscreteInputsReq,\n");
        out.push_str("    _userdata: *mut c_void,\n");
        out.push_str(") -> MbusServerExceptionCode {\n");
        out.push_str("    if req.is_null() { return MbusServerExceptionCode::ServerDeviceFailure; }\n");
        out.push_str("    let req = unsafe { &mut *req };\n");
        out.push_str("    unsafe {\n");
        out.push_str("        mbus_app_lock();\n");
        out.push_str("        let result = (&*ptr::addr_of!(APP_MODEL))\n");
        out.push_str("            .as_ref()\n");
        out.push_str("            .and_then(|app| {\n");
        out.push_str("                let out = core::slice::from_raw_parts_mut(req.out_data, req.out_data_len);\n");
        out.push_str("                app.discrete_inputs.encode(req.address, req.quantity, out).ok()\n");
        out.push_str("            });\n");
        out.push_str("        mbus_app_unlock();\n");
        out.push_str("        match result {\n");
        out.push_str("            Some(n) => { req.out_byte_count = n; MbusServerExceptionCode::Ok }\n");
        out.push_str("            None => MbusServerExceptionCode::IllegalDataAddress,\n");
        out.push_str("        }\n");
        out.push_str("    }\n");
        out.push_str("}\n\n");
    }

    // FC03 — Read Holding Registers
    if has_holding {
        out.push_str("/// Handler for FC 0x03 — Read Holding Registers.\n");
        out.push_str("#[unsafe(no_mangle)]\n");
        out.push_str("pub unsafe extern \"C\" fn mbus_gen_on_read_holding_registers(\n");
        out.push_str("    req: *mut MbusServerReadHoldingRegistersReq,\n");
        out.push_str("    _userdata: *mut c_void,\n");
        out.push_str(") -> MbusServerExceptionCode {\n");
        out.push_str("    if req.is_null() { return MbusServerExceptionCode::ServerDeviceFailure; }\n");
        out.push_str("    let req = unsafe { &mut *req };\n");
        out.push_str("    unsafe {\n");
        out.push_str("        mbus_app_lock();\n");
        out.push_str("        let result = (&*ptr::addr_of!(APP_MODEL))\n");
        out.push_str("            .as_ref()\n");
        out.push_str("            .and_then(|app| {\n");
        out.push_str("                let out = core::slice::from_raw_parts_mut(req.out_data, req.out_data_len);\n");
        out.push_str("                app.holding.encode(req.address, req.quantity, out).ok()\n");
        out.push_str("            });\n");
        out.push_str("        mbus_app_unlock();\n");
        out.push_str("        match result {\n");
        out.push_str("            Some(n) => { req.out_byte_count = n; MbusServerExceptionCode::Ok }\n");
        out.push_str("            None => MbusServerExceptionCode::IllegalDataAddress,\n");
        out.push_str("        }\n");
        out.push_str("    }\n");
        out.push_str("}\n\n");
    }

    // FC06 — Write Single Register
    if !holding_write_entries.is_empty() {
        out.push_str("/// Handler for FC 0x06 — Write Single Register.\n");
        out.push_str("#[unsafe(no_mangle)]\n");
        out.push_str("pub unsafe extern \"C\" fn mbus_gen_on_write_single_register(\n");
        out.push_str("    req: *const MbusServerWriteSingleRegisterReq,\n");
        out.push_str("    userdata: *mut c_void,\n");
        out.push_str(") -> MbusServerExceptionCode {\n");
        out.push_str("    if req.is_null() { return MbusServerExceptionCode::ServerDeviceFailure; }\n");
        out.push_str("    let req = unsafe { &*req };\n");
        out.push_str("    let st = dispatch_write_holding(userdata, req.address, req.value);\n");
        out.push_str("    if st != MbusHookStatus::MbusHookOk { return hook_to_exception(st); }\n");
        out.push_str("    unsafe {\n");
        out.push_str("        mbus_app_lock();\n");
        out.push_str("        if let Some(app) = (&mut *ptr::addr_of_mut!(APP_MODEL)).as_mut() {\n");
        out.push_str("            let _ = app.holding.write_single(req.address, req.value);\n");
        out.push_str("        }\n");
        out.push_str("        mbus_app_unlock();\n");
        out.push_str("    }\n");
        out.push_str("    MbusServerExceptionCode::Ok\n");
        out.push_str("}\n\n");

        // FC10 — Write Multiple Registers
        out.push_str("/// Handler for FC 0x10 — Write Multiple Registers.\n");
        out.push_str("#[unsafe(no_mangle)]\n");
        out.push_str("pub unsafe extern \"C\" fn mbus_gen_on_write_multiple_registers(\n");
        out.push_str("    req: *const MbusServerWriteMultipleRegistersReq,\n");
        out.push_str("    userdata: *mut c_void,\n");
        out.push_str(") -> MbusServerExceptionCode {\n");
        out.push_str("    if req.is_null() { return MbusServerExceptionCode::ServerDeviceFailure; }\n");
        out.push_str("    let req = unsafe { &*req };\n");
        out.push_str("    let values = unsafe { core::slice::from_raw_parts(req.values, req.values_len) };\n");
        out.push_str("    for (i, &v) in values.iter().enumerate() {\n");
        out.push_str("        let addr = req.starting_address.wrapping_add(i as u16);\n");
        out.push_str("        let st = dispatch_write_holding(userdata, addr, v);\n");
        out.push_str("        if st != MbusHookStatus::MbusHookOk { return hook_to_exception(st); }\n");
        out.push_str("    }\n");
        out.push_str("    unsafe {\n");
        out.push_str("        mbus_app_lock();\n");
        out.push_str("        if let Some(app) = (&mut *ptr::addr_of_mut!(APP_MODEL)).as_mut() {\n");
        out.push_str("            let _ = app.holding.write_many(req.starting_address, values);\n");
        out.push_str("        }\n");
        out.push_str("        mbus_app_unlock();\n");
        out.push_str("    }\n");
        out.push_str("    MbusServerExceptionCode::Ok\n");
        out.push_str("}\n\n");
    }

    // FC04 — Read Input Registers
    if has_input {
        out.push_str("/// Handler for FC 0x04 — Read Input Registers.\n");
        out.push_str("#[unsafe(no_mangle)]\n");
        out.push_str("pub unsafe extern \"C\" fn mbus_gen_on_read_input_registers(\n");
        out.push_str("    req: *mut MbusServerReadInputRegistersReq,\n");
        out.push_str("    _userdata: *mut c_void,\n");
        out.push_str(") -> MbusServerExceptionCode {\n");
        out.push_str("    if req.is_null() { return MbusServerExceptionCode::ServerDeviceFailure; }\n");
        out.push_str("    let req = unsafe { &mut *req };\n");
        out.push_str("    unsafe {\n");
        out.push_str("        mbus_app_lock();\n");
        out.push_str("        let result = (&*ptr::addr_of!(APP_MODEL))\n");
        out.push_str("            .as_ref()\n");
        out.push_str("            .and_then(|app| {\n");
        out.push_str("                let out = core::slice::from_raw_parts_mut(req.out_data, req.out_data_len);\n");
        out.push_str("                app.input.encode(req.address, req.quantity, out).ok()\n");
        out.push_str("            });\n");
        out.push_str("        mbus_app_unlock();\n");
        out.push_str("        match result {\n");
        out.push_str("            Some(n) => { req.out_byte_count = n; MbusServerExceptionCode::Ok }\n");
        out.push_str("            None => MbusServerExceptionCode::IllegalDataAddress,\n");
        out.push_str("        }\n");
        out.push_str("    }\n");
        out.push_str("}\n\n");
    }

    out
}

fn render_model_init_and_default_handlers(config: &ServerAppConfig) -> String {
    let coil_write_entries: Vec<&MapEntry> =
        config.memory_map.coils.iter().filter(|e| e.access.is_writable()).collect();
    let holding_write_entries: Vec<&MapEntry> =
        config.memory_map.holding_registers.iter().filter(|e| e.access.is_writable()).collect();
    let has_coils   = !config.memory_map.coils.is_empty();
    let has_discrete = !config.memory_map.discrete_inputs.is_empty();
    let has_holding = !config.memory_map.holding_registers.is_empty();
    let has_input   = !config.memory_map.input_registers.is_empty();

    let mut out = String::new();

    out.push_str("/// Initialise the generated Modbus register/coil model.\n");
    out.push_str("///\n");
    out.push_str("/// Must be called once before creating the server with `mbus_tcp_server_new` or\n");
    out.push_str("/// `mbus_serial_server_new`.\n");
    out.push_str("#[unsafe(no_mangle)]\n");
    out.push_str("pub extern \"C\" fn mbus_server_model_init() {\n");
    out.push_str("    unsafe { APP_MODEL = Some(AppModel::default()); }\n");
    out.push_str("}\n\n");

    out.push_str("/// Returns a fully-populated `MbusServerHandlers` struct pointing to the\n");
    out.push_str("/// generated handler callbacks.  Pass the returned struct to\n");
    out.push_str("/// `mbus_tcp_server_new` or `mbus_serial_server_new`.\n");
    out.push_str("///\n");
    out.push_str("/// `userdata` is forwarded to write-notification hooks as their first argument.\n");
    out.push_str("#[unsafe(no_mangle)]\n");
    out.push_str("pub extern \"C\" fn mbus_server_default_handlers(\n");
    out.push_str("    userdata: *mut c_void,\n");
    out.push_str(") -> MbusServerHandlers {\n");
    out.push_str("    MbusServerHandlers {\n");
    out.push_str("        userdata,\n");

    if has_coils {
        out.push_str("        on_read_coils: Some(mbus_gen_on_read_coils),\n");
    } else {
        out.push_str("        on_read_coils: None,\n");
    }
    if !coil_write_entries.is_empty() {
        out.push_str("        on_write_single_coil: Some(mbus_gen_on_write_single_coil),\n");
        out.push_str("        on_write_multiple_coils: Some(mbus_gen_on_write_multiple_coils),\n");
    } else {
        out.push_str("        on_write_single_coil: None,\n");
        out.push_str("        on_write_multiple_coils: None,\n");
    }
    if has_discrete {
        out.push_str("        on_read_discrete_inputs: Some(mbus_gen_on_read_discrete_inputs),\n");
    } else {
        out.push_str("        on_read_discrete_inputs: None,\n");
    }
    if has_holding {
        out.push_str("        on_read_holding_registers: Some(mbus_gen_on_read_holding_registers),\n");
    } else {
        out.push_str("        on_read_holding_registers: None,\n");
    }
    if !holding_write_entries.is_empty() {
        out.push_str("        on_write_single_register: Some(mbus_gen_on_write_single_register),\n");
        out.push_str("        on_write_multiple_registers: Some(mbus_gen_on_write_multiple_registers),\n");
    } else {
        out.push_str("        on_write_single_register: None,\n");
        out.push_str("        on_write_multiple_registers: None,\n");
    }
    out.push_str("        on_mask_write_register: None,\n");
    out.push_str("        on_read_write_multiple_registers: None,\n");
    if has_input {
        out.push_str("        on_read_input_registers: Some(mbus_gen_on_read_input_registers),\n");
    } else {
        out.push_str("        on_read_input_registers: None,\n");
    }
    out.push_str("        on_read_fifo_queue: None,\n");
    out.push_str("        on_read_file_record: None,\n");
    out.push_str("        on_write_file_record: None,\n");
    out.push_str("        on_read_exception_status: None,\n");
    out.push_str("        on_diagnostics: None,\n");
    out.push_str("        on_get_comm_event_counter: None,\n");
    out.push_str("        on_get_comm_event_log: None,\n");
    out.push_str("        on_report_server_id: None,\n");
    out.push_str("        on_read_device_identification: None,\n");
    out.push_str("    }\n");
    out.push_str("}\n");
    out
}

fn render_named_ffi(config: &ServerAppConfig) -> String {
    let mut out = String::new();
    out.push_str("// Named field accessors — push/pull values from your application code.\n");
    out.push_str("// These bypass write-notification hooks (app-side push, not a Modbus write).\n\n");

    for entry in &config.memory_map.coils {
        let name = &entry.name;
        out.push_str("#[unsafe(no_mangle)]\n");
        out.push_str(&format!("pub extern \"C\" fn mbus_server_set_{name}(value: u8) {{\n"));
        out.push_str("    unsafe {\n");
        out.push_str("        mbus_app_lock();\n");
        out.push_str(&format!(
            "        if let Some(app) = (&mut *ptr::addr_of_mut!(APP_MODEL)).as_mut() {{ app.coils.{name} = value != 0; }}\n"
        ));
        out.push_str("        mbus_app_unlock();\n");
        out.push_str("    }\n");
        out.push_str("}\n\n");
        out.push_str("#[unsafe(no_mangle)]\n");
        out.push_str(&format!(
            "pub extern \"C\" fn mbus_server_get_{name}(out_value: *mut u8) -> MbusHookStatus {{\n"
        ));
        out.push_str("    if out_value.is_null() { return MbusHookStatus::MbusHookDeviceFailure; }\n");
        out.push_str("    unsafe {\n");
        out.push_str("        mbus_app_lock();\n");
        out.push_str(&format!(
            "        let val = (&*ptr::addr_of!(APP_MODEL)).as_ref().map(|app| app.coils.{name} as u8);\n"
        ));
        out.push_str("        mbus_app_unlock();\n");
        out.push_str("        match val {\n");
        out.push_str("            Some(v) => { *out_value = v; MbusHookStatus::MbusHookOk }\n");
        out.push_str("            None => MbusHookStatus::MbusHookDeviceFailure,\n");
        out.push_str("        }\n");
        out.push_str("    }\n");
        out.push_str("}\n\n");
    }

    for entry in &config.memory_map.discrete_inputs {
        let name = &entry.name;
        out.push_str("#[unsafe(no_mangle)]\n");
        out.push_str(&format!("pub extern \"C\" fn mbus_server_set_{name}(value: u8) {{\n"));
        out.push_str("    unsafe {\n");
        out.push_str("        mbus_app_lock();\n");
        out.push_str(&format!(
            "        if let Some(app) = (&mut *ptr::addr_of_mut!(APP_MODEL)).as_mut() {{ app.discrete_inputs.{name} = value != 0; }}\n"
        ));
        out.push_str("        mbus_app_unlock();\n");
        out.push_str("    }\n");
        out.push_str("}\n\n");
        out.push_str("#[unsafe(no_mangle)]\n");
        out.push_str(&format!(
            "pub extern \"C\" fn mbus_server_get_{name}(out_value: *mut u8) -> MbusHookStatus {{\n"
        ));
        out.push_str("    if out_value.is_null() { return MbusHookStatus::MbusHookDeviceFailure; }\n");
        out.push_str("    unsafe {\n");
        out.push_str("        mbus_app_lock();\n");
        out.push_str(&format!(
            "        let val = (&*ptr::addr_of!(APP_MODEL)).as_ref().map(|app| app.discrete_inputs.{name} as u8);\n"
        ));
        out.push_str("        mbus_app_unlock();\n");
        out.push_str("        match val {\n");
        out.push_str("            Some(v) => { *out_value = v; MbusHookStatus::MbusHookOk }\n");
        out.push_str("            None => MbusHookStatus::MbusHookDeviceFailure,\n");
        out.push_str("        }\n");
        out.push_str("    }\n");
        out.push_str("}\n\n");
    }

    for entry in &config.memory_map.holding_registers {
        let name = &entry.name;
        out.push_str("#[unsafe(no_mangle)]\n");
        out.push_str(&format!("pub extern \"C\" fn mbus_server_set_{name}(value: u16) {{\n"));
        out.push_str("    unsafe {\n");
        out.push_str("        mbus_app_lock();\n");
        out.push_str(&format!(
            "        if let Some(app) = (&mut *ptr::addr_of_mut!(APP_MODEL)).as_mut() {{ app.holding.set_{name}(value); }}\n"
        ));
        out.push_str("        mbus_app_unlock();\n");
        out.push_str("    }\n");
        out.push_str("}\n\n");
        out.push_str("#[unsafe(no_mangle)]\n");
        out.push_str(&format!(
            "pub extern \"C\" fn mbus_server_get_{name}(out_value: *mut u16) -> MbusHookStatus {{\n"
        ));
        out.push_str("    if out_value.is_null() { return MbusHookStatus::MbusHookDeviceFailure; }\n");
        out.push_str("    unsafe {\n");
        out.push_str("        mbus_app_lock();\n");
        out.push_str(&format!(
            "        let val = (&*ptr::addr_of!(APP_MODEL)).as_ref().map(|app| app.holding.{name}());\n"
        ));
        out.push_str("        mbus_app_unlock();\n");
        out.push_str("        match val {\n");
        out.push_str("            Some(v) => { *out_value = v; MbusHookStatus::MbusHookOk }\n");
        out.push_str("            None => MbusHookStatus::MbusHookDeviceFailure,\n");
        out.push_str("        }\n");
        out.push_str("    }\n");
        out.push_str("}\n\n");
    }

    for entry in &config.memory_map.input_registers {
        let name = &entry.name;
        out.push_str("#[unsafe(no_mangle)]\n");
        out.push_str(&format!("pub extern \"C\" fn mbus_server_set_{name}(value: u16) {{\n"));
        out.push_str("    unsafe {\n");
        out.push_str("        mbus_app_lock();\n");
        out.push_str(&format!(
            "        if let Some(app) = (&mut *ptr::addr_of_mut!(APP_MODEL)).as_mut() {{ app.input.set_{name}(value); }}\n"
        ));
        out.push_str("        mbus_app_unlock();\n");
        out.push_str("    }\n");
        out.push_str("}\n\n");
        out.push_str("#[unsafe(no_mangle)]\n");
        out.push_str(&format!(
            "pub extern \"C\" fn mbus_server_get_{name}(out_value: *mut u16) -> MbusHookStatus {{\n"
        ));
        out.push_str("    if out_value.is_null() { return MbusHookStatus::MbusHookDeviceFailure; }\n");
        out.push_str("    unsafe {\n");
        out.push_str("        mbus_app_lock();\n");
        out.push_str(&format!(
            "        let val = (&*ptr::addr_of!(APP_MODEL)).as_ref().map(|app| app.input.{name}());\n"
        ));
        out.push_str("        mbus_app_unlock();\n");
        out.push_str("        match val {\n");
        out.push_str("            Some(v) => { *out_value = v; MbusHookStatus::MbusHookOk }\n");
        out.push_str("            None => MbusHookStatus::MbusHookDeviceFailure,\n");
        out.push_str("        }\n");
        out.push_str("    }\n");
        out.push_str("}\n\n");
    }

    out
}
