#![allow(dead_code)]
//! Option parsing and JS type extraction helpers for the WASM client.

use js_sys::{Array, Reflect, Uint16Array};
use wasm_bindgen::prelude::*;

pub(crate) fn get_u32(obj: &JsValue, key: &str, default: u32) -> u32 {
    if obj.is_null() || obj.is_undefined() {
        return default;
    }
    Reflect::get(obj, &JsValue::from_str(key))
        .ok()
        .and_then(|v| {
            if v.is_null() || v.is_undefined() {
                None
            } else {
                v.as_f64().map(|f| f as u32)
            }
        })
        .unwrap_or(default)
}

pub(crate) fn get_u16(obj: &JsValue, key: &str, default: u16) -> u16 {
    get_u32(obj, key, default as u32) as u16
}

pub(crate) fn get_u8(obj: &JsValue, key: &str, default: u8) -> u8 {
    get_u32(obj, key, default as u32) as u8
}

pub(crate) fn coil_state_to_bool(val: &JsValue) -> bool {
    if let Some(n) = val.as_f64() {
        return n != 0.0;
    }
    val.as_bool().unwrap_or(false)
}

pub(crate) fn get_bool(obj: &JsValue, key: &str, default: bool) -> bool {
    if obj.is_null() || obj.is_undefined() {
        return default;
    }
    Reflect::get(obj, &JsValue::from_str(key))
        .ok()
        .and_then(|v| {
            if v.is_null() || v.is_undefined() {
                None
            } else {
                Some(coil_state_to_bool(&v))
            }
        })
        .unwrap_or(default)
}

pub(crate) fn get_string(obj: &JsValue, key: &str, default: &str) -> String {
    if obj.is_null() || obj.is_undefined() {
        return default.to_string();
    }
    Reflect::get(obj, &JsValue::from_str(key))
        .ok()
        .and_then(|v| {
            if v.is_null() || v.is_undefined() {
                None
            } else {
                v.as_string()
            }
        })
        .unwrap_or_else(|| default.to_string())
}

/// Parse a boolean array from options. Can be a JS array of booleans or numbers (0/1).
pub(crate) fn get_bool_array(obj: &JsValue, key: &str) -> Result<Vec<bool>, String> {
    let val = Reflect::get(obj, &JsValue::from_str(key))
        .map_err(|_| format!("Missing property '{}'", key))?;
    if val.is_null() || val.is_undefined() {
        return Err(format!("Property '{}' is null or undefined", key));
    }
    if !Array::is_array(&val) {
        return Err(format!("Property '{}' must be an array", key));
    }
    let arr = Array::from(&val);
    let len = arr.length() as usize;
    let mut vec = Vec::with_capacity(len);
    for i in 0..len {
        let item = arr.get(i as u32);
        vec.push(coil_state_to_bool(&item));
    }
    Ok(vec)
}

/// Parse a u16 array from options. Must be a Uint16Array.
pub(crate) fn get_u16_array(obj: &JsValue, key: &str) -> Result<Vec<u16>, String> {
    let val = Reflect::get(obj, &JsValue::from_str(key))
        .map_err(|_| format!("Missing property '{}'", key))?;
    if val.is_null() || val.is_undefined() {
        return Err(format!("Property '{}' is null or undefined", key));
    }

    if let Ok(u16_arr) = val.clone().dyn_into::<Uint16Array>() {
        return Ok(u16_arr.to_vec());
    }

    Err(format!("Property '{}' must be a Uint16Array", key))
}
