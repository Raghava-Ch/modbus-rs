use serde::Deserialize;
use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
struct RawConfig {
    #[serde(default = "default_version")]
    version: u16,
    #[serde(default)]
    units: Vec<RawUnit>,
}

#[derive(Debug, Deserialize)]
struct RawUnit {
    id: u8,
    #[serde(default)]
    coils: RawBitSection,
    #[serde(default)]
    discrete_inputs: RawBitSection,
    #[serde(default)]
    holding_registers: RawRegSection,
    #[serde(default)]
    input_registers: RawRegSection,
}

#[derive(Debug, Default, Deserialize)]
struct RawBitSection {
    #[serde(default)]
    blocks: Vec<RawBitBlock>,
    #[serde(default)]
    overrides: Vec<RawBitOverride>,
}

#[derive(Debug, Default, Deserialize)]
struct RawRegSection {
    #[serde(default)]
    blocks: Vec<RawRegBlock>,
    #[serde(default)]
    overrides: Vec<RawRegOverride>,
}

#[derive(Debug, Deserialize)]
struct RawBitBlock {
    start_address: u16,
    quantity: u16,
    default: bool,
    access: String,
}

#[derive(Debug, Deserialize)]
struct RawBitOverride {
    address: u16,
    value: bool,
}

#[derive(Debug, Deserialize)]
struct RawRegBlock {
    start_address: u16,
    quantity: u16,
    default: u16,
    access: String,
}

#[derive(Debug, Deserialize)]
struct RawRegOverride {
    address: u16,
    value: u16,
}

fn default_version() -> u16 {
    1
}

fn parse_access_mode(value: &str) -> Result<&'static str, String> {
    match value {
        "r" => Ok("AccessMode::ReadOnly"),
        "rw" => Ok("AccessMode::ReadWrite"),
        _ => Err(format!(
            "invalid access mode '{value}', expected 'r' or 'rw'"
        )),
    }
}

fn validate_span(start_address: u16, quantity: u16, context: &str) -> Result<(), String> {
    if quantity == 0 {
        return Err(format!("{context}: quantity must be > 0"));
    }

    let end = start_address as u32 + quantity as u32 - 1;
    if end > u16::MAX as u32 {
        return Err(format!(
            "{context}: address span overflows u16 range (start={start_address}, quantity={quantity})"
        ));
    }

    Ok(())
}

fn spans_overlap(a_start: u16, a_qty: u16, b_start: u16, b_qty: u16) -> bool {
    let a_end = a_start as u32 + a_qty as u32 - 1;
    let b_end = b_start as u32 + b_qty as u32 - 1;
    !(a_end < b_start as u32 || b_end < a_start as u32)
}

fn validate_non_overlapping_bit_blocks(
    section_name: &str,
    unit_id: u8,
    blocks: &[RawBitBlock],
) -> Result<(), String> {
    for (i, lhs) in blocks.iter().enumerate() {
        validate_span(
            lhs.start_address,
            lhs.quantity,
            &format!("unit {unit_id} {section_name} block {i}"),
        )?;
        parse_access_mode(&lhs.access)?;

        for (j, rhs) in blocks.iter().enumerate().skip(i + 1) {
            if spans_overlap(
                lhs.start_address,
                lhs.quantity,
                rhs.start_address,
                rhs.quantity,
            ) {
                return Err(format!(
                    "unit {unit_id} {section_name}: overlapping blocks {i} and {j}"
                ));
            }
        }
    }

    Ok(())
}

fn validate_non_overlapping_reg_blocks(
    section_name: &str,
    unit_id: u8,
    blocks: &[RawRegBlock],
) -> Result<(), String> {
    for (i, lhs) in blocks.iter().enumerate() {
        validate_span(
            lhs.start_address,
            lhs.quantity,
            &format!("unit {unit_id} {section_name} block {i}"),
        )?;
        parse_access_mode(&lhs.access)?;

        for (j, rhs) in blocks.iter().enumerate().skip(i + 1) {
            if spans_overlap(
                lhs.start_address,
                lhs.quantity,
                rhs.start_address,
                rhs.quantity,
            ) {
                return Err(format!(
                    "unit {unit_id} {section_name}: overlapping blocks {i} and {j}"
                ));
            }
        }
    }

    Ok(())
}

fn bit_override_in_any_block(address: u16, blocks: &[RawBitBlock]) -> bool {
    blocks.iter().any(|b| {
        let end = b.start_address as u32 + b.quantity as u32 - 1;
        (b.start_address as u32..=end).contains(&(address as u32))
    })
}

fn reg_override_in_any_block(address: u16, blocks: &[RawRegBlock]) -> bool {
    blocks.iter().any(|b| {
        let end = b.start_address as u32 + b.quantity as u32 - 1;
        (b.start_address as u32..=end).contains(&(address as u32))
    })
}

fn validate_config(cfg: &RawConfig) -> Result<(), String> {
    for unit in &cfg.units {
        if unit.id == 0 || unit.id > 247 {
            return Err(format!("unit id {} is invalid, expected 1..=247", unit.id));
        }

        validate_non_overlapping_bit_blocks("coils", unit.id, &unit.coils.blocks)?;
        validate_non_overlapping_bit_blocks(
            "discrete_inputs",
            unit.id,
            &unit.discrete_inputs.blocks,
        )?;
        validate_non_overlapping_reg_blocks(
            "holding_registers",
            unit.id,
            &unit.holding_registers.blocks,
        )?;
        validate_non_overlapping_reg_blocks(
            "input_registers",
            unit.id,
            &unit.input_registers.blocks,
        )?;

        for (idx, ov) in unit.coils.overrides.iter().enumerate() {
            if !bit_override_in_any_block(ov.address, &unit.coils.blocks) {
                return Err(format!(
                    "unit {} coils override {} address {} is outside declared blocks",
                    unit.id, idx, ov.address
                ));
            }
        }

        for (idx, ov) in unit.discrete_inputs.overrides.iter().enumerate() {
            if !bit_override_in_any_block(ov.address, &unit.discrete_inputs.blocks) {
                return Err(format!(
                    "unit {} discrete_inputs override {} address {} is outside declared blocks",
                    unit.id, idx, ov.address
                ));
            }
        }

        for (idx, ov) in unit.holding_registers.overrides.iter().enumerate() {
            if !reg_override_in_any_block(ov.address, &unit.holding_registers.blocks) {
                return Err(format!(
                    "unit {} holding_registers override {} address {} is outside declared blocks",
                    unit.id, idx, ov.address
                ));
            }
        }

        for (idx, ov) in unit.input_registers.overrides.iter().enumerate() {
            if !reg_override_in_any_block(ov.address, &unit.input_registers.blocks) {
                return Err(format!(
                    "unit {} input_registers override {} address {} is outside declared blocks",
                    unit.id, idx, ov.address
                ));
            }
        }
    }

    Ok(())
}

fn write_generated(cfg: &RawConfig, out_file: &str) {
    let mut coil_blocks = Vec::<String>::new();
    let mut di_blocks = Vec::<String>::new();
    let mut hr_blocks = Vec::<String>::new();
    let mut ir_blocks = Vec::<String>::new();

    let mut coil_overrides = Vec::<String>::new();
    let mut di_overrides = Vec::<String>::new();
    let mut hr_overrides = Vec::<String>::new();
    let mut ir_overrides = Vec::<String>::new();

    let mut units = Vec::<String>::new();

    for unit in &cfg.units {
        let coils_block_start = coil_blocks.len();
        for b in &unit.coils.blocks {
            let access = parse_access_mode(&b.access).expect("validated access");
            coil_blocks.push(format!(
                "BitBlock {{ start_address: {}, quantity: {}, default: {}, access: {} }}",
                b.start_address, b.quantity, b.default, access
            ));
        }

        let di_block_start = di_blocks.len();
        for b in &unit.discrete_inputs.blocks {
            let access = parse_access_mode(&b.access).expect("validated access");
            di_blocks.push(format!(
                "BitBlock {{ start_address: {}, quantity: {}, default: {}, access: {} }}",
                b.start_address, b.quantity, b.default, access
            ));
        }

        let hr_block_start = hr_blocks.len();
        for b in &unit.holding_registers.blocks {
            let access = parse_access_mode(&b.access).expect("validated access");
            hr_blocks.push(format!(
                "RegisterBlock {{ start_address: {}, quantity: {}, default: {}, access: {} }}",
                b.start_address, b.quantity, b.default, access
            ));
        }

        let ir_block_start = ir_blocks.len();
        for b in &unit.input_registers.blocks {
            let access = parse_access_mode(&b.access).expect("validated access");
            ir_blocks.push(format!(
                "RegisterBlock {{ start_address: {}, quantity: {}, default: {}, access: {} }}",
                b.start_address, b.quantity, b.default, access
            ));
        }

        let coils_ov_start = coil_overrides.len();
        for ov in &unit.coils.overrides {
            coil_overrides.push(format!(
                "BitOverride {{ address: {}, value: {} }}",
                ov.address, ov.value
            ));
        }

        let di_ov_start = di_overrides.len();
        for ov in &unit.discrete_inputs.overrides {
            di_overrides.push(format!(
                "BitOverride {{ address: {}, value: {} }}",
                ov.address, ov.value
            ));
        }

        let hr_ov_start = hr_overrides.len();
        for ov in &unit.holding_registers.overrides {
            hr_overrides.push(format!(
                "RegisterOverride {{ address: {}, value: {} }}",
                ov.address, ov.value
            ));
        }

        let ir_ov_start = ir_overrides.len();
        for ov in &unit.input_registers.overrides {
            ir_overrides.push(format!(
                "RegisterOverride {{ address: {}, value: {} }}",
                ov.address, ov.value
            ));
        }

        units.push(format!(
            "UnitDescriptor {{\n  id: {},\n  coils: SectionSlice {{ block_start: {}, block_count: {}, override_start: {}, override_count: {} }},\n  discrete_inputs: SectionSlice {{ block_start: {}, block_count: {}, override_start: {}, override_count: {} }},\n  holding_registers: SectionSlice {{ block_start: {}, block_count: {}, override_start: {}, override_count: {} }},\n  input_registers: SectionSlice {{ block_start: {}, block_count: {}, override_start: {}, override_count: {} }},\n}}",
            unit.id,
            coils_block_start,
            unit.coils.blocks.len(),
            coils_ov_start,
            unit.coils.overrides.len(),
            di_block_start,
            unit.discrete_inputs.blocks.len(),
            di_ov_start,
            unit.discrete_inputs.overrides.len(),
            hr_block_start,
            unit.holding_registers.blocks.len(),
            hr_ov_start,
            unit.holding_registers.overrides.len(),
            ir_block_start,
            unit.input_registers.blocks.len(),
            ir_ov_start,
            unit.input_registers.overrides.len(),
        ));
    }

    let content = format!(
        "// @generated by mbus-server/build.rs\n\
pub const GENERATED_MAP_VERSION: u16 = {};\n\
pub const GENERATED_UNITS: [UnitDescriptor; {}] = [{}];\n\
pub const GENERATED_COIL_BLOCKS: [BitBlock; {}] = [{}];\n\
pub const GENERATED_DISCRETE_INPUT_BLOCKS: [BitBlock; {}] = [{}];\n\
pub const GENERATED_HOLDING_REGISTER_BLOCKS: [RegisterBlock; {}] = [{}];\n\
pub const GENERATED_INPUT_REGISTER_BLOCKS: [RegisterBlock; {}] = [{}];\n\
pub const GENERATED_COIL_OVERRIDES: [BitOverride; {}] = [{}];\n\
pub const GENERATED_DISCRETE_INPUT_OVERRIDES: [BitOverride; {}] = [{}];\n\
pub const GENERATED_HOLDING_REGISTER_OVERRIDES: [RegisterOverride; {}] = [{}];\n\
pub const GENERATED_INPUT_REGISTER_OVERRIDES: [RegisterOverride; {}] = [{}];\n",
        cfg.version,
        units.len(),
        units.join(","),
        coil_blocks.len(),
        coil_blocks.join(","),
        di_blocks.len(),
        di_blocks.join(","),
        hr_blocks.len(),
        hr_blocks.join(","),
        ir_blocks.len(),
        ir_blocks.join(","),
        coil_overrides.len(),
        coil_overrides.join(","),
        di_overrides.len(),
        di_overrides.join(","),
        hr_overrides.len(),
        hr_overrides.join(","),
        ir_overrides.len(),
        ir_overrides.join(",")
    );

    fs::write(out_file, content).expect("failed to write generated server map");
}

fn main() {
    println!("cargo::rustc-check-cfg=cfg(rust_analyzer)");
    println!("cargo::rerun-if-env-changed=MBUS_SERVER_MAP");

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set");
    let out_file = format!("{out_dir}/server_map_generated.rs");
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");

    let cfg = match env::var("MBUS_SERVER_MAP") {
        Ok(path) => {
            let resolved_path = {
                let p = PathBuf::from(&path);
                if p.is_absolute() {
                    p
                } else {
                    PathBuf::from(&manifest_dir).join(p)
                }
            };

            let resolved_display = resolved_path.to_string_lossy();
            println!("cargo::rerun-if-changed={}", resolved_display);

            let text = fs::read_to_string(&resolved_path).unwrap_or_else(|err| {
                panic!(
                    "failed to read MBUS_SERVER_MAP file '{}' (resolved from '{}'): {err}",
                    resolved_display, path
                )
            });

            let parsed: RawConfig = toml::from_str(&text).unwrap_or_else(|err| {
                panic!("invalid MBUS_SERVER_MAP TOML '{}': {err}", resolved_display)
            });

            if let Err(err) = validate_config(&parsed) {
                panic!(
                    "invalid MBUS_SERVER_MAP config '{}': {err}",
                    resolved_display
                );
            }

            parsed
        }
        Err(_) => RawConfig {
            version: default_version(),
            units: Vec::new(),
        },
    };

    write_generated(&cfg, &out_file);
}
