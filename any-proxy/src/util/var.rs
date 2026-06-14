/*
配置变量解析
 */
use any_base::util::ArcString;
use anyhow::anyhow;
use anyhow::Result;
use http::HeaderValue;
use std::fmt::Write;
use std::net::SocketAddr;
use std::sync::Arc;

pub enum VarAnyData<'a> {
    ArcStr(Arc<String>),
    ArcString(ArcString),
    String(String),
    Str(&'a str),
    Bool(bool),
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    Isize(isize),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    Usize(usize),
    F32(f32),
    F64(f64),
    SocketAddr(SocketAddr),
    U128(u128),
    HeaderValue(HeaderValue),
}

impl VarAnyData<'_> {
    pub fn len(&self) -> usize {
        match self {
            Self::ArcStr(data) => return data.len(),
            Self::ArcString(data) => return data.len(),
            Self::String(data) => return data.len(),
            Self::Str(data) => return data.len(),
            Self::Bool(data) => {
                if *data {
                    return "true".len();
                } else {
                    return "false".len();
                }
            }
            Self::I8(data) => {
                let mut n = 0;
                let mut d = *data as i32;
                loop {
                    d = d >> 10;
                    n += 10;
                    if d <= 0 {
                        return n;
                    }
                }
            }
            Self::I16(data) => {
                let mut n = 0;
                let mut d = *data;
                loop {
                    d = d >> 10;
                    n += 10;
                    if d <= 0 {
                        return n;
                    }
                }
            }
            Self::I32(data) => {
                let mut n = 0;
                let mut d = *data;
                loop {
                    d = d >> 10;
                    n += 10;
                    if d <= 0 {
                        return n;
                    }
                }
            }
            Self::I64(data) => {
                let mut n = 0;
                let mut d = *data;
                loop {
                    d = d >> 10;
                    n += 10;
                    if d <= 0 {
                        return n;
                    }
                }
            }
            Self::Isize(data) => {
                let mut n = 0;
                let mut d = *data;
                loop {
                    d = d >> 10;
                    n += 10;
                    if d <= 0 {
                        return n;
                    }
                }
            }
            Self::U8(data) => {
                let mut n = 0;
                let mut d = *data as i32;
                loop {
                    d = d >> 10;
                    n += 10;
                    if d <= 0 {
                        return n;
                    }
                }
            }
            Self::U16(data) => {
                let mut n = 0;
                let mut d = *data;
                loop {
                    d = d >> 10;
                    n += 10;
                    if d <= 0 {
                        return n;
                    }
                }
            }
            Self::U32(data) => {
                let mut n = 0;
                let mut d = *data;
                loop {
                    d = d >> 10;
                    n += 10;
                    if d <= 0 {
                        return n;
                    }
                }
            }
            Self::U64(data) => {
                let mut n = 0;
                let mut d = *data;
                loop {
                    d = d >> 10;
                    n += 10;
                    if d <= 0 {
                        return n;
                    }
                }
            }
            Self::Usize(data) => {
                let mut n = 0;
                let mut d = *data;
                loop {
                    d = d >> 10;
                    n += 10;
                    if d <= 0 {
                        return n;
                    }
                }
            }
            Self::F32(data) => {
                let mut n = 0;
                let mut d = (*data * 1000.0) as i64;
                loop {
                    d = d >> 10;
                    n += 10;
                    if d <= 0 {
                        return n;
                    }
                }
            }
            Self::F64(data) => {
                let mut n = 0;
                let mut d = (*data * 1000.0) as i64;
                loop {
                    d = d >> 10;
                    n += 10;
                    if d <= 0 {
                        return n;
                    }
                }
            }

            Self::SocketAddr(_) => {
                return 30;
            }
            Self::U128(data) => {
                let mut n = 0;
                let mut d = *data;
                loop {
                    d = d >> 10;
                    n += 10;
                    if d <= 0 {
                        return n;
                    }
                }
            }
            Self::HeaderValue(data) => {
                let data = data.to_str();
                if data.is_err() {
                    return 10;
                }
                data.unwrap().len()
            }
        }
    }

    pub fn write(&self, buf: &mut String) -> Result<()> {
        match &self {
            Self::ArcStr(data) => {
                write!(buf, "{}", &data)?;
                Ok(())
            }
            Self::ArcString(data) => {
                write!(buf, "{}", &data)?;
                Ok(())
            }
            Self::String(data) => {
                write!(buf, "{}", &data)?;
                Ok(())
            }
            Self::Str(data) => {
                write!(buf, "{}", data)?;
                Ok(())
            }
            Self::Bool(data) => {
                if *data {
                    write!(buf, "{}", "true")?;
                } else {
                    write!(buf, "{}", "false")?;
                }
                Ok(())
            }
            Self::I8(data) => {
                write!(buf, "{}", &data)?;
                Ok(())
            }
            Self::I16(data) => {
                write!(buf, "{}", &data)?;
                Ok(())
            }
            Self::I32(data) => {
                write!(buf, "{}", &data)?;
                Ok(())
            }
            Self::I64(data) => {
                write!(buf, "{}", &data)?;
                Ok(())
            }
            Self::Isize(data) => {
                write!(buf, "{}", &data)?;
                Ok(())
            }
            Self::U8(data) => {
                write!(buf, "{}", &data)?;
                Ok(())
            }
            Self::U16(data) => {
                write!(buf, "{}", &data)?;
                Ok(())
            }
            Self::U32(data) => {
                write!(buf, "{}", &data)?;
                Ok(())
            }
            Self::U64(data) => {
                write!(buf, "{}", &data)?;
                Ok(())
            }
            Self::Usize(data) => {
                write!(buf, "{}", &data)?;
                Ok(())
            }

            Self::F32(data) => {
                write!(buf, "{}", &data)?;
                Ok(())
            }

            Self::F64(data) => {
                write!(buf, "{}", &data)?;
                Ok(())
            }

            Self::SocketAddr(data) => {
                write!(buf, "{}", &data)?;
                Ok(())
            }
            Self::U128(data) => {
                write!(buf, "{}", &data)?;
                Ok(())
            }
            Self::HeaderValue(data) => {
                let data = data.to_str();
                if data.is_ok() {
                    write!(buf, "{}", data.unwrap())?;
                }
                Ok(())
            }
        }
    }

    pub fn to_string(&self) -> Result<String> {
        let len = self.len();
        let mut string = String::with_capacity(len);
        self.write(&mut string)?;
        Ok(string)
    }

    pub fn to_str(&self) -> Result<&str> {
        match &self {
            Self::ArcStr(data) => Ok(data.as_ref()),
            Self::ArcString(data) => Ok(data.as_str()),
            Self::String(data) => Ok(&data),
            Self::Str(data) => Ok(data),
            Self::Bool(_data) => Ok(""),
            Self::I8(_data) => Ok(""),
            Self::I16(_data) => Ok(""),
            Self::I32(_data) => Ok(""),
            Self::I64(_data) => Ok(""),
            Self::Isize(_data) => Ok(""),
            Self::U8(_data) => Ok(""),
            Self::U16(_data) => Ok(""),
            Self::U32(_data) => Ok(""),
            Self::U64(_data) => Ok(""),
            Self::Usize(_data) => Ok(""),
            Self::F32(_data) => Ok(""),
            Self::F64(_data) => Ok(""),
            Self::SocketAddr(_data) => Ok(""),
            Self::U128(_data) => Ok(""),
            Self::HeaderValue(data) => {
                let data = data.to_str()?;
                Ok(data)
            }
        }
    }

    pub fn to_number(&self) -> Result<i128> {
        match &self {
            Self::ArcStr(_data) => Ok(0),
            Self::ArcString(_data) => Ok(0),
            Self::String(_data) => Ok(0),
            Self::Str(_data) => Ok(0),
            Self::Bool(data) => {
                if *data {
                    Ok(1)
                } else {
                    Ok(0)
                }
            }
            Self::I8(data) => Ok(*data as i128),
            Self::I16(data) => Ok(*data as i128),
            Self::I32(data) => Ok(*data as i128),
            Self::I64(data) => Ok(*data as i128),
            Self::Isize(data) => Ok(*data as i128),
            Self::U8(data) => Ok(*data as i128),
            Self::U16(data) => Ok(*data as i128),
            Self::U32(data) => Ok(*data as i128),
            Self::U64(data) => Ok(*data as i128),
            Self::Usize(data) => Ok(*data as i128),
            Self::F32(data) => Ok(*data as i128),
            Self::F64(data) => Ok(*data as i128),
            Self::SocketAddr(_data) => Ok(0),
            Self::U128(data) => Ok(*data as i128),
            Self::HeaderValue(_data) => Ok(0),
        }
    }

    pub fn to_float(&self) -> Result<f64> {
        match &self {
            Self::ArcStr(_data) => Ok(0.0),
            Self::ArcString(_data) => Ok(0.0),
            Self::String(_data) => Ok(0.0),
            Self::Str(_data) => Ok(0.0),
            Self::Bool(data) => {
                if *data {
                    Ok(1.0)
                } else {
                    Ok(0.0)
                }
            }
            Self::I8(data) => Ok(*data as f64),
            Self::I16(data) => Ok(*data as f64),
            Self::I32(data) => Ok(*data as f64),
            Self::I64(data) => Ok(*data as f64),
            Self::Isize(data) => Ok(*data as f64),
            Self::U8(data) => Ok(*data as f64),
            Self::U16(data) => Ok(*data as f64),
            Self::U32(data) => Ok(*data as f64),
            Self::U64(data) => Ok(*data as f64),
            Self::Usize(data) => Ok(*data as f64),
            Self::F32(data) => Ok(*data as f64),
            Self::F64(data) => Ok(*data as f64),
            Self::SocketAddr(_data) => Ok(0.0),
            Self::U128(data) => Ok(*data as f64),
            Self::HeaderValue(_data) => Ok(0.0),
        }
    }
}

struct VarItem {
    is_var: bool,
    data: String,
}

impl VarItem {
    pub fn write_data(&self, buf: &mut String) -> Result<()> {
        write!(buf, "{}", self.data)?;
        Ok(())
    }
}

pub struct VarData {
    item: Arc<VarItem>,
}

impl Clone for VarData {
    fn clone(&self) -> Self {
        Self {
            item: self.item.clone(),
        }
    }
}

pub struct VarDataAny<'a> {
    var_data: Option<VarAnyData<'a>>,
}

impl Default for VarDataAny<'_> {
    fn default() -> Self {
        Self { var_data: None }
    }
}

impl Clone for VarDataAny<'_> {
    fn clone(&self) -> Self {
        Self { var_data: None }
    }
}

pub struct VarContext {
    pub default_str: String,
    pub datas: Vec<VarData>,
    pub is_var: bool,
}

impl VarContext {
    pub fn write_default_str(&self, buf: &mut String) -> Result<()> {
        write!(buf, "{}", self.default_str)?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct VarParse {
    pub context: Arc<VarContext>,
}

impl VarParse {
    pub fn new(vars_str: &str, default_str: &str) -> Result<VarParse> {
        //let mut items = Vec::with_capacity(50);
        let mut datas = Vec::with_capacity(50);
        let mut vars = vars_str;
        let var_start = "${";
        let var_end = "}";
        let mut is_var = false;

        loop {
            let var_start_index = vars.find(var_start);
            if var_start_index.is_none() {
                let item = Arc::new(VarItem {
                    is_var: false,
                    data: vars.to_string(),
                });
                //items.push(item.clone());
                let data = VarData { item: item };
                datas.push(data);
                break;
            }
            is_var = true;
            let var_start_index = var_start_index.unwrap();
            if var_start_index > 0 {
                let data = &vars[..var_start_index];
                let item = Arc::new(VarItem {
                    is_var: false,
                    data: data.to_string(),
                });
                //items.push(item.clone());
                let data = VarData { item: item };
                datas.push(data);
                vars = &vars[var_start_index..];
            }

            let var_end_index = vars.find(var_end);
            let var_end_index =
                var_end_index.ok_or(anyhow!("err:var invalid => var:{}", vars_str))?;
            let data = &vars[..var_end_index + 1];
            let item = Arc::new(VarItem {
                is_var: true,
                data: data.to_string(),
            });
            //items.push(item.clone());
            let data = VarData { item: item };
            datas.push(data);

            if var_end_index == vars.len() - 1 {
                break;
            }

            vars = &vars[var_end_index + 1..];
        }

        Ok(VarParse {
            context: Arc::new(VarContext {
                default_str: default_str.to_string(),
                datas,
                is_var,
            }),
        })
    }
}

pub struct Var<'a> {
    pub var_parse: VarParse,
    pub data_anys: Vec<VarDataAny<'a>>,
    pub max_len: usize,
}

impl<'a> Var<'a> {
    pub fn copy(var_parse: &VarParse) -> Result<Var> {
        Ok(Var {
            var_parse: var_parse.clone(),
            data_anys: vec![VarDataAny::default(); var_parse.context.datas.len()],
            max_len: 128,
        })
    }

    pub fn for_each<'b, S>(&mut self, mut service: S) -> Result<()>
    where
        S: FnMut(&str) -> Result<Option<VarAnyData<'a>>>,
    {
        let mut max_len = 0;
        for (i, v) in self.var_parse.context.datas.iter().enumerate() {
            if v.item.is_var {
                if self.data_anys[i].var_data.is_none() {
                    let var_data = service(v.item.data.as_str());
                    match var_data {
                        Err(e) => return Err(e)?,
                        Ok(var_data) => {
                            if var_data.is_some() {
                                self.data_anys[i].var_data = Some(var_data.unwrap());
                            }
                        }
                    }
                }

                if self.data_anys[i].var_data.is_none() {
                    max_len += self.var_parse.context.default_str.len();
                } else {
                    max_len += self.data_anys[i].var_data.as_ref().unwrap().len();
                }
            } else {
                max_len += v.item.data.len();
            }
            max_len += 1;
        }
        self.max_len = max_len;
        Ok(())
    }

    pub fn join(&self) -> Result<String> {
        let mut var_datas = String::with_capacity(self.max_len);
        for (i, v) in self.var_parse.context.datas.iter().enumerate() {
            if v.item.is_var {
                if let Some(data) = self.data_anys[i].var_data.as_ref() {
                    data.write(&mut var_datas)?;
                } else {
                    self.var_parse.context.write_default_str(&mut var_datas)?;
                }
            } else {
                v.item.write_data(&mut var_datas)?;
            };
        }
        Ok(var_datas)
    }

    pub fn is_valid(var: &str) -> bool {
        let var_start = "${";
        let var_end = "}";
        if var.len() <= 3 || &var[..2] != var_start || &var[var.len() - 1..] != var_end {
            return false;
        } else {
            true
        }
    }

    pub fn var_name(var: &str) -> &str {
        var[2..var.len() - 1].trim()
    }
    pub fn var_to_number(var: &str) -> Result<usize> {
        let var_name = Var::var_name(var);
        return Ok(var_name.parse::<usize>()?);
    }
}
