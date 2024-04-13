use crate::config::net_core_wasm::WasmHashValue;
use crate::wasm::component::server::wasm_store;
use crate::wasm::WasmHost;
use any_base::typ::ArcRwLock;
use async_trait::async_trait;

#[async_trait]
impl wasm_store::Host for WasmHost {
    async fn set_bool(
        &mut self,
        key: String,
        value: bool,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        net_core_wasm_conf
            .wash_hash
            .get_mut()
            .insert(key, WasmHashValue::Bool(value));
        Ok(Ok(()))
    }

    async fn set_s8(
        &mut self,
        key: String,
        value: i8,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        net_core_wasm_conf
            .wash_hash
            .get_mut()
            .insert(key, WasmHashValue::I8(value));
        Ok(Ok(()))
    }

    async fn set_s16(
        &mut self,
        key: String,
        value: i16,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        net_core_wasm_conf
            .wash_hash
            .get_mut()
            .insert(key, WasmHashValue::I16(value));
        Ok(Ok(()))
    }

    async fn set_s32(
        &mut self,
        key: String,
        value: i32,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        net_core_wasm_conf
            .wash_hash
            .get_mut()
            .insert(key, WasmHashValue::I32(value));
        Ok(Ok(()))
    }

    async fn set_s64(
        &mut self,
        key: String,
        value: i64,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        net_core_wasm_conf
            .wash_hash
            .get_mut()
            .insert(key, WasmHashValue::I64(value));
        Ok(Ok(()))
    }

    async fn set_u8(
        &mut self,
        key: String,
        value: u8,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        net_core_wasm_conf
            .wash_hash
            .get_mut()
            .insert(key, WasmHashValue::U8(value));
        Ok(Ok(()))
    }

    async fn set_u16(
        &mut self,
        key: String,
        value: u16,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        net_core_wasm_conf
            .wash_hash
            .get_mut()
            .insert(key, WasmHashValue::U16(value));
        Ok(Ok(()))
    }

    async fn set_u32(
        &mut self,
        key: String,
        value: u32,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        net_core_wasm_conf
            .wash_hash
            .get_mut()
            .insert(key, WasmHashValue::U32(value));
        Ok(Ok(()))
    }

    async fn set_u64(
        &mut self,
        key: String,
        value: u64,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        net_core_wasm_conf
            .wash_hash
            .get_mut()
            .insert(key, WasmHashValue::U64(value));
        Ok(Ok(()))
    }

    async fn set_f32(
        &mut self,
        key: String,
        value: f32,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        net_core_wasm_conf
            .wash_hash
            .get_mut()
            .insert(key, WasmHashValue::F32(value));
        Ok(Ok(()))
    }

    async fn set_f64(
        &mut self,
        key: String,
        value: f64,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        net_core_wasm_conf
            .wash_hash
            .get_mut()
            .insert(key, WasmHashValue::F64(value));
        Ok(Ok(()))
    }

    async fn set_char(
        &mut self,
        key: String,
        value: char,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        net_core_wasm_conf
            .wash_hash
            .get_mut()
            .insert(key, WasmHashValue::Char(value));
        Ok(Ok(()))
    }

    async fn set_string(
        &mut self,
        key: String,
        value: String,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        net_core_wasm_conf
            .wash_hash
            .get_mut()
            .insert(key, WasmHashValue::String(value));
        Ok(Ok(()))
    }

    async fn hset_bool(
        &mut self,
        key: String,
        field: String,
        value: bool,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        let wash_hash =
            get_or_create_wash_hash_hash(key, net_core_wasm_conf.wash_hash_hash.clone());
        wash_hash
            .get_mut()
            .insert(field, WasmHashValue::Bool(value));
        Ok(Ok(()))
    }

    async fn hset_s8(
        &mut self,
        key: String,
        field: String,
        value: i8,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        let wash_hash =
            get_or_create_wash_hash_hash(key, net_core_wasm_conf.wash_hash_hash.clone());
        wash_hash.get_mut().insert(field, WasmHashValue::I8(value));
        Ok(Ok(()))
    }

    async fn hset_s16(
        &mut self,
        key: String,
        field: String,
        value: i16,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        let wash_hash =
            get_or_create_wash_hash_hash(key, net_core_wasm_conf.wash_hash_hash.clone());
        wash_hash.get_mut().insert(field, WasmHashValue::I16(value));
        Ok(Ok(()))
    }

    async fn hset_s32(
        &mut self,
        key: String,
        field: String,
        value: i32,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        let wash_hash =
            get_or_create_wash_hash_hash(key, net_core_wasm_conf.wash_hash_hash.clone());
        wash_hash.get_mut().insert(field, WasmHashValue::I32(value));
        Ok(Ok(()))
    }

    async fn hset_s64(
        &mut self,
        key: String,
        field: String,
        value: i64,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        let wash_hash =
            get_or_create_wash_hash_hash(key, net_core_wasm_conf.wash_hash_hash.clone());
        wash_hash.get_mut().insert(field, WasmHashValue::I64(value));
        Ok(Ok(()))
    }

    async fn hset_u8(
        &mut self,
        key: String,
        field: String,
        value: u8,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        let wash_hash =
            get_or_create_wash_hash_hash(key, net_core_wasm_conf.wash_hash_hash.clone());
        wash_hash.get_mut().insert(field, WasmHashValue::U8(value));
        Ok(Ok(()))
    }

    async fn hset_u16(
        &mut self,
        key: String,
        field: String,
        value: u16,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        let wash_hash =
            get_or_create_wash_hash_hash(key, net_core_wasm_conf.wash_hash_hash.clone());
        wash_hash.get_mut().insert(field, WasmHashValue::U16(value));
        Ok(Ok(()))
    }

    async fn hset_u32(
        &mut self,
        key: String,
        field: String,
        value: u32,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        let wash_hash =
            get_or_create_wash_hash_hash(key, net_core_wasm_conf.wash_hash_hash.clone());
        wash_hash.get_mut().insert(field, WasmHashValue::U32(value));
        Ok(Ok(()))
    }

    async fn hset_u64(
        &mut self,
        key: String,
        field: String,
        value: u64,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        let wash_hash =
            get_or_create_wash_hash_hash(key, net_core_wasm_conf.wash_hash_hash.clone());
        wash_hash.get_mut().insert(field, WasmHashValue::U64(value));
        Ok(Ok(()))
    }

    async fn hset_f32(
        &mut self,
        key: String,
        field: String,
        value: f32,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        let wash_hash =
            get_or_create_wash_hash_hash(key, net_core_wasm_conf.wash_hash_hash.clone());
        wash_hash.get_mut().insert(field, WasmHashValue::F32(value));
        Ok(Ok(()))
    }

    async fn hset_f64(
        &mut self,
        key: String,
        field: String,
        value: f64,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        let wash_hash =
            get_or_create_wash_hash_hash(key, net_core_wasm_conf.wash_hash_hash.clone());
        wash_hash.get_mut().insert(field, WasmHashValue::F64(value));
        Ok(Ok(()))
    }

    async fn hset_char(
        &mut self,
        key: String,
        field: String,
        value: char,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        let wash_hash =
            get_or_create_wash_hash_hash(key, net_core_wasm_conf.wash_hash_hash.clone());
        wash_hash
            .get_mut()
            .insert(field, WasmHashValue::Char(value));
        Ok(Ok(()))
    }

    async fn hset_string(
        &mut self,
        key: String,
        field: String,
        value: String,
    ) -> wasmtime::Result<std::result::Result<(), String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        let wash_hash =
            get_or_create_wash_hash_hash(key, net_core_wasm_conf.wash_hash_hash.clone());
        wash_hash
            .get_mut()
            .insert(field, WasmHashValue::String(value));
        Ok(Ok(()))
    }

    async fn get_bool(
        &mut self,
        key: String,
    ) -> wasmtime::Result<std::result::Result<Option<bool>, String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        let value = net_core_wasm_conf.wash_hash.get().get(&key).cloned();
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::Bool(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }

    async fn get_s8(
        &mut self,
        key: String,
    ) -> wasmtime::Result<std::result::Result<Option<i8>, String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        let value = net_core_wasm_conf.wash_hash.get().get(&key).cloned();
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::I8(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }

    async fn get_s16(
        &mut self,
        key: String,
    ) -> wasmtime::Result<std::result::Result<Option<i16>, String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        let value = net_core_wasm_conf.wash_hash.get().get(&key).cloned();
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::I16(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }
    async fn get_s32(
        &mut self,
        key: String,
    ) -> wasmtime::Result<std::result::Result<Option<i32>, String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        let value = net_core_wasm_conf.wash_hash.get().get(&key).cloned();
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::I32(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }
    async fn get_s64(
        &mut self,
        key: String,
    ) -> wasmtime::Result<std::result::Result<Option<i64>, String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        let value = net_core_wasm_conf.wash_hash.get().get(&key).cloned();
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::I64(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }

    async fn get_u8(
        &mut self,
        key: String,
    ) -> wasmtime::Result<std::result::Result<Option<u8>, String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        let value = net_core_wasm_conf.wash_hash.get().get(&key).cloned();
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::U8(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }

    async fn get_u16(
        &mut self,
        key: String,
    ) -> wasmtime::Result<std::result::Result<Option<u16>, String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        let value = net_core_wasm_conf.wash_hash.get().get(&key).cloned();
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::U16(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }
    async fn get_u32(
        &mut self,
        key: String,
    ) -> wasmtime::Result<std::result::Result<Option<u32>, String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        let value = net_core_wasm_conf.wash_hash.get().get(&key).cloned();
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::U32(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }
    async fn get_u64(
        &mut self,
        key: String,
    ) -> wasmtime::Result<std::result::Result<Option<u64>, String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        let value = net_core_wasm_conf.wash_hash.get().get(&key).cloned();
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::U64(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }

    async fn get_f32(
        &mut self,
        key: String,
    ) -> wasmtime::Result<std::result::Result<Option<f32>, String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        let value = net_core_wasm_conf.wash_hash.get().get(&key).cloned();
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::F32(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }
    async fn get_f64(
        &mut self,
        key: String,
    ) -> wasmtime::Result<std::result::Result<Option<f64>, String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        let value = net_core_wasm_conf.wash_hash.get().get(&key).cloned();
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::F64(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }

    async fn get_char(
        &mut self,
        key: String,
    ) -> wasmtime::Result<std::result::Result<Option<char>, String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        let value = net_core_wasm_conf.wash_hash.get().get(&key).cloned();
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::Char(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }

    async fn get_string(
        &mut self,
        key: String,
    ) -> wasmtime::Result<std::result::Result<Option<String>, String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        let value = net_core_wasm_conf.wash_hash.get().get(&key).cloned();
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::String(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }

    async fn hget_bool(
        &mut self,
        key: String,
        field: String,
    ) -> wasmtime::Result<std::result::Result<Option<bool>, String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        let value = get_wash_hash_value(key, field, net_core_wasm_conf.wash_hash_hash.clone());
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::Bool(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }

    async fn hget_s8(
        &mut self,
        key: String,
        field: String,
    ) -> wasmtime::Result<std::result::Result<Option<i8>, String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        let value = get_wash_hash_value(key, field, net_core_wasm_conf.wash_hash_hash.clone());
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::I8(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }

    async fn hget_s16(
        &mut self,
        key: String,
        field: String,
    ) -> wasmtime::Result<std::result::Result<Option<i16>, String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        let value = get_wash_hash_value(key, field, net_core_wasm_conf.wash_hash_hash.clone());
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::I16(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }
    async fn hget_s32(
        &mut self,
        key: String,
        field: String,
    ) -> wasmtime::Result<std::result::Result<Option<i32>, String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        let value = get_wash_hash_value(key, field, net_core_wasm_conf.wash_hash_hash.clone());
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::I32(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }
    async fn hget_s64(
        &mut self,
        key: String,
        field: String,
    ) -> wasmtime::Result<std::result::Result<Option<i64>, String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        let value = get_wash_hash_value(key, field, net_core_wasm_conf.wash_hash_hash.clone());
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::I64(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }

    async fn hget_u8(
        &mut self,
        key: String,
        field: String,
    ) -> wasmtime::Result<std::result::Result<Option<u8>, String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        let value = get_wash_hash_value(key, field, net_core_wasm_conf.wash_hash_hash.clone());
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::U8(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }

    async fn hget_u16(
        &mut self,
        key: String,
        field: String,
    ) -> wasmtime::Result<std::result::Result<Option<u16>, String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        let value = get_wash_hash_value(key, field, net_core_wasm_conf.wash_hash_hash.clone());
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::U16(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }
    async fn hget_u32(
        &mut self,
        key: String,
        field: String,
    ) -> wasmtime::Result<std::result::Result<Option<u32>, String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        let value = get_wash_hash_value(key, field, net_core_wasm_conf.wash_hash_hash.clone());
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::U32(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }
    async fn hget_u64(
        &mut self,
        key: String,
        field: String,
    ) -> wasmtime::Result<std::result::Result<Option<u64>, String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        let value = get_wash_hash_value(key, field, net_core_wasm_conf.wash_hash_hash.clone());
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::U64(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }

    async fn hget_f32(
        &mut self,
        key: String,
        field: String,
    ) -> wasmtime::Result<std::result::Result<Option<f32>, String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        let value = get_wash_hash_value(key, field, net_core_wasm_conf.wash_hash_hash.clone());
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::F32(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }
    async fn hget_f64(
        &mut self,
        key: String,
        field: String,
    ) -> wasmtime::Result<std::result::Result<Option<f64>, String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        let value = get_wash_hash_value(key, field, net_core_wasm_conf.wash_hash_hash.clone());
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::F64(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }

    async fn hget_char(
        &mut self,
        key: String,
        field: String,
    ) -> wasmtime::Result<std::result::Result<Option<char>, String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        let value = get_wash_hash_value(key, field, net_core_wasm_conf.wash_hash_hash.clone());
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::Char(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }

    async fn hget_string(
        &mut self,
        key: String,
        field: String,
    ) -> wasmtime::Result<std::result::Result<Option<String>, String>> {
        use crate::config::net_core_wasm;
        let net_core_wasm_conf = net_core_wasm::main_conf(self.scc.ms()).await;
        let value = get_wash_hash_value(key, field, net_core_wasm_conf.wash_hash_hash.clone());
        if value.is_none() {
            return Ok(Ok(None));
        }
        let value = value.unwrap();
        if let WasmHashValue::String(value) = value {
            return Ok(Ok(Some(value)));
        }
        return Ok(Err("err:type".to_string()));
    }
}

pub fn get_or_create_wash_hash_hash(
    key: String,
    wash_hash_hash: ArcRwLock<
        std::collections::HashMap<
            String,
            ArcRwLock<std::collections::HashMap<String, WasmHashValue>>,
        >,
    >,
) -> ArcRwLock<std::collections::HashMap<String, WasmHashValue>> {
    let wash_hash = wash_hash_hash.get().get(&key).cloned();
    let wash_hash = if wash_hash.is_some() {
        wash_hash.unwrap()
    } else {
        let wash_hash_hash = &mut wash_hash_hash.get_mut();
        let wash_hash = wash_hash_hash.get(&key).cloned();
        if wash_hash.is_some() {
            wash_hash.unwrap()
        } else {
            let wash_hash = ArcRwLock::new(std::collections::HashMap::new());
            wash_hash_hash.insert(key, wash_hash.clone());
            wash_hash
        }
    };
    wash_hash
}

pub fn get_wash_hash_value(
    key: String,
    field: String,
    wash_hash_hash: ArcRwLock<
        std::collections::HashMap<
            String,
            ArcRwLock<std::collections::HashMap<String, WasmHashValue>>,
        >,
    >,
) -> Option<WasmHashValue> {
    let value = wash_hash_hash.get().get(&key).cloned();
    if value.is_none() {
        return None;
    }
    let value = value.unwrap();
    let value = value.get().get(&field).cloned();
    value
}
