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
                v.as_bool()
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

/// Parse a boolean array from options. Can be a JS array of booleans.
pub(crate) fn get_bool_array(obj: &JsValue, key: &str) -> Result<Vec<bool>, String> {
    let val = Reflect::get(obj, &JsValue::from_str(key))
        .map_err(|_| format!("Missing property '{}'", key))?;
    if val.is_null() || val.is_undefined() {
        return Err(format!("Property '{}' is null or undefined", key));
    }
    if !Array::is_array(&val) {
        return Err(format!("Property '{}' must be an array of booleans", key));
    }
    let arr = Array::from(&val);
    let len = arr.length() as usize;
    let mut vec = Vec::with_capacity(len);
    for i in 0..len {
        let item = arr.get(i as u32);
        vec.push(item.as_bool().unwrap_or(false));
    }
    Ok(vec)
}

/// Parse a u16 array from options. Can be a JS array of numbers or a Uint16Array.
pub(crate) fn get_u16_array(obj: &JsValue, key: &str) -> Result<Vec<u16>, String> {
    let val = Reflect::get(obj, &JsValue::from_str(key))
        .map_err(|_| format!("Missing property '{}'", key))?;
    if val.is_null() || val.is_undefined() {
        return Err(format!("Property '{}' is null or undefined", key));
    }

    if let Ok(u16_arr) = val.clone().dyn_into::<Uint16Array>() {
        return Ok(u16_arr.to_vec());
    }

    if Array::is_array(&val) {
        let arr = Array::from(&val);
        let len = arr.length() as usize;
        let mut vec = Vec::with_capacity(len);
        for i in 0..len {
            let item = arr.get(i as u32);
            vec.push(item.as_f64().map(|f| f as u16).unwrap_or(0));
        }
        return Ok(vec);
    }

    Err(format!(
        "Property '{}' must be a Uint16Array or an array of numbers",
        key
    ))
}
