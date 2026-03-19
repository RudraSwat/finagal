use std::ffi::{CString, c_char, c_void, CStr};
use libffi::low::{ffi_type, ffi_cif, prep_cif};
use libffi::raw::{ffi_type_sint64, ffi_type_float, ffi_type_double, ffi_type_pointer, ffi_abi_FFI_DEFAULT_ABI, ffi_call};

use crate::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum CType {
    Char,
    Double,
    Float,
    Int,
    Str,
}

pub fn string_to_ctype(s: &str) -> CType {
    match s {
        "Char" => CType::Char,
        "Double" => CType::Double,
        "Float" => CType::Float,
        "Int" => CType::Int,
        "Str" => CType::Str,
        _ => panic!("unknown C type: {}", s),
    }
}

#[allow(static_mut_refs)]
pub unsafe fn call_ffi_function(func_ptr: unsafe extern "C" fn(), ctypes: &[CType], values: &[Value], return_type: CType) -> Value {
    assert!(ctypes.len() == values.len(), "number of ctypes and values must match");

    let mut int_storage: Vec<i64> = Vec::new();
    let mut float_storage: Vec<f64> = Vec::new();
    let mut string_storage: Vec<CString> = Vec::new();
    let mut string_ptrs: Vec<*const c_char> = Vec::new();
    let mut arg_types: Vec<*mut ffi_type> = Vec::with_capacity(values.len());
    let mut arg_values: Vec<*mut ::std::os::raw::c_void> = Vec::with_capacity(values.len());

    unsafe {
        for (val, ctype) in values.iter().zip(ctypes.iter()) {
            match (val, ctype) {
                (Value::Int(i), CType::Int) => {
                    int_storage.push(*i);
                    arg_types.push(&mut ffi_type_sint64);
                    arg_values.push(int_storage.last_mut().unwrap() as *mut _ as *mut _);
                }
                (Value::Float(f), CType::Float) => {
                    float_storage.push(*f as f64);
                    arg_types.push(&mut ffi_type_float);
                    arg_values.push(float_storage.last_mut().unwrap() as *mut _ as *mut _);
                }
                (Value::Float(f), CType::Double) => {
                    float_storage.push(*f);
                    arg_types.push(&mut ffi_type_double);
                    arg_values.push(float_storage.last_mut().unwrap() as *mut _ as *mut _);
                }
                (Value::Str(s), CType::Str) => {
                    let cstr = CString::new(s.as_str()).unwrap();
                    string_storage.push(cstr);
                    let ptr = string_storage.last().unwrap().as_ptr();
                    string_ptrs.push(ptr);
                    arg_types.push(&mut ffi_type_pointer);
                    arg_values.push(string_ptrs.last_mut().unwrap() as *mut _ as *mut c_void);
                }
                _ => panic!("unsupported type conversion: {:?} -> {:?}", val, ctype),
            }
        }

        let ret_type: *mut ffi_type = match return_type {
            CType::Int => &mut ffi_type_sint64 as *mut _,
            CType::Float => &mut ffi_type_float as *mut _,
            CType::Double => &mut ffi_type_double as *mut _,
            CType::Str => &mut ffi_type_pointer as *mut _,
            _ => panic!("unsupported return type")
        };

        let mut cif: ffi_cif = std::mem::zeroed();
        prep_cif(
            &mut cif,
            ffi_abi_FFI_DEFAULT_ABI,
            arg_types.len(),
            ret_type,
            arg_types.as_mut_ptr(),
        ).expect("prep_cif failed");

        let mut ret_storage: [u8; 16] = [0; 16];
        let ret_ptr: *mut ::std::os::raw::c_void = ret_storage.as_mut_ptr() as *mut _;

        ffi_call(
            &mut cif,
            Some(std::mem::transmute(func_ptr)),
            ret_ptr,
            arg_values.as_mut_ptr(),
        );

        match return_type {
            CType::Int => Value::Int(*(ret_ptr as *mut i64)),
            CType::Float => Value::Float(*(ret_ptr as *mut f32) as f64),
            CType::Double => Value::Float(*(ret_ptr as *mut f64)),
            CType::Str => {
                let ptr = *(ret_ptr as *mut *const c_char);
                if ptr.is_null() {
                    Value::Str("".to_string())
                } else {
                    Value::Str(CStr::from_ptr(ptr).to_string_lossy().into_owned())
                }
            },
            _ => panic!("unsupported return type")
        }
    }
}
