#![allow(dead_code)]

use crate::wasm_store;

pub fn any_to_u64<T>(value: Box<T>) -> u64 {
    // Convert the raw pointer to a u64 address
    let raw_ptr: *const T = Box::into_raw(value);
    let addr: u64 = raw_ptr as u64;
    //let addr: u64 = &*value as *const T as u64;
    //std::mem::forget(value);
    addr
}

pub fn any2_to_u64<T>(value: Box<T>) -> u64 {
    // Convert the raw pointer to a u64 address
    let addr: u64 = &*value as *const T as u64;
    std::mem::forget(value);
    addr
}

pub fn u64_to_any<T>(addr: u64) -> Box<T> {
    let value: Box<T> = unsafe { Box::from_raw(addr as *mut T) };
    value
}

pub fn set_any<T>(key: &str, value: Box<T>) -> Result<(), String> {
    let addr = any_to_u64(value);
    crate::info!("set_any key:{}, addr:{:?}", key, addr);
    wasm_store::set_u64(key, addr)
}

pub fn get_any<T>(key: &str) -> Result<Option<Box<T>>, String> {
    let addr = wasm_store::get_u64(key)?;
    crate::info!("get_any key:{}, addr:{:?}", key, addr);
    if addr.is_none() {
        return Ok(None);
    }
    let addr = addr.unwrap();
    if addr == 0 {
        return Ok(None);
    }
    let value = u64_to_any(addr);
    Ok(Some(value))
}

pub fn del_any<T>(key: &str) -> Result<bool, String> {
    let value = get_any::<T>(key)?;
    if value.is_none() {
        return Ok(false);
    }
    let value = value.unwrap();
    std::mem::drop(value);
    let _ = wasm_store::set_u64(key, 0);
    Ok(true)
}

pub fn drop_any<T>(key: &str, value: Box<T>) {
    std::mem::drop(value);
    let _ = wasm_store::set_u64(key, 0);
}
