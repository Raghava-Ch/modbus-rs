//! Response enum representing results returned from WasmClientTask to WasmModbusClient.

use js_sys::{Array, Object, Reflect, Uint16Array};
use wasm_bindgen::JsValue;

pub(crate) enum WasmResponse {
    Void,
    BoolArray(Vec<bool>),
    U16Array(Vec<u16>),
    U8(u8),
    #[cfg(feature = "file-record")]
    FileRecord(Vec<Vec<u16>>),
    #[cfg(feature = "diagnostics")]
    Diagnostics {
        sub_function: u16,
        data: Vec<u16>,
    },
    #[cfg(feature = "diagnostics")]
    DeviceIdentification {
        read_device_id_code: u8,
        conformity_level: u8,
        more_follows: bool,
        objects: Vec<(u8, String)>,
    },
}

impl WasmResponse {
    pub fn to_js_value(self) -> JsValue {
        match self {
            WasmResponse::Void => JsValue::UNDEFINED,
            WasmResponse::BoolArray(vec) => {
                let arr = Array::new();
                for &b in &vec {
                    arr.push(&JsValue::from_bool(b));
                }
                arr.into()
            }
            WasmResponse::U16Array(vec) => {
                let arr = Uint16Array::from(vec.as_slice());
                arr.into()
            }
            WasmResponse::U8(val) => JsValue::from_f64(val as f64),
            #[cfg(feature = "file-record")]
            WasmResponse::FileRecord(records) => {
                let arr = Array::new();
                for rec in records {
                    let js_data = Uint16Array::from(rec.as_slice());
                    arr.push(&js_data.into());
                }
                arr.into()
            }
            #[cfg(feature = "diagnostics")]
            WasmResponse::Diagnostics { sub_function, data } => {
                let obj = Object::new();
                let _ = Reflect::set(
                    &obj,
                    &JsValue::from_str("subFunction"),
                    &JsValue::from_f64(sub_function as f64),
                );
                let _ = Reflect::set(
                    &obj,
                    &JsValue::from_str("data"),
                    &Uint16Array::from(data.as_slice()).into(),
                );
                obj.into()
            }
            #[cfg(feature = "diagnostics")]
            WasmResponse::DeviceIdentification {
                read_device_id_code,
                conformity_level,
                more_follows,
                objects,
            } => {
                let obj = Object::new();
                let _ = Reflect::set(
                    &obj,
                    &JsValue::from_str("readDeviceIdCode"),
                    &JsValue::from_f64(read_device_id_code as f64),
                );
                let _ = Reflect::set(
                    &obj,
                    &JsValue::from_str("conformityLevel"),
                    &JsValue::from_f64(conformity_level as f64),
                );
                let _ = Reflect::set(
                    &obj,
                    &JsValue::from_str("moreFollows"),
                    &JsValue::from_bool(more_follows),
                );
                let objects_arr = Array::new();
                for (id, val) in objects {
                    let entry = Object::new();
                    let _ = Reflect::set(
                        &entry,
                        &JsValue::from_str("id"),
                        &JsValue::from_f64(id as f64),
                    );
                    let _ = Reflect::set(
                        &entry,
                        &JsValue::from_str("value"),
                        &JsValue::from_str(&val),
                    );
                    objects_arr.push(&entry.into());
                }
                let _ = Reflect::set(&obj, &JsValue::from_str("objects"), &objects_arr);
                obj.into()
            }
        }
    }
}
