use proc_macro::TokenStream;
use quote::quote;
use syn::{
    Data, DeriveInput, Error, Expr, ExprLit, Fields, Ident, ItemStruct, Lit, LitFloat, LitInt,
    LitStr, Meta, parse::Parser, parse_macro_input,
};

#[proc_macro_derive(CoilsModel, attributes(coil))]
pub fn derive_coils_model(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand_coils_model(&input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

// ---------------------------------------------------------------------------
// New: HoldingRegistersModel derive (simple, wire-ready u16 fields + #[reg(addr)])
// ---------------------------------------------------------------------------

#[proc_macro_derive(HoldingRegistersModel, attributes(reg))]
pub fn derive_holding_registers_model(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand_holding_registers(&input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

#[proc_macro_derive(InputRegistersModel, attributes(reg))]
pub fn derive_input_registers_model(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand_input_registers(&input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

#[proc_macro_derive(DiscreteInputsModel, attributes(discrete_input))]
pub fn derive_discrete_inputs_model(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand_discrete_inputs_model(&input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

// ---------------------------------------------------------------------------
// New: modbus_app attribute macro
// ---------------------------------------------------------------------------

#[proc_macro_attribute]
pub fn modbus_app(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemStruct);
    let selected_fields = match parse_modbus_app_args(attr) {
        Ok(v) => v,
        Err(err) => return err.to_compile_error().into(),
    };
    match expand_modbus_app_struct(&input, &selected_fields) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

/// Hook spec for a single-write at a known Modbus address.
/// `on_write_N = fn_name` in the #[modbus_app(...)] attribute — where N is the decimal address.
#[derive(Debug, Clone)]
struct SingleWriteHook {
    addr: u16,
    addr_text: String,
    hook: Ident,
}

/// Per-group write-hook configuration for writable map groups (coils / holding_registers).
#[derive(Default, Debug)]
struct WritableGroupConfig {
    /// App struct field idents that belong to this group.
    field_idents: Vec<Ident>,
    /// Optional `on_batch_write = fn_name` for FC0F / FC10.
    on_batch_write: Option<Ident>,
    /// Zero or more `on_write_N = fn_name` hooks (by Modbus address).
    single_write_hooks: Vec<SingleWriteHook>,
}

#[derive(Default)]
struct SelectedAppFields {
    holding_registers: WritableGroupConfig,
    input_registers: Vec<Ident>,
    coils: WritableGroupConfig,
    discrete_inputs: Vec<Ident>,
}

/// Parse `group_name(field1, field2, ..., on_batch_write = fn, on_write_N = fn, ...)`.
///
/// Field idents are the leading bare identifiers; key-value pairs starting with
/// `on_batch_write` or `on_write_<addr>` are hook declarations.
fn parse_writable_group(meta: Meta, group_name: &str) -> Result<WritableGroupConfig, Error> {
    let list = match meta {
        Meta::List(list) if list.path.is_ident(group_name) => list,
        other => {
            return Err(Error::new_spanned(
                other,
                format!(
                    "invalid #[modbus_app(...)] group; expected {}(field, ...)",
                    group_name
                ),
            ));
        }
    };

    let mut cfg = WritableGroupConfig::default();

    list.parse_nested_meta(|nested| {
        let key = nested
            .path
            .get_ident()
            .cloned()
            .ok_or_else(|| nested.error("expected an identifier"))?;

        let key_str = key.to_string();

        if key_str == "on_batch_write" {
            // on_batch_write = fn_name
            let batch_fn: Ident = nested.value()?.parse()?;
            cfg.on_batch_write = Some(batch_fn);
        } else if let Some(addr_str) = key_str.strip_prefix("on_write_") {
            // on_write_N = fn_name  (N is a decimal Modbus address)
            let addr: u16 = addr_str.parse().map_err(|_| {
                nested.error(format!(
                    "invalid address in `{}`; expected `on_write_<u16>`, e.g. `on_write_0`",
                    key_str
                ))
            })?;
            if cfg.single_write_hooks.iter().any(|h| h.addr == addr) {
                return Err(nested.error(format!(
                    "duplicate on_write_{} in #[modbus_app({}(...))]; each single-write address can be configured once",
                    addr, group_name
                )));
            }
            let hook_fn: Ident = nested.value()?.parse()?;
            cfg.single_write_hooks.push(SingleWriteHook {
                addr,
                addr_text: addr_str.to_owned(),
                hook: hook_fn,
            });
        } else {
            // bare field ident
            if nested.input.is_empty() || nested.input.peek(syn::Token![,]) {
                cfg.field_idents.push(key);
            } else {
                return Err(nested.error(format!(
                    "unexpected key `{key_str}` in #[modbus_app({group_name}(...))]; \
                     valid options: field names, `on_batch_write = fn`, `on_write_N = fn`"
                )));
            }
        }
        Ok(())
    })?;

    if cfg.field_idents.is_empty() {
        return Err(Error::new_spanned(
            list,
            format!(
                "{}(...) requires at least one field ident; example: {}(my_map)",
                group_name, group_name
            ),
        ));
    }
    Ok(cfg)
}

fn parse_group_idents(meta: Meta, group_name: &str) -> Result<Vec<Ident>, Error> {
    let list = match meta {
        Meta::List(list) if list.path.is_ident(group_name) => list,
        other => {
            return Err(Error::new_spanned(
                other,
                format!(
                    "invalid #[modbus_app(...)] group; expected {}(field1, field2, ...)",
                    group_name
                ),
            ));
        }
    };

    let mut out = Vec::new();
    list.parse_nested_meta(|nested| {
        let ident = nested.path.get_ident().cloned().ok_or_else(|| {
            nested.error("expected a field identifier, e.g. input_registers(my_map)")
        })?;
        out.push(ident);
        Ok(())
    })?;

    if out.is_empty() {
        return Err(Error::new_spanned(
            list,
            format!(
                "{}(...) requires at least one field; example: {}(my_map)",
                group_name, group_name
            ),
        ));
    }

    Ok(out)
}

fn parse_modbus_app_args(attr: TokenStream) -> Result<SelectedAppFields, Error> {
    let tokens: proc_macro2::TokenStream = attr.into();
    if tokens.is_empty() {
        return Ok(SelectedAppFields::default());
    }

    let parser = syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated;
    let groups = parser.parse2(tokens)?;

    let mut selected = SelectedAppFields::default();
    for meta in groups {
        let path_ident = meta
            .path()
            .get_ident()
            .map(ToString::to_string)
            .ok_or_else(|| {
                Error::new_spanned(
                    &meta,
                    "expected a group name in #[modbus_app(...)]; allowed groups: holding_registers, input_registers, coils",
                )
            })?;
        match path_ident.as_str() {
            "holding_registers" => {
                selected.holding_registers = parse_writable_group(meta, "holding_registers")?
            }
            "input_registers" => {
                selected.input_registers = parse_group_idents(meta, "input_registers")?
            }
            "coils" => selected.coils = parse_writable_group(meta, "coils")?,
            "discrete_inputs" => {
                selected.discrete_inputs = parse_group_idents(meta, "discrete_inputs")?
            }
            _ => {
                return Err(Error::new_spanned(
                    meta,
                    "invalid #[modbus_app(...)] syntax; expected #[modbus_app(holding_registers(...), input_registers(...), coils(...), discrete_inputs(...))]",
                ));
            }
        }
    }

    Ok(selected)
}

#[derive(Debug, Clone)]
struct CoilField {
    ident: Ident,
    addr: u16,
    /// When true, a single FC05 write with no individual hook falls through to the
    /// map-level `on_batch_write` hook (called with qty = 1) if that hook exists.
    notify_via_batch: bool,
}

/// Field for `#[derive(DiscreteInputsModel)]`: a read-only `bool` at a fixed address.
#[derive(Debug, Clone)]
struct DiscreteInputField {
    ident: Ident,
    addr: u16,
}

/// Simple field for `#[derive(HoldingRegistersModel)]`: always a `u16` at a fixed address.
#[derive(Debug, Clone)]
struct RegField {
    ident: Ident,
    addr: u16,
    scale: f32,
    has_scale: bool,
    unit: Option<String>,
    /// When true, a single FC06 write with no individual hook falls through to the
    /// map-level `on_batch_write` hook (called with qty = 1) if that hook exists.
    notify_via_batch: bool,
}

fn expand_coils_model(input: &DeriveInput) -> Result<proc_macro2::TokenStream, Error> {
    let struct_name = &input.ident;
    let fields = parse_coils_fields(input)?;
    validate_duplicate_bit_addresses(&fields)?;

    let addr_min = fields.iter().map(|f| f.addr).min().unwrap_or(0);
    let addr_max = fields.iter().map(|f| f.addr).max().unwrap_or(0);
    let bit_count = addr_max.saturating_sub(addr_min) as usize + 1;
    let has_batch_notified_fields = fields.iter().any(|f| f.notify_via_batch);
    let encode_arms = fields.iter().map(|f| {
        let ident = &f.ident;
        let addr = f.addr;
        quote! { #addr => self.#ident, }
    });
    let write_arms = fields.iter().map(|f| {
        let ident = &f.ident;
        let addr = f.addr;
        quote! {
            #addr => {
                self.#ident = value;
                ::core::result::Result::Ok(())
            }
        }
    });
    let batch_notify_arms: Vec<_> = fields
        .iter()
        .filter(|f| f.notify_via_batch)
        .map(|f| {
            let addr = f.addr;
            quote! { #addr => true, }
        })
        .collect();

    Ok(quote! {
        impl ::mbus_server::CoilMap for #struct_name {
            const ADDR_MIN: u16 = #addr_min;
            const ADDR_MAX: u16 = #addr_max;
            const BIT_COUNT: usize = #bit_count;
            const HAS_BATCH_NOTIFIED_FIELDS: bool = #has_batch_notified_fields;

            fn encode(
                &self,
                address: u16,
                quantity: u16,
                out: &mut [u8],
            ) -> ::core::result::Result<u8, ::mbus_core::errors::MbusError> {
                if quantity == 0 {
                    return ::core::result::Result::Err(::mbus_core::errors::MbusError::InvalidQuantity);
                }

                let req_start = address as usize;
                let qty = quantity as usize;
                let req_end = req_start
                    .checked_add(qty)
                    .ok_or(::mbus_core::errors::MbusError::InvalidAddress)?;
                let map_start = Self::ADDR_MIN as usize;
                let map_end = Self::ADDR_MAX as usize + 1;
                if req_start < map_start || req_end > map_end {
                    return ::core::result::Result::Err(::mbus_core::errors::MbusError::InvalidAddress);
                }

                let byte_len = qty.div_ceil(8);
                if out.len() < byte_len {
                    return ::core::result::Result::Err(::mbus_core::errors::MbusError::BufferTooSmall);
                }
                out[..byte_len].fill(0);

                for index in 0..qty {
                    let cur_addr = (req_start + index) as u16;
                    let value = match cur_addr {
                        #(#encode_arms)*
                        _ => return ::core::result::Result::Err(::mbus_core::errors::MbusError::InvalidAddress),
                    };
                    if value {
                        out[index / 8] |= 1 << (index % 8);
                    }
                }

                ::core::result::Result::Ok(byte_len as u8)
            }

            fn write_single(
                &mut self,
                address: u16,
                value: bool,
            ) -> ::core::result::Result<(), ::mbus_core::errors::MbusError> {
                match address {
                    #(#write_arms,)*
                    _ => ::core::result::Result::Err(::mbus_core::errors::MbusError::InvalidAddress),
                }
            }

            fn write_many_from_packed(
                &mut self,
                address: u16,
                quantity: u16,
                values: &[u8],
                packed_bit_offset: usize,
            ) -> ::core::result::Result<(), ::mbus_core::errors::MbusError> {
                if quantity == 0 {
                    return ::core::result::Result::Err(::mbus_core::errors::MbusError::InvalidQuantity);
                }

                let end_addr = address
                    .checked_add(quantity - 1)
                    .ok_or(::mbus_core::errors::MbusError::InvalidAddress)?;
                if address < Self::ADDR_MIN || end_addr > Self::ADDR_MAX {
                    return ::core::result::Result::Err(::mbus_core::errors::MbusError::InvalidAddress);
                }

                for index in 0..quantity as usize {
                    let absolute_bit = packed_bit_offset + index;
                    let byte_index = absolute_bit / 8;
                    if byte_index >= values.len() {
                        return ::core::result::Result::Err(::mbus_core::errors::MbusError::InvalidByteCount);
                    }
                    let bit_mask = 1u8 << (absolute_bit % 8);
                    let value = values[byte_index] & bit_mask != 0;
                    self.write_single(address + index as u16, value)?;
                }

                ::core::result::Result::Ok(())
            }

            fn is_batch_notified(addr: u16) -> bool {
                match addr {
                    #(#batch_notify_arms)*
                    _ => false,
                }
            }
        }
    })
}

fn parse_coils_fields(input: &DeriveInput) -> Result<Vec<CoilField>, Error> {
    let data = match &input.data {
        Data::Struct(data) => data,
        _ => {
            return Err(Error::new_spanned(
                input,
                "CoilsModel can only be derived for structs; use `struct MyCoils { ... }` with named fields",
            ));
        }
    };

    let named = match &data.fields {
        Fields::Named(named) => named,
        _ => {
            return Err(Error::new_spanned(
                input,
                "CoilsModel requires named fields; tuple/unit structs are not supported",
            ));
        }
    };

    let mut out = Vec::new();
    for field in &named.named {
        let ident = field.ident.clone().ok_or_else(|| {
            Error::new_spanned(
                field,
                "field identifier missing; CoilsModel expects named fields",
            )
        })?;

        let ty = &field.ty;
        let ty_ok = match ty {
            syn::Type::Path(p) => p
                .path
                .segments
                .last()
                .map(|seg| seg.ident == "bool")
                .unwrap_or(false),
            _ => false,
        };
        if !ty_ok {
            return Err(Error::new_spanned(
                ty,
                "CoilsModel fields must be bool (phase 1 limitation); change this field type to bool",
            ));
        }

        let mut addr: Option<u16> = None;
        let mut notify_via_batch = false;
        parse_coil_attr(field, |key, lit| match (key.as_str(), lit) {
            ("addr", Lit::Int(v)) => {
                addr = Some(parse_u16(v)?);
                Ok(())
            }
            ("notify_via_batch", Lit::Bool(v)) => {
                notify_via_batch = v.value;
                Ok(())
            }
            _ => Err(Error::new_spanned(
                lit,
                "unsupported #[coil(...)] key/value; expected #[coil(addr = N)] or #[coil(addr = N, notify_via_batch = true)]",
            )),
        })?;

        let addr = addr.ok_or_else(|| {
            Error::new_spanned(
                field,
                "missing required #[coil(addr = N)] for CoilsModel field; example: #[coil(addr = 0)]",
            )
        })?;

        out.push(CoilField {
            ident,
            addr,
            notify_via_batch,
        });
    }

    Ok(out)
}

fn parse_coil_attr(
    field: &syn::Field,
    mut on_pair: impl FnMut(String, &Lit) -> Result<(), Error>,
) -> Result<(), Error> {
    let mut found = false;
    for attr in &field.attrs {
        if !attr.path().is_ident("coil") {
            continue;
        }
        found = true;

        attr.parse_nested_meta(|meta| {
            let key = meta
                .path
                .get_ident()
                .map(|i| i.to_string())
                .ok_or_else(|| {
                    Error::new_spanned(
                        &meta.path,
                        "unsupported #[coil(...)] key; supported keys: `addr`, `notify_via_batch`",
                    )
                })?;

            let value_expr: Expr = meta.value()?.parse()?;
            match value_expr {
                Expr::Lit(ExprLit { lit, .. }) => on_pair(key, &lit),
                _ => Err(Error::new_spanned(
                    value_expr,
                    "#[coil(...)] values must be literals; example: #[coil(addr = 12)]",
                )),
            }
        })?;
    }

    if !found {
        return Err(Error::new_spanned(
            field,
            "missing required #[coil(...)] attribute; add #[coil(addr = N)] to each bool field",
        ));
    }

    Ok(())
}

fn parse_u16(v: &LitInt) -> Result<u16, Error> {
    v.base10_parse::<u16>()
        .map_err(|_| Error::new_spanned(v, "expected a u16 literal (0..=65535)"))
}

fn parse_f32(v: &LitFloat) -> Result<f32, Error> {
    v.base10_parse::<f32>()
        .map_err(|_| Error::new_spanned(v, "expected an f32 literal (e.g. 0.1)"))
}

fn parse_f32_from_int(v: &LitInt) -> Result<f32, Error> {
    v.base10_parse::<f32>()
        .map_err(|_| Error::new_spanned(v, "expected a numeric literal for scale (e.g. 1 or 0.1)"))
}

fn expand_discrete_inputs_model(input: &DeriveInput) -> Result<proc_macro2::TokenStream, Error> {
    let struct_name = &input.ident;
    let fields = parse_discrete_inputs_fields(input)?;
    validate_duplicate_discrete_input_addresses(&fields)?;

    let addr_min = fields.iter().map(|f| f.addr).min().unwrap_or(0);
    let addr_max = fields.iter().map(|f| f.addr).max().unwrap_or(0);
    let bit_count = addr_max.saturating_sub(addr_min) as usize + 1;

    let encode_arms = fields.iter().map(|f| {
        let ident = &f.ident;
        let addr = f.addr;
        quote! { #addr => self.#ident, }
    });

    Ok(quote! {
        impl ::mbus_server::DiscreteInputMap for #struct_name {
            const ADDR_MIN: u16 = #addr_min;
            const ADDR_MAX: u16 = #addr_max;
            const BIT_COUNT: usize = #bit_count;

            fn encode(
                &self,
                address: u16,
                quantity: u16,
                out: &mut [u8],
            ) -> ::core::result::Result<u8, ::mbus_core::errors::MbusError> {
                if quantity == 0 {
                    return ::core::result::Result::Err(::mbus_core::errors::MbusError::InvalidQuantity);
                }

                let req_start = address as usize;
                let qty = quantity as usize;
                let req_end = req_start
                    .checked_add(qty)
                    .ok_or(::mbus_core::errors::MbusError::InvalidAddress)?;
                let map_start = Self::ADDR_MIN as usize;
                let map_end = Self::ADDR_MAX as usize + 1;
                if req_start < map_start || req_end > map_end {
                    return ::core::result::Result::Err(::mbus_core::errors::MbusError::InvalidAddress);
                }

                let byte_len = qty.div_ceil(8);
                if out.len() < byte_len {
                    return ::core::result::Result::Err(::mbus_core::errors::MbusError::BufferTooSmall);
                }
                out[..byte_len].fill(0);

                for index in 0..qty {
                    let cur_addr = (req_start + index) as u16;
                    let value = match cur_addr {
                        #(#encode_arms)*
                        _ => return ::core::result::Result::Err(::mbus_core::errors::MbusError::InvalidAddress),
                    };
                    if value {
                        out[index / 8] |= 1 << (index % 8);
                    }
                }

                ::core::result::Result::Ok(byte_len as u8)
            }
        }
    })
}

fn parse_discrete_inputs_fields(input: &DeriveInput) -> Result<Vec<DiscreteInputField>, Error> {
    let data = match &input.data {
        Data::Struct(data) => data,
        _ => {
            return Err(Error::new_spanned(
                input,
                "DiscreteInputsModel can only be derived for structs; use `struct MyDiscreteInputs { ... }` with named fields",
            ));
        }
    };

    let named = match &data.fields {
        Fields::Named(named) => named,
        _ => {
            return Err(Error::new_spanned(
                input,
                "DiscreteInputsModel requires named fields; tuple/unit structs are not supported",
            ));
        }
    };

    let mut out = Vec::new();
    for field in &named.named {
        let ident = field.ident.clone().ok_or_else(|| {
            Error::new_spanned(
                field,
                "field identifier missing; DiscreteInputsModel expects named fields",
            )
        })?;

        let ty = &field.ty;
        let ty_ok = match ty {
            syn::Type::Path(p) => p
                .path
                .segments
                .last()
                .map(|seg| seg.ident == "bool")
                .unwrap_or(false),
            _ => false,
        };
        if !ty_ok {
            return Err(Error::new_spanned(
                ty,
                "DiscreteInputsModel fields must be bool; change this field type to bool",
            ));
        }

        let mut addr: Option<u16> = None;
        parse_discrete_input_attr(field, |key, lit| match (key.as_str(), lit) {
            ("addr", Lit::Int(v)) => {
                addr = Some(parse_u16(v)?);
                Ok(())
            }
            _ => Err(Error::new_spanned(
                lit,
                "unsupported #[discrete_input(...)] key/value; expected #[discrete_input(addr = N)]",
            )),
        })?;

        let addr = addr.ok_or_else(|| {
            Error::new_spanned(
                field,
                "missing required #[discrete_input(addr = N)] for DiscreteInputsModel field; example: #[discrete_input(addr = 0)]",
            )
        })?;

        out.push(DiscreteInputField { ident, addr });
    }

    Ok(out)
}

fn parse_discrete_input_attr<F>(field: &syn::Field, mut callback: F) -> Result<(), Error>
where
    F: FnMut(String, &Lit) -> Result<(), Error>,
{
    for attr in &field.attrs {
        if !attr.path().is_ident("discrete_input") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            let key = meta
                .path
                .get_ident()
                .ok_or_else(|| Error::new_spanned(&meta.path, "expected simple key"))?
                .to_string();
            meta.input.parse::<syn::Token![=]>()?;
            let expr: Expr = meta.input.parse()?;
            if let Expr::Lit(ExprLit { lit, .. }) = expr {
                callback(key, &lit)
            } else {
                Err(Error::new_spanned(
                    expr,
                    "#[discrete_input(...)] values must be literals",
                ))
            }
        })?;
    }
    Ok(())
}

fn validate_duplicate_discrete_input_addresses(fields: &[DiscreteInputField]) -> Result<(), Error> {
    let mut seen = std::collections::HashSet::new();
    for field in fields {
        if !seen.insert(field.addr) {
            return Err(Error::new_spanned(
                &field.ident,
                format!(
                    "duplicate discrete input address {}; each discrete input must have a unique address",
                    field.addr
                ),
            ));
        }
    }
    Ok(())
}

fn validate_duplicate_bit_addresses(fields: &[CoilField]) -> Result<(), Error> {
    for (i, a) in fields.iter().enumerate() {
        for b in fields.iter().skip(i + 1) {
            if a.addr == b.addr {
                return Err(Error::new(
                    proc_macro2::Span::call_site(),
                    format!(
                        "duplicate coil address {} for fields '{}' and '{}'; each coil address must be unique",
                        a.addr, a.ident, b.ident,
                    ),
                ));
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// HoldingRegistersModel derive expansion
// ---------------------------------------------------------------------------

fn expand_holding_registers(input: &DeriveInput) -> Result<proc_macro2::TokenStream, Error> {
    let struct_name = &input.ident;
    let fields = parse_reg_fields(input, "HoldingRegistersModel")?;
    let allow_gaps = parse_holding_registers_options(input)?;
    validate_duplicate_reg_addresses(&fields)?;
    if !allow_gaps {
        validate_contiguous_reg_addresses(&fields, input, "HoldingRegistersModel")?;
    }

    if fields.is_empty() {
        return Err(Error::new_spanned(
            input,
            "HoldingRegistersModel requires at least one #[reg(addr = N)] field",
        ));
    }

    let addr_min = fields.iter().map(|f| f.addr).min().unwrap();
    let addr_max = fields.iter().map(|f| f.addr).max().unwrap();
    let word_count = (addr_max as usize) - (addr_min as usize) + 1;
    let has_batch_notified_fields = fields.iter().any(|f| f.notify_via_batch);

    let mut sorted_fields = fields.clone();
    sorted_fields.sort_by_key(|f| f.addr);

    let hr_batch_notify_arms: Vec<_> = fields
        .iter()
        .filter(|f| f.notify_via_batch)
        .map(|f| {
            let addr = f.addr;
            quote! { #addr => true, }
        })
        .collect();

    let getters_setters = fields.iter().map(|f| {
        let ident = &f.ident;
        let setter = quote::format_ident!("set_{}", &f.ident);
        let scaled_getter = quote::format_ident!("{}_scaled", &f.ident);
        let scaled_setter = quote::format_ident!("set_{}_scaled", &f.ident);
        let scale = f.scale;
        let scaled_methods = if f.has_scale {
            quote! {
                pub fn #scaled_getter(&self) -> f32 {
                    self.#ident as f32 * #scale
                }

                pub fn #scaled_setter(&mut self, val: f32) -> ::core::result::Result<(), ::mbus_core::errors::MbusError> {
                    let raw = (val / #scale).round();
                    if !(0.0..=(u16::MAX as f32)).contains(&raw) {
                        return ::core::result::Result::Err(::mbus_core::errors::MbusError::InvalidValue);
                    }
                    self.#ident = raw as u16;
                    ::core::result::Result::Ok(())
                }
            }
        } else {
            quote! {}
        };

        let unit_method = if let Some(unit) = &f.unit {
            let unit_method = quote::format_ident!("{}_unit", &f.ident);
            quote! {
                pub fn #unit_method() -> &'static str {
                    #unit
                }
            }
        } else {
            quote! {}
        };

        quote! {
            pub fn #ident(&self) -> u16 { self.#ident }
            pub fn #setter(&mut self, val: u16) { self.#ident = val; }
            #scaled_methods
            #unit_method
        }
    });

    let encode_match_arms = sorted_fields.iter().map(|f| {
        let ident = &f.ident;
        let addr = f.addr;
        quote! { #addr => self.#ident, }
    });
    let write_match_arms = sorted_fields.iter().map(|f| {
        let ident = &f.ident;
        let addr = f.addr;
        quote! {
            #addr => {
                self.#ident = value;
                ::core::result::Result::Ok(())
            }
        }
    });

    Ok(quote! {
        impl #struct_name {
            #(#getters_setters)*
        }

        impl ::mbus_server::HoldingRegisterMap for #struct_name {
            const ADDR_MIN: u16 = #addr_min;
            const ADDR_MAX: u16 = #addr_max;
            const WORD_COUNT: usize = #word_count;
            const HAS_BATCH_NOTIFIED_FIELDS: bool = #has_batch_notified_fields;

            fn encode(
                &self,
                address: u16,
                quantity: u16,
                out: &mut [u8],
            ) -> ::core::result::Result<u8, ::mbus_core::errors::MbusError> {
                let qty = quantity as usize;
                if qty == 0 {
                    return ::core::result::Result::Err(
                        ::mbus_core::errors::MbusError::InvalidAddress,
                    );
                }
                let map_start = Self::ADDR_MIN as usize;
                let req_start = address as usize;
                let req_end   = req_start.checked_add(qty).ok_or(
                    ::mbus_core::errors::MbusError::InvalidAddress,
                )?;
                let map_end   = Self::ADDR_MAX as usize + 1;
                if req_start < map_start || req_end > map_end {
                    return ::core::result::Result::Err(
                        ::mbus_core::errors::MbusError::InvalidAddress,
                    );
                }
                let byte_len = qty * 2;
                if out.len() < byte_len {
                    return ::core::result::Result::Err(
                        ::mbus_core::errors::MbusError::BufferTooSmall,
                    );
                }
                for (i, chunk) in out[..byte_len].chunks_exact_mut(2).enumerate() {
                    let cur_addr = (req_start + i) as u16;
                    let word: u16 = match cur_addr {
                        #(#encode_match_arms)*
                        _ => {
                            return ::core::result::Result::Err(
                                ::mbus_core::errors::MbusError::InvalidAddress,
                            );
                        }
                    };
                    chunk.copy_from_slice(&word.to_be_bytes());
                }
                ::core::result::Result::Ok(byte_len as u8)
            }

            fn write_single(
                &mut self,
                address: u16,
                value: u16,
            ) -> ::core::result::Result<(), ::mbus_core::errors::MbusError> {
                match address {
                    #(#write_match_arms,)*
                    _ => ::core::result::Result::Err(::mbus_core::errors::MbusError::InvalidAddress),
                }
            }

            fn write_many(
                &mut self,
                address: u16,
                values: &[u16],
            ) -> ::core::result::Result<(), ::mbus_core::errors::MbusError> {
                if values.is_empty() {
                    return ::core::result::Result::Err(::mbus_core::errors::MbusError::InvalidQuantity);
                }

                for (index, value) in values.iter().copied().enumerate() {
                    let cur_addr = address
                        .checked_add(index as u16)
                        .ok_or(::mbus_core::errors::MbusError::InvalidAddress)?;
                    self.write_single(cur_addr, value)?;
                }

                ::core::result::Result::Ok(())
            }

            fn is_batch_notified(addr: u16) -> bool {
                match addr {
                    #(#hr_batch_notify_arms)*
                    _ => false,
                }
            }
        }
    })
}

fn expand_input_registers(input: &DeriveInput) -> Result<proc_macro2::TokenStream, Error> {
    let struct_name = &input.ident;
    let fields = parse_reg_fields(input, "InputRegistersModel")?;
    let allow_gaps = parse_holding_registers_options(input)?;
    validate_duplicate_reg_addresses(&fields)?;
    if !allow_gaps {
        validate_contiguous_reg_addresses(&fields, input, "InputRegistersModel")?;
    }

    if fields.is_empty() {
        return Err(Error::new_spanned(
            input,
            "InputRegistersModel requires at least one #[reg(addr = N)] field",
        ));
    }

    let addr_min = fields.iter().map(|f| f.addr).min().unwrap();
    let addr_max = fields.iter().map(|f| f.addr).max().unwrap();
    let word_count = (addr_max as usize) - (addr_min as usize) + 1;

    let mut sorted_fields = fields.clone();
    sorted_fields.sort_by_key(|f| f.addr);

    let getters_setters = fields.iter().map(|f| {
        let ident = &f.ident;
        let setter = quote::format_ident!("set_{}", &f.ident);
        let scaled_getter = quote::format_ident!("{}_scaled", &f.ident);
        let scaled_setter = quote::format_ident!("set_{}_scaled", &f.ident);
        let scale = f.scale;
        let scaled_methods = if f.has_scale {
            quote! {
                pub fn #scaled_getter(&self) -> f32 {
                    self.#ident as f32 * #scale
                }

                pub fn #scaled_setter(&mut self, val: f32) -> ::core::result::Result<(), ::mbus_core::errors::MbusError> {
                    let raw = (val / #scale).round();
                    if !(0.0..=(u16::MAX as f32)).contains(&raw) {
                        return ::core::result::Result::Err(::mbus_core::errors::MbusError::InvalidValue);
                    }
                    self.#ident = raw as u16;
                    ::core::result::Result::Ok(())
                }
            }
        } else {
            quote! {}
        };

        let unit_method = if let Some(unit) = &f.unit {
            let unit_method = quote::format_ident!("{}_unit", &f.ident);
            quote! {
                pub fn #unit_method() -> &'static str {
                    #unit
                }
            }
        } else {
            quote! {}
        };

        quote! {
            pub fn #ident(&self) -> u16 { self.#ident }
            pub fn #setter(&mut self, val: u16) { self.#ident = val; }
            #scaled_methods
            #unit_method
        }
    });

    let encode_match_arms = sorted_fields.iter().map(|f| {
        let ident = &f.ident;
        let addr = f.addr;
        quote! { #addr => self.#ident, }
    });

    Ok(quote! {
        impl #struct_name {
            #(#getters_setters)*
        }

        impl ::mbus_server::InputRegisterMap for #struct_name {
            const ADDR_MIN: u16 = #addr_min;
            const ADDR_MAX: u16 = #addr_max;
            const WORD_COUNT: usize = #word_count;

            fn encode(
                &self,
                address: u16,
                quantity: u16,
                out: &mut [u8],
            ) -> ::core::result::Result<u8, ::mbus_core::errors::MbusError> {
                let qty = quantity as usize;
                if qty == 0 {
                    return ::core::result::Result::Err(
                        ::mbus_core::errors::MbusError::InvalidAddress,
                    );
                }
                let map_start = Self::ADDR_MIN as usize;
                let req_start = address as usize;
                let req_end   = req_start.checked_add(qty).ok_or(
                    ::mbus_core::errors::MbusError::InvalidAddress,
                )?;
                let map_end   = Self::ADDR_MAX as usize + 1;
                if req_start < map_start || req_end > map_end {
                    return ::core::result::Result::Err(
                        ::mbus_core::errors::MbusError::InvalidAddress,
                    );
                }
                let byte_len = qty * 2;
                if out.len() < byte_len {
                    return ::core::result::Result::Err(
                        ::mbus_core::errors::MbusError::BufferTooSmall,
                    );
                }
                for (i, chunk) in out[..byte_len].chunks_exact_mut(2).enumerate() {
                    let cur_addr = (req_start + i) as u16;
                    let word: u16 = match cur_addr {
                        #(#encode_match_arms)*
                        _ => {
                            return ::core::result::Result::Err(
                                ::mbus_core::errors::MbusError::InvalidAddress,
                            );
                        }
                    };
                    chunk.copy_from_slice(&word.to_be_bytes());
                }
                ::core::result::Result::Ok(byte_len as u8)
            }
        }
    })
}

fn parse_holding_registers_options(input: &DeriveInput) -> Result<bool, Error> {
    let mut allow_gaps = false;

    for attr in &input.attrs {
        if !attr.path().is_ident("reg") {
            continue;
        }

        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("allow_gaps") {
                allow_gaps = true;
                Ok(())
            } else {
                Err(meta.error(
                    "unsupported key in struct-level #[reg(...)]; supported key: `allow_gaps` (example: #[reg(allow_gaps)])",
                ))
            }
        })?;
    }

    Ok(allow_gaps)
}

fn build_available_take_tokens(
    field_ty: &syn::Type,
    trait_path: proc_macro2::TokenStream,
    remaining_expr: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    quote! {
        let map_max = <#field_ty as #trait_path>::ADDR_MAX;
        let available = map_max
            .checked_sub(cursor)
            .ok_or(::mbus_core::errors::MbusError::InvalidAddress)?
            .saturating_add(1);
        let take = if (#remaining_expr) < available {
            (#remaining_expr)
        } else {
            available
        };
    }
}

fn build_advance_cursor_tokens() -> proc_macro2::TokenStream {
    quote! {
        cursor = cursor
            .checked_add(take)
            .ok_or(::mbus_core::errors::MbusError::InvalidAddress)?;
    }
}

fn build_segmented_read_route(
    fields: &[(Ident, syn::Type)],
    trait_path: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let mut route = quote! {{}};
    for (field_ident, field_ty) in fields.iter().rev() {
        let inner = route;
        let availability =
            build_available_take_tokens(field_ty, trait_path.clone(), quote!(remaining));
        let advance_cursor = build_advance_cursor_tokens();
        route = quote! {
            if (<#field_ty as #trait_path>::ADDR_MIN..=<#field_ty as #trait_path>::ADDR_MAX)
                .contains(&cursor)
            {
                #availability
                let take_bytes = byte_width(take);
                let next_offset = write_offset
                    .checked_add(take_bytes)
                    .ok_or(::mbus_core::errors::MbusError::BufferTooSmall)?;

                <#field_ty as #trait_path>::encode(
                    &self.#field_ident,
                    cursor,
                    take,
                    &mut out[write_offset..next_offset],
                )?;

                #advance_cursor
                remaining -= take;
                write_offset = next_offset;
                advanced = true;
            } else {
                #inner
            }
        };
    }
    route
}

fn build_segmented_coil_read_route(fields: &[(Ident, syn::Type)]) -> proc_macro2::TokenStream {
    let mut route = quote! {{}};
    for (field_ident, field_ty) in fields.iter().rev() {
        let inner = route;
        let availability = build_available_take_tokens(
            field_ty,
            quote!(::mbus_server::CoilMap),
            quote!(remaining),
        );
        let advance_cursor = build_advance_cursor_tokens();
        route = quote! {
            if (<#field_ty as ::mbus_server::CoilMap>::ADDR_MIN..=<#field_ty as ::mbus_server::CoilMap>::ADDR_MAX)
                .contains(&cursor)
            {
                #availability
                let take_bytes = byte_width(take);
                let mut segment = [0u8; ::mbus_core::data_unit::common::MAX_PDU_DATA_LEN];
                let encoded = <#field_ty as ::mbus_server::CoilMap>::encode(
                    &self.#field_ident,
                    cursor,
                    take,
                    &mut segment[..take_bytes],
                )?;

                if encoded as usize != take_bytes {
                    return ::core::result::Result::Err(::mbus_core::errors::MbusError::InvalidByteCount);
                }

                for index in 0..take as usize {
                    let src_byte = index / 8;
                    let src_mask = 1u8 << (index % 8);
                    if segment[src_byte] & src_mask != 0 {
                        let dst_bit = bit_offset + index;
                        out[dst_bit / 8] |= 1u8 << (dst_bit % 8);
                    }
                }

                #advance_cursor
                remaining -= take;
                bit_offset += take as usize;
                advanced = true;
            } else {
                #inner
            }
        };
    }
    route
}

// ---------------------------------------------------------------------------
// Hook-aware write route builders
// ---------------------------------------------------------------------------

/// FC05 single-coil write with individual / batch-notify hook dispatch.
///
/// Policy (in order for each address):
/// 1. Individual `on_write_N` hook present — read old value, call hook, commit if approved.
/// 2. No individual hook but address has `notify_via_batch = true` AND `on_batch_write` present
///    — call batch hook with qty = 1 (same signature as FC0F), commit if approved.
/// 3. No hooks — write silently.
fn build_write_single_coil_route_with_hooks(
    fields: &[(Ident, syn::Type)],
    on_batch_write: Option<&Ident>,
    single_write_hooks: &[SingleWriteHook],
) -> proc_macro2::TokenStream {
    let mut route = quote! {{}};
    for (field_ident, field_ty) in fields.iter().rev() {
        let inner = route;

        let hook_arms: Vec<proc_macro2::TokenStream> = single_write_hooks
            .iter()
            .map(|h| {
                let addr = h.addr;
                let hook_fn = &h.hook;
                quote! {
                    #addr => {
                        let mut __old_buf = [0u8; 1];
                        let _ = <#field_ty as ::mbus_server::CoilMap>::encode(
                            &self.#field_ident, address, 1u16, &mut __old_buf,
                        );
                        let __old_val = (__old_buf[0] & 1u8) != 0u8;
                        self.#hook_fn(address, __old_val, value)?;
                        true
                    }
                }
            })
            .collect();

        let batch_notify_block = if let Some(batch_fn) = on_batch_write {
            quote! {
                if !__hook_dispatched {
                    if <#field_ty as ::mbus_server::CoilMap>::is_batch_notified(address) {
                        let __packed: u8 = if value { 1u8 } else { 0u8 };
                        self.#batch_fn(address, 1u16, &[__packed])?;
                    }
                }
            }
        } else {
            quote! {}
        };

        route = quote! {
            if (<#field_ty as ::mbus_server::CoilMap>::ADDR_MIN
                ..=<#field_ty as ::mbus_server::CoilMap>::ADDR_MAX)
                .contains(&address)
            {
                let __hook_dispatched: bool = match address {
                    #(#hook_arms)*
                    _ => false,
                };
                #batch_notify_block
                <#field_ty as ::mbus_server::CoilMap>::write_single(
                    &mut self.#field_ident, address, value,
                )?;
                wrote = true;
            } else {
                #inner
            }
        };
    }
    route
}

fn build_segmented_discrete_input_read_route(fields: &[(Ident, syn::Type)]) -> proc_macro2::TokenStream {
    let mut route = quote! {{}};
    for (field_ident, field_ty) in fields.iter().rev() {
        let inner = route;
        let availability = build_available_take_tokens(
            field_ty,
            quote!(::mbus_server::DiscreteInputMap),
            quote!(remaining),
        );
        let advance_cursor = build_advance_cursor_tokens();
        route = quote! {
            if (<#field_ty as ::mbus_server::DiscreteInputMap>::ADDR_MIN..=<#field_ty as ::mbus_server::DiscreteInputMap>::ADDR_MAX)
                .contains(&cursor)
            {
                #availability
                let take_bytes = byte_width(take);
                let mut segment = [0u8; ::mbus_core::data_unit::common::MAX_PDU_DATA_LEN];
                let encoded = <#field_ty as ::mbus_server::DiscreteInputMap>::encode(
                    &self.#field_ident,
                    cursor,
                    take,
                    &mut segment[..take_bytes],
                )?;

                if encoded as usize != take_bytes {
                    return ::core::result::Result::Err(::mbus_core::errors::MbusError::InvalidByteCount);
                }

                for index in 0..take as usize {
                    let src_byte = index / 8;
                    let src_mask = 1u8 << (index % 8);
                    if segment[src_byte] & src_mask != 0 {
                        let dst_bit = bit_offset + index;
                        out[dst_bit / 8] |= 1u8 << (dst_bit % 8);
                    }
                }

                #advance_cursor
                remaining -= take;
                bit_offset += take as usize;
                advanced = true;
            } else {
                #inner
            }
        };
    }
    route
}

fn build_write_single_register_route_with_hooks(
    fields: &[(Ident, syn::Type)],
    on_batch_write: Option<&Ident>,
    single_write_hooks: &[SingleWriteHook],
) -> proc_macro2::TokenStream {
    let mut route = quote! {{}};
    for (field_ident, field_ty) in fields.iter().rev() {
        let inner = route;

        let hook_arms: Vec<proc_macro2::TokenStream> = single_write_hooks
            .iter()
            .map(|h| {
                let addr = h.addr;
                let hook_fn = &h.hook;
                quote! {
                    #addr => {
                        let mut __old_buf = [0u8; 2];
                        let _ = <#field_ty as ::mbus_server::HoldingRegisterMap>::encode(
                            &self.#field_ident, address, 1u16, &mut __old_buf,
                        );
                        let __old_val = u16::from_be_bytes([__old_buf[0], __old_buf[1]]);
                        self.#hook_fn(address, __old_val, value)?;
                        true
                    }
                }
            })
            .collect();

        let batch_notify_block = if let Some(batch_fn) = on_batch_write {
            quote! {
                if !__hook_dispatched {
                    if <#field_ty as ::mbus_server::HoldingRegisterMap>::is_batch_notified(address) {
                        self.#batch_fn(address, 1u16, &[value])?;
                    }
                }
            }
        } else {
            quote! {}
        };

        route = quote! {
            if (<#field_ty as ::mbus_server::HoldingRegisterMap>::ADDR_MIN
                ..=<#field_ty as ::mbus_server::HoldingRegisterMap>::ADDR_MAX)
                .contains(&address)
            {
                let __hook_dispatched: bool = match address {
                    #(#hook_arms)*
                    _ => false,
                };
                #batch_notify_block
                <#field_ty as ::mbus_server::HoldingRegisterMap>::write_single(
                    &mut self.#field_ident, address, value,
                )?;
                wrote = true;
            } else {
                #inner
            }
        };
    }
    route
}

/// FC0F multiple-coils write with batch hook dispatch.
///
/// If `on_batch_write` is present, the hook is called with re-packed coil bits
/// for this map's address range before the write is committed.
fn build_write_many_coil_route_with_hooks(
    fields: &[(Ident, syn::Type)],
    on_batch_write: Option<&Ident>,
) -> proc_macro2::TokenStream {
    let mut route = quote! {{}};
    for (field_ident, field_ty) in fields.iter().rev() {
        let inner = route;
        let availability = build_available_take_tokens(
            field_ty,
            quote!(::mbus_server::CoilMap),
            quote!(remaining_bits),
        );
        let advance_cursor = build_advance_cursor_tokens();

        let hook_prelude = if let Some(batch_fn) = on_batch_write {
            quote! {
                {
                    let __packed_len = (take as usize).div_ceil(8);
                    let mut __packed = [0u8; ::mbus_core::data_unit::common::MAX_PDU_DATA_LEN];
                    for __bit_i in 0usize..(take as usize) {
                        let __src_pos = bit_offset + __bit_i;
                        let __src_byte = values[__src_pos / 8];
                        let __src_bit = (__src_byte >> (__src_pos % 8)) & 1u8;
                        __packed[__bit_i / 8] |= __src_bit << (__bit_i % 8);
                    }
                    self.#batch_fn(cursor, take, &__packed[..__packed_len])?;
                }
            }
        } else {
            quote! {}
        };

        route = quote! {
            if (<#field_ty as ::mbus_server::CoilMap>::ADDR_MIN
                ..=<#field_ty as ::mbus_server::CoilMap>::ADDR_MAX)
                .contains(&cursor)
            {
                #availability
                #hook_prelude
                <#field_ty as ::mbus_server::CoilMap>::write_many_from_packed(
                    &mut self.#field_ident,
                    cursor,
                    take,
                    values,
                    bit_offset,
                )?;
                #advance_cursor
                remaining_bits -= take;
                bit_offset += take as usize;
                advanced = true;
            } else {
                #inner
            }
        };
    }
    route
}

/// FC10 multiple-registers write with batch hook dispatch.
///
/// If `on_batch_write` is present, the hook receives the decoded u16 value slice for
/// this map's contribution before the write is committed.
fn build_write_many_register_route_with_hooks(
    fields: &[(Ident, syn::Type)],
    on_batch_write: Option<&Ident>,
) -> proc_macro2::TokenStream {
    let mut route = quote! {{}};
    for (field_ident, field_ty) in fields.iter().rev() {
        let inner = route;
        let availability = build_available_take_tokens(
            field_ty,
            quote!(::mbus_server::HoldingRegisterMap),
            quote!(remaining_values.len() as u16),
        );
        let advance_cursor = build_advance_cursor_tokens();

        let hook_prelude = if let Some(batch_fn) = on_batch_write {
            quote! {
                self.#batch_fn(cursor, take, &remaining_values[..split])?;
            }
        } else {
            quote! {}
        };

        route = quote! {
            if (<#field_ty as ::mbus_server::HoldingRegisterMap>::ADDR_MIN
                ..=<#field_ty as ::mbus_server::HoldingRegisterMap>::ADDR_MAX)
                .contains(&cursor)
            {
                #availability
                let split = take as usize;
                #hook_prelude
                <#field_ty as ::mbus_server::HoldingRegisterMap>::write_many(
                    &mut self.#field_ident,
                    cursor,
                    &remaining_values[..split],
                )?;
                #advance_cursor
                remaining_values = &remaining_values[split..];
                advanced = true;
            } else {
                #inner
            }
        };
    }
    route
}

fn build_overlap_checks(
    fields: &[(Ident, syn::Type)],
    trait_path: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let mut checks = Vec::new();
    for i in 0..fields.len() {
        for j in (i + 1)..fields.len() {
            let (_, a_ty) = &fields[i];
            let (_, b_ty) = &fields[j];
            checks.push(quote! {
                if !(<#a_ty as #trait_path>::ADDR_MAX <
                        <#b_ty as #trait_path>::ADDR_MIN ||
                     <#b_ty as #trait_path>::ADDR_MAX <
                        <#a_ty as #trait_path>::ADDR_MIN)
                {
                    panic!("overlapping modbus_app address ranges in same data domain");
                }
            });
        }
    }
    quote! { #(#checks)* }
}

fn build_order_checks(
    fields: &[(Ident, syn::Type)],
    trait_path: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let mut checks = Vec::new();
    for window in fields.windows(2) {
        let (_, prev_ty) = &window[0];
        let (_, next_ty) = &window[1];
        checks.push(quote! {
            if <#prev_ty as #trait_path>::ADDR_MIN > <#next_ty as #trait_path>::ADDR_MIN {
                panic!(
                    "modbus_app maps in the same data domain must be declared in ascending ADDR_MIN order; reorder maps in #[modbus_app(...)]",
                );
            }
        });
    }
    quote! { #(#checks)* }
}

// ---------------------------------------------------------------------------
// modbus_app attribute macro expansion
// ---------------------------------------------------------------------------

/// Re-emits the original struct (stripping only the `#[holding_registers]`
/// helper attributes) and generates a `ModbusAppHandler` impl directly on it.
/// The FC03 response buffer is stack-allocated inside `ServerServices::dispatch_request`
/// and passed in via the `out: &mut [u8]` parameter — no permanent per-struct RAM cost.
fn expand_modbus_app_struct(
    input: &ItemStruct,
    selected_fields: &SelectedAppFields,
) -> Result<proc_macro2::TokenStream, Error> {
    let vis = &input.vis;
    let struct_name = &input.ident;
    let generics = &input.generics;
    let where_clause = &generics.where_clause;

    let named_fields = match &input.fields {
        Fields::Named(n) => n,
        _ => {
            return Err(Error::new_spanned(
                &input.ident,
                "#[modbus_app] requires a struct with named fields",
            ));
        }
    };

    let collect_group_fields = |selected: &[Ident],
                                helper_attr: &str,
                                group_name: &str|
     -> Result<Vec<(Ident, syn::Type)>, Error> {
        let mut out = Vec::new();

        if !selected.is_empty() {
            for selected_ident in selected {
                let mut found: Option<(Ident, syn::Type)> = None;
                for field in &named_fields.named {
                    if field.ident.as_ref() == Some(selected_ident) {
                        let ident = field
                            .ident
                            .clone()
                            .ok_or_else(|| Error::new_spanned(field, "field missing ident"))?;
                        found = Some((ident, field.ty.clone()));
                        break;
                    }
                }

                if let Some(pair) = found {
                    out.push(pair);
                } else {
                    return Err(Error::new_spanned(
                        &input.ident,
                        format!(
                            "unknown field '{}' in #[modbus_app({}(...))]",
                            selected_ident, group_name
                        ),
                    ));
                }
            }
        } else {
            for field in &named_fields.named {
                if !field.attrs.iter().any(|a| a.path().is_ident(helper_attr)) {
                    continue;
                }
                let ident = field
                    .ident
                    .clone()
                    .ok_or_else(|| Error::new_spanned(field, "field missing ident"))?;
                out.push((ident, field.ty.clone()));
            }
        }

        Ok(out)
    };

    let hr_fields = collect_group_fields(
        &selected_fields.holding_registers.field_idents,
        "holding_registers",
        "holding_registers",
    )?;
    let ir_fields = collect_group_fields(
        &selected_fields.input_registers,
        "input_registers",
        "input_registers",
    )?;
    let coil_fields = collect_group_fields(&selected_fields.coils.field_idents, "coils", "coils")?;
    let discrete_input_fields = collect_group_fields(
        &selected_fields.discrete_inputs,
        "discrete_inputs",
        "discrete_inputs",
    )?;

    if hr_fields.is_empty() && ir_fields.is_empty() && coil_fields.is_empty() && discrete_input_fields.is_empty() {
        return Err(Error::new_spanned(
            &input.ident,
            "no modbus_app fields selected; use #[modbus_app(holding_registers(...), input_registers(...), coils(...), discrete_inputs(...))] or helper field attributes",
        ));
    }

    // Re-emit the original struct fields, stripping only #[holding_registers]
    // / #[input_registers] / #[coils] / #[discrete_inputs] helper attributes.
    let struct_attrs = &input.attrs;
    let clean_fields = named_fields.named.iter().map(|field| {
        let clean_attrs: Vec<_> = field
            .attrs
            .iter()
            .filter(|a| {
                !a.path().is_ident("holding_registers")
                    && !a.path().is_ident("input_registers")
                    && !a.path().is_ident("coils")
                    && !a.path().is_ident("discrete_inputs")
            })
            .collect();
        let fvis = &field.vis;
        let fident = &field.ident;
        let fty = &field.ty;
        quote! { #(#clean_attrs)* #fvis #fident: #fty }
    });

    let hr_read_route =
        build_segmented_read_route(&hr_fields, quote!(::mbus_server::HoldingRegisterMap));
    let ir_read_route =
        build_segmented_read_route(&ir_fields, quote!(::mbus_server::InputRegisterMap));
    let coil_read_route = build_segmented_coil_read_route(&coil_fields);
    let discrete_input_read_route = build_segmented_discrete_input_read_route(&discrete_input_fields);

    let hr_write_single_route = build_write_single_register_route_with_hooks(
        &hr_fields,
        selected_fields.holding_registers.on_batch_write.as_ref(),
        &selected_fields.holding_registers.single_write_hooks,
    );
    let coil_write_single_route = build_write_single_coil_route_with_hooks(
        &coil_fields,
        selected_fields.coils.on_batch_write.as_ref(),
        &selected_fields.coils.single_write_hooks,
    );

    let hr_write_many_route = build_write_many_register_route_with_hooks(
        &hr_fields,
        selected_fields.holding_registers.on_batch_write.as_ref(),
    );
    let coil_write_many_route = build_write_many_coil_route_with_hooks(
        &coil_fields,
        selected_fields.coils.on_batch_write.as_ref(),
    );

    let hr_overlap_checks =
        build_overlap_checks(&hr_fields, quote!(::mbus_server::HoldingRegisterMap));
    let ir_overlap_checks =
        build_overlap_checks(&ir_fields, quote!(::mbus_server::InputRegisterMap));
    let coil_overlap_checks = build_overlap_checks(&coil_fields, quote!(::mbus_server::CoilMap));
    let discrete_input_overlap_checks =
        build_overlap_checks(&discrete_input_fields, quote!(::mbus_server::DiscreteInputMap));

    let hr_order_checks = build_order_checks(&hr_fields, quote!(::mbus_server::HoldingRegisterMap));
    let ir_order_checks = build_order_checks(&ir_fields, quote!(::mbus_server::InputRegisterMap));
    let coil_order_checks = build_order_checks(&coil_fields, quote!(::mbus_server::CoilMap));
    let discrete_input_order_checks = build_order_checks(&discrete_input_fields, quote!(::mbus_server::DiscreteInputMap));

    let hr_notify_requires_batch_checks = if selected_fields
        .holding_registers
        .on_batch_write
        .is_none()
    {
        let checks = hr_fields.iter().map(|(_, field_ty)| {
            quote! {
                if <#field_ty as ::mbus_server::HoldingRegisterMap>::HAS_BATCH_NOTIFIED_FIELDS {
                    panic!(
                        "notify_via_batch is set on a holding_registers map field, but #[modbus_app(holding_registers(...))] has no on_batch_write hook",
                    );
                }
            }
        });
        quote! { #(#checks)* }
    } else {
        quote! {}
    };

    let hr_on_write_address_checks = {
        let checks = selected_fields
            .holding_registers
            .single_write_hooks
            .iter()
            .map(|hook| {
                let addr = hook.addr;
                let addr_text = syn::LitStr::new(&hook.addr_text, proc_macro2::Span::call_site());
                let hook_ident = &hook.hook;
                let in_any_map = hr_fields.iter().map(|(_, field_ty)| {
                    quote! {
                        (#addr >= <#field_ty as ::mbus_server::HoldingRegisterMap>::ADDR_MIN
                            && #addr <= <#field_ty as ::mbus_server::HoldingRegisterMap>::ADDR_MAX)
                    }
                });
                quote! {
                    if !(false #(|| #in_any_map)*) {
                        panic!(
                            concat!(
                                "invalid #[modbus_app(holding_registers(...))] hook: ",
                                "on_write_",
                                #addr_text,
                                " = ",
                                stringify!(#hook_ident),
                                " targets an unmapped holding-register address",
                            )
                        );
                    }
                }
            });
        quote! { #(#checks)* }
    };

    let hr_hook_signature_checks = {
        let mut checks: Vec<proc_macro2::TokenStream> = Vec::new();

        if let Some(batch_hook) = selected_fields.holding_registers.on_batch_write.as_ref() {
            checks.push(quote! {
                let _sig_check: fn(
                    &mut Self,
                    u16,
                    u16,
                    &[u16],
                ) -> ::core::result::Result<(), ::mbus_core::errors::MbusError> = Self::#batch_hook;
            });
        }

        for hook in &selected_fields.holding_registers.single_write_hooks {
            let hook_ident = &hook.hook;
            checks.push(quote! {
                let _sig_check: fn(
                    &mut Self,
                    u16,
                    u16,
                    u16,
                ) -> ::core::result::Result<(), ::mbus_core::errors::MbusError> = Self::#hook_ident;
            });
        }

        quote! { #(#checks)* }
    };

    let coil_notify_requires_batch_checks = if selected_fields.coils.on_batch_write.is_none() {
        let checks = coil_fields.iter().map(|(_, field_ty)| {
            quote! {
                if <#field_ty as ::mbus_server::CoilMap>::HAS_BATCH_NOTIFIED_FIELDS {
                    panic!(
                        "notify_via_batch is set on a coils map field, but #[modbus_app(coils(...))] has no on_batch_write hook",
                    );
                }
            }
        });
        quote! { #(#checks)* }
    } else {
        quote! {}
    };

    let coil_on_write_address_checks = {
        let checks = selected_fields.coils.single_write_hooks.iter().map(|hook| {
            let addr = hook.addr;
            let addr_text = syn::LitStr::new(&hook.addr_text, proc_macro2::Span::call_site());
            let hook_ident = &hook.hook;
            let in_any_map = coil_fields.iter().map(|(_, field_ty)| {
                quote! {
                    (#addr >= <#field_ty as ::mbus_server::CoilMap>::ADDR_MIN
                        && #addr <= <#field_ty as ::mbus_server::CoilMap>::ADDR_MAX)
                }
            });
            quote! {
                if !(false #(|| #in_any_map)*) {
                    panic!(
                        concat!(
                            "invalid #[modbus_app(coils(...))] hook: ",
                            "on_write_",
                            #addr_text,
                            " = ",
                            stringify!(#hook_ident),
                            " targets an unmapped coil address",
                        )
                    );
                }
            }
        });
        quote! { #(#checks)* }
    };

    let coil_hook_signature_checks = {
        let mut checks: Vec<proc_macro2::TokenStream> = Vec::new();

        if let Some(batch_hook) = selected_fields.coils.on_batch_write.as_ref() {
            checks.push(quote! {
                let _sig_check: fn(
                    &mut Self,
                    u16,
                    u16,
                    &[u8],
                ) -> ::core::result::Result<(), ::mbus_core::errors::MbusError> = Self::#batch_hook;
            });
        }

        for hook in &selected_fields.coils.single_write_hooks {
            let hook_ident = &hook.hook;
            checks.push(quote! {
                let _sig_check: fn(
                    &mut Self,
                    u16,
                    bool,
                    bool,
                ) -> ::core::result::Result<(), ::mbus_core::errors::MbusError> = Self::#hook_ident;
            });
        }

        quote! { #(#checks)* }
    };

    let force_layout_check = if generics.params.is_empty() {
        quote! {
            const _: () = <#struct_name>::_MBUS_HOLDING_MAP_LAYOUT_VALIDATION;
        }
    } else {
        quote! {}
    };

    Ok(quote! {
        // Re-emit the original struct (all attrs preserved, #[holding_registers]
        // stripped from field-level attrs to avoid "unknown attribute" errors).
        #(#struct_attrs)*
        #vis struct #struct_name #generics #where_clause {
            #(#clean_fields,)*
        }

        impl #generics #struct_name #generics #where_clause {
            const _MBUS_HOLDING_MAP_LAYOUT_VALIDATION: () = {
                #hr_overlap_checks
                #ir_overlap_checks
                #coil_overlap_checks
                #discrete_input_overlap_checks
                #hr_order_checks
                #ir_order_checks
                #coil_order_checks
                #discrete_input_order_checks
                #hr_notify_requires_batch_checks
                #coil_notify_requires_batch_checks
                #hr_on_write_address_checks
                #coil_on_write_address_checks
                #hr_hook_signature_checks
                #coil_hook_signature_checks
            };
        }

        #force_layout_check

        // -----------------------------------------------------------------------
        // ModbusAppHandler impl directly on the application struct.
        // The response buffer is stack-allocated by ServerServices::dispatch_request;
        // no permanent per-struct RAM is consumed.
        // -----------------------------------------------------------------------

        impl #generics ::mbus_server::app::ModbusAppHandler for #struct_name #generics #where_clause {
            #[cfg(feature = "coils")]
            fn read_coils_request(
                &mut self,
                txn_id: u16,
                unit_id_or_slave_addr: ::mbus_core::transport::UnitIdOrSlaveAddr,
                address: u16,
                quantity: u16,
                out: &mut [u8],
            ) -> ::core::result::Result<u8, ::mbus_core::errors::MbusError> {
                let _ = (txn_id, unit_id_or_slave_addr);
                let result: ::core::result::Result<u8, ::mbus_core::errors::MbusError> = (|| {
                    if quantity == 0 {
                        return ::core::result::Result::Err(::mbus_core::errors::MbusError::InvalidQuantity);
                    }

                    let byte_width = |count: u16| -> usize { (count as usize).div_ceil(8) };
                    let total_bytes = byte_width(quantity);
                    if out.len() < total_bytes {
                        return ::core::result::Result::Err(::mbus_core::errors::MbusError::BufferTooSmall);
                    }
                    out[..total_bytes].fill(0);

                    let mut cursor = address;
                    let mut remaining = quantity;
                    let mut bit_offset = 0usize;
                    while remaining > 0 {
                        let mut advanced = false;
                        #coil_read_route
                        if !advanced {
                            return ::core::result::Result::Err(::mbus_core::errors::MbusError::InvalidAddress);
                        }
                    }

                    Ok(total_bytes as u8)
                })();

                result
            }

            #[cfg(feature = "discrete-inputs")]
            fn read_discrete_inputs_request(
                &mut self,
                txn_id: u16,
                unit_id_or_slave_addr: ::mbus_core::transport::UnitIdOrSlaveAddr,
                address: u16,
                quantity: u16,
                out: &mut [u8],
            ) -> ::core::result::Result<u8, ::mbus_core::errors::MbusError> {
                let _ = (txn_id, unit_id_or_slave_addr);
                let result: ::core::result::Result<u8, ::mbus_core::errors::MbusError> = (|| {
                    if quantity == 0 {
                        return ::core::result::Result::Err(::mbus_core::errors::MbusError::InvalidQuantity);
                    }

                    let byte_width = |count: u16| -> usize { (count as usize).div_ceil(8) };
                    let total_bytes = byte_width(quantity);
                    if out.len() < total_bytes {
                        return ::core::result::Result::Err(::mbus_core::errors::MbusError::BufferTooSmall);
                    }
                    out[..total_bytes].fill(0);

                    let mut cursor = address;
                    let mut remaining = quantity;
                    let mut bit_offset = 0usize;
                    while remaining > 0 {
                        let mut advanced = false;
                        #discrete_input_read_route
                        if !advanced {
                            return ::core::result::Result::Err(::mbus_core::errors::MbusError::InvalidAddress);
                        }
                    }

                    Ok(total_bytes as u8)
                })();

                result
            }

            #[cfg(feature = "holding-registers")]
            fn read_multiple_holding_registers_request(
                &mut self,
                txn_id: u16,
                unit_id_or_slave_addr: ::mbus_core::transport::UnitIdOrSlaveAddr,
                address: u16,
                quantity: u16,
                out: &mut [u8],
            ) -> ::core::result::Result<u8, ::mbus_core::errors::MbusError> {
                let _ = (txn_id, unit_id_or_slave_addr);
                let result: ::core::result::Result<u8, ::mbus_core::errors::MbusError> = (|| {
                    if quantity == 0 {
                        return ::core::result::Result::Err(::mbus_core::errors::MbusError::InvalidQuantity);
                    }

                    let byte_width = |count: u16| -> usize { (count as usize) * 2 };
                    let total_bytes = byte_width(quantity);
                    if out.len() < total_bytes {
                        return ::core::result::Result::Err(::mbus_core::errors::MbusError::BufferTooSmall);
                    }

                    let mut cursor = address;
                    let mut remaining = quantity;
                    let mut write_offset = 0usize;

                    while remaining > 0 {
                        let mut advanced = false;
                        #hr_read_route

                        if !advanced {
                            return ::core::result::Result::Err(::mbus_core::errors::MbusError::InvalidAddress);
                        }
                    }

                    Ok(total_bytes as u8)
                })();

                result
            }

            #[cfg(feature = "input-registers")]
            fn read_multiple_input_registers_request(
                &mut self,
                txn_id: u16,
                unit_id_or_slave_addr: ::mbus_core::transport::UnitIdOrSlaveAddr,
                address: u16,
                quantity: u16,
                out: &mut [u8],
            ) -> ::core::result::Result<u8, ::mbus_core::errors::MbusError> {
                let _ = (txn_id, unit_id_or_slave_addr);
                let result: ::core::result::Result<u8, ::mbus_core::errors::MbusError> = (|| {
                    if quantity == 0 {
                        return ::core::result::Result::Err(::mbus_core::errors::MbusError::InvalidQuantity);
                    }

                    let byte_width = |count: u16| -> usize { (count as usize) * 2 };
                    let total_bytes = byte_width(quantity);
                    if out.len() < total_bytes {
                        return ::core::result::Result::Err(::mbus_core::errors::MbusError::BufferTooSmall);
                    }

                    let mut cursor = address;
                    let mut remaining = quantity;
                    let mut write_offset = 0usize;
                    while remaining > 0 {
                        let mut advanced = false;
                        #ir_read_route
                        if !advanced {
                            return ::core::result::Result::Err(::mbus_core::errors::MbusError::InvalidAddress);
                        }
                    }

                    Ok(total_bytes as u8)
                })();

                result
            }

            #[cfg(feature = "coils")]
            fn write_single_coil_request(
                &mut self,
                txn_id: u16,
                unit_id_or_slave_addr: ::mbus_core::transport::UnitIdOrSlaveAddr,
                address: u16,
                value: bool,
            ) -> ::core::result::Result<(), ::mbus_core::errors::MbusError> {
                let _ = (txn_id, unit_id_or_slave_addr);
                let result: ::core::result::Result<(), ::mbus_core::errors::MbusError> = (|| {
                    let mut wrote = false;
                    #coil_write_single_route
                    if !wrote {
                        return ::core::result::Result::Err(::mbus_core::errors::MbusError::InvalidAddress);
                    }
                    Ok(())
                })();

                result
            }

            #[cfg(feature = "holding-registers")]
            fn write_single_register_request(
                &mut self,
                txn_id: u16,
                unit_id_or_slave_addr: ::mbus_core::transport::UnitIdOrSlaveAddr,
                address: u16,
                value: u16,
            ) -> ::core::result::Result<(), ::mbus_core::errors::MbusError> {
                let _ = (txn_id, unit_id_or_slave_addr);
                let result: ::core::result::Result<(), ::mbus_core::errors::MbusError> = (|| {
                    let mut wrote = false;
                    #hr_write_single_route
                    if !wrote {
                        return ::core::result::Result::Err(::mbus_core::errors::MbusError::InvalidAddress);
                    }
                    Ok(())
                })();

                result
            }

            #[cfg(feature = "coils")]
            fn write_multiple_coils_request(
                &mut self,
                txn_id: u16,
                unit_id_or_slave_addr: ::mbus_core::transport::UnitIdOrSlaveAddr,
                starting_address: u16,
                quantity: u16,
                values: &[u8],
            ) -> ::core::result::Result<(), ::mbus_core::errors::MbusError> {
                let _ = (txn_id, unit_id_or_slave_addr);
                let result: ::core::result::Result<(), ::mbus_core::errors::MbusError> = (|| {
                    if quantity == 0 {
                        return ::core::result::Result::Err(::mbus_core::errors::MbusError::InvalidQuantity);
                    }
                    let mut cursor = starting_address;
                    let mut remaining_bits = quantity;
                    let mut bit_offset = 0usize;
                    while remaining_bits > 0 {
                        let mut advanced = false;
                        #coil_write_many_route
                        if !advanced {
                            return ::core::result::Result::Err(::mbus_core::errors::MbusError::InvalidAddress);
                        }
                    }
                    Ok(())
                })();

                result
            }

            #[cfg(feature = "holding-registers")]
            fn write_multiple_registers_request(
                &mut self,
                txn_id: u16,
                unit_id_or_slave_addr: ::mbus_core::transport::UnitIdOrSlaveAddr,
                starting_address: u16,
                values: &[u16],
            ) -> ::core::result::Result<(), ::mbus_core::errors::MbusError> {
                let _ = (txn_id, unit_id_or_slave_addr);
                let result: ::core::result::Result<(), ::mbus_core::errors::MbusError> = (|| {
                    if values.is_empty() {
                        return ::core::result::Result::Err(::mbus_core::errors::MbusError::InvalidQuantity);
                    }

                    let mut cursor = starting_address;
                    let mut remaining_values = values;
                    while !remaining_values.is_empty() {
                        let mut advanced = false;
                        #hr_write_many_route
                        if !advanced {
                            return ::core::result::Result::Err(::mbus_core::errors::MbusError::InvalidAddress);
                        }
                    }
                    Ok(())
                })();

                result
            }
        }
    })
}

// ---------------------------------------------------------------------------
// HoldingRegisters field parser
// ---------------------------------------------------------------------------

fn parse_reg_fields(input: &DeriveInput, model_name: &str) -> Result<Vec<RegField>, Error> {
    let data = match &input.data {
        Data::Struct(d) => d,
        _ => {
            return Err(Error::new_spanned(
                input,
                format!(
                    "{} can only be derived for structs; use `struct MyRegs {{ ... }}` with named fields",
                    model_name
                ),
            ));
        }
    };

    let named = match &data.fields {
        Fields::Named(n) => n,
        _ => {
            return Err(Error::new_spanned(
                input,
                format!(
                    "{} requires named fields; tuple/unit structs are not supported",
                    model_name
                ),
            ));
        }
    };

    let mut out = Vec::new();
    for field in &named.named {
        let ident = field.ident.clone().ok_or_else(|| {
            Error::new_spanned(
                field,
                format!(
                    "field identifier missing; {} expects named fields",
                    model_name
                ),
            )
        })?;

        // All fields must be u16.
        let ty_ok = match &field.ty {
            syn::Type::Path(p) => p
                .path
                .segments
                .last()
                .map(|s| s.ident == "u16")
                .unwrap_or(false),
            _ => false,
        };
        if !ty_ok {
            return Err(Error::new_spanned(
                &field.ty,
                format!(
                    "{} fields must be u16 (wire-ready register word); convert this field to u16",
                    model_name
                ),
            ));
        }

        // Parse #[reg(addr = N, scale = F?, unit = "...")].
        let mut addr: Option<u16> = None;
        let mut scale: f32 = 1.0;
        let mut has_scale = false;
        let mut unit: Option<String> = None;
        let mut notify_via_batch = false;
        for attr in &field.attrs {
            if !attr.path().is_ident("reg") {
                continue;
            }
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("addr") {
                    let value: LitInt = meta.value()?.parse()?;
                    addr = Some(parse_u16(&value)?);
                    Ok(())
                } else if meta.path.is_ident("scale") {
                    has_scale = true;
                    let value: Lit = meta.value()?.parse()?;
                    match value {
                        Lit::Float(v) => {
                            scale = parse_f32(&v)?;
                            Ok(())
                        }
                        Lit::Int(v) => {
                            scale = parse_f32_from_int(&v)?;
                            Ok(())
                        }
                        _ => Err(meta.error(
                            "scale must be a numeric literal; examples: scale = 1 or scale = 0.1",
                        )),
                    }
                } else if meta.path.is_ident("unit") {
                    let value: LitStr = meta.value()?.parse()?;
                    unit = Some(value.value());
                    Ok(())
                } else if meta.path.is_ident("notify_via_batch") {
                    let value: syn::LitBool = meta.value()?.parse()?;
                    notify_via_batch = value.value;
                    Ok(())
                } else {
                    Err(meta.error(
                        "unsupported key in #[reg(...)]; supported keys: `addr`, `scale`, `unit`, `notify_via_batch` (example: #[reg(addr = 10, scale = 0.1, unit = \"C\")])",
                    ))
                }
            })?;
        }

        if scale <= 0.0 {
            return Err(Error::new_spanned(
                field,
                "reg scale must be > 0; example: #[reg(addr = 0, scale = 0.1)]",
            ));
        }

        let addr = addr.ok_or_else(|| {
            Error::new_spanned(
                field,
                format!(
                    "missing #[reg(addr = N)] on {} field; example: #[reg(addr = 0)]",
                    model_name
                ),
            )
        })?;

        out.push(RegField {
            ident,
            addr,
            scale,
            has_scale,
            unit,
            notify_via_batch,
        });
    }

    Ok(out)
}

fn validate_duplicate_reg_addresses(fields: &[RegField]) -> Result<(), Error> {
    for (i, a) in fields.iter().enumerate() {
        for b in fields.iter().skip(i + 1) {
            if a.addr == b.addr {
                return Err(Error::new(
                    proc_macro2::Span::call_site(),
                    format!(
                        "duplicate register address {} for fields '{}' and '{}'; each register address must be unique",
                        a.addr, a.ident, b.ident
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn validate_contiguous_reg_addresses(
    fields: &[RegField],
    input: &DeriveInput,
    model_name: &str,
) -> Result<(), Error> {
    if fields.len() <= 1 {
        return Ok(());
    }

    let mut addrs: Vec<u16> = fields.iter().map(|f| f.addr).collect();
    addrs.sort_unstable();

    for window in addrs.windows(2) {
        let current = window[0];
        let next = window[1];
        if next != current + 1 {
            return Err(Error::new_spanned(
                input,
                format!(
                    "non-contiguous register addresses in {}: gap between {} and {}. Fix by making addresses contiguous (e.g. {} then {}), or add #[reg(allow_gaps)] on the struct to allow sparse maps",
                    model_name,
                    current,
                    next,
                    current,
                    current + 1
                ),
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    #[test]
    fn coils_duplicate_address_rejected() {
        let input: DeriveInput = parse_quote! {
            struct Model {
                #[coil(addr = 0)]
                a: bool,
                #[coil(addr = 0)]
                b: bool,
            }
        };

        let err = expand_coils_model(&input).unwrap_err().to_string();
        assert!(err.contains("duplicate coil address"));
    }

    #[test]
    fn discrete_inputs_duplicate_address_rejected() {
        let input: DeriveInput = parse_quote! {
            struct Model {
                #[discrete_input(addr = 0)]
                a: bool,
                #[discrete_input(addr = 0)]
                b: bool,
            }
        };

        let err = expand_discrete_inputs_model(&input).unwrap_err().to_string();
        assert!(err.contains("duplicate discrete input address"));
    }

    #[test]
    fn discrete_inputs_expand_read_only_map_trait() {
        let input: DeriveInput = parse_quote! {
            struct Model {
                #[discrete_input(addr = 0)]
                a: bool,
                #[discrete_input(addr = 1)]
                b: bool,
            }
        };

        let tokens = expand_discrete_inputs_model(&input).unwrap().to_string();
        assert!(tokens.contains("impl :: mbus_server :: DiscreteInputMap"));
        assert!(!tokens.contains("write_single"));
        assert!(!tokens.contains("write_many"));
    }

    #[test]
    fn holding_registers_duplicate_address_rejected() {
        let input: DeriveInput = parse_quote! {
            struct Model {
                #[reg(addr = 10)]
                a: u16,
                #[reg(addr = 10)]
                b: u16,
            }
        };

        let err = expand_holding_registers(&input).unwrap_err().to_string();
        assert!(err.contains("duplicate register address"));
    }

    #[test]
    fn holding_registers_expands_scaled_and_unit_helpers() {
        let input: DeriveInput = parse_quote! {
            struct Model {
                #[reg(addr = 0, scale = 0.1, unit = "C")]
                temp: u16,
                #[reg(addr = 1)]
                mode: u16,
            }
        };

        let tokens = expand_holding_registers(&input).unwrap().to_string();
        assert!(tokens.contains("impl Model"));
        assert!(tokens.contains("temp_scaled"));
        assert!(tokens.contains("set_temp_scaled"));
        assert!(tokens.contains("temp_unit"));
        assert!(tokens.contains("impl :: mbus_server :: HoldingRegisterMap"));
    }

    #[test]
    fn holding_registers_non_u16_field_rejected() {
        let input: DeriveInput = parse_quote! {
            struct Model {
                #[reg(addr = 0)]
                a: f32,
            }
        };

        let err = expand_holding_registers(&input).unwrap_err().to_string();
        assert!(err.contains("HoldingRegistersModel fields must be u16"));
    }

    #[test]
    fn holding_registers_non_positive_scale_rejected() {
        let input: DeriveInput = parse_quote! {
            struct Model {
                #[reg(addr = 0, scale = 0.0)]
                a: u16,
            }
        };

        let err = expand_holding_registers(&input).unwrap_err().to_string();
        assert!(err.contains("reg scale must be > 0"));
    }

    #[test]
    fn holding_registers_gaps_rejected_by_default() {
        let input: DeriveInput = parse_quote! {
            struct Model {
                #[reg(addr = 0)]
                a: u16,
                #[reg(addr = 5)]
                b: u16,
            }
        };

        let err = expand_holding_registers(&input).unwrap_err().to_string();
        assert!(err.contains("non-contiguous register addresses"));
        assert!(err.contains("allow_gaps"));
    }

    #[test]
    fn holding_registers_gaps_allowed_with_struct_option() {
        let input: DeriveInput = parse_quote! {
            #[reg(allow_gaps)]
            struct Model {
                #[reg(addr = 0)]
                a: u16,
                #[reg(addr = 5)]
                b: u16,
            }
        };

        let tokens = expand_holding_registers(&input).unwrap().to_string();
        assert!(tokens.contains("impl :: mbus_server :: HoldingRegisterMap"));
    }

    #[test]
    fn input_registers_expand_read_only_map_trait() {
        let input: DeriveInput = parse_quote! {
            struct Model {
                #[reg(addr = 0)]
                a: u16,
                #[reg(addr = 1)]
                b: u16,
            }
        };

        let tokens = expand_input_registers(&input).unwrap().to_string();
        assert!(tokens.contains("impl :: mbus_server :: InputRegisterMap"));
        assert!(!tokens.contains("fn write_single"));
        assert!(!tokens.contains("fn write_many"));
    }
}
