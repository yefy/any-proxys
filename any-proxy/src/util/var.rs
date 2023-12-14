/*
配置变量解析
 */
use any_base::util::ArcString;
use anyhow::anyhow;
use anyhow::Result;
use std::fmt::Write;
use std::net::SocketAddr;
use std::sync::Arc;

pub enum VarAnyData {
    ArcStr(Arc<String>),
    ArcString(ArcString),
    Str(String),
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
}

impl VarAnyData {
    pub fn len(&self) -> usize {
        match self {
            Self::ArcStr(data) => return data.len(),
            Self::ArcString(data) => return data.len(),
            Self::Str(data) => return data.len(),
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
            Self::Str(data) => {
                write!(buf, "{}", &data)?;
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
    var_data: Option<VarAnyData>,
}

impl Clone for VarData {
    fn clone(&self) -> Self {
        Self {
            item: self.item.clone(),
            var_data: None,
        }
    }
}

pub struct VarContext {
    //vars: String,
    //items: Vec<Arc<VarItem>>,
    pub default_str: String,
}

impl VarContext {
    pub fn write_default_str(&self, buf: &mut String) -> Result<()> {
        write!(buf, "{}", self.default_str)?;
        Ok(())
    }
}

pub struct Var {
    pub context: Arc<VarContext>,
    pub datas: Vec<VarData>,
    pub max_len: usize,
}

impl Var {
    pub fn new(vars_str: &str, default_str: &str) -> Result<Var> {
        if vars_str.len() <= 0 {
            return Err(anyhow!("err:var nil"))?;
        }

        //let mut items = Vec::with_capacity(50);
        let mut datas = Vec::with_capacity(50);
        let mut vars = vars_str;
        let var_start = "${";
        let var_end = "}";

        loop {
            let var_start_index = vars.find(var_start);
            if var_start_index.is_none() {
                let item = Arc::new(VarItem {
                    is_var: false,
                    data: vars.to_string(),
                });
                //items.push(item.clone());
                let data = VarData {
                    item: item,
                    var_data: None,
                };
                datas.push(data);
                break;
            }
            let var_start_index = var_start_index.unwrap();
            if var_start_index > 0 {
                let data = &vars[..var_start_index];
                let item = Arc::new(VarItem {
                    is_var: false,
                    data: data.to_string(),
                });
                //items.push(item.clone());
                let data = VarData {
                    item: item,
                    var_data: None,
                };
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
            let data = VarData {
                item: item,
                var_data: None,
            };
            datas.push(data);

            if var_end_index == vars.len() - 1 {
                break;
            }

            vars = &vars[var_end_index + 1..];
        }

        Ok(Var {
            context: Arc::new(VarContext {
                //vars: vars_str.to_string(),
                //items,
                default_str: default_str.to_string(),
            }),
            datas,
            max_len: 128,
        })
    }

    pub fn copy(var_parse: &Var) -> Result<Var> {
        Ok(Var {
            context: var_parse.context.clone(),
            datas: var_parse.datas.to_vec(),
            max_len: var_parse.max_len,
        })
    }

    pub fn for_each<'b, S>(&mut self, mut service: S) -> Result<()>
    where
        S: FnMut(&str) -> Result<Option<VarAnyData>>,
    {
        let mut max_len = 0;
        for v in self.datas.iter_mut() {
            if v.item.is_var {
                if v.var_data.is_none() {
                    let var_data = service(v.item.data.as_str());
                    match var_data {
                        Err(e) => return Err(e)?,
                        Ok(var_data) => {
                            if var_data.is_some() {
                                v.var_data = Some(var_data.unwrap());
                            }
                        }
                    }
                }

                if v.var_data.is_none() {
                    max_len += self.context.default_str.len();
                } else {
                    max_len += v.var_data.as_ref().unwrap().len();
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
        for v in self.datas.iter() {
            if v.item.is_var {
                if let Some(data) = v.var_data.as_ref() {
                    data.write(&mut var_datas)?;
                } else {
                    self.context.write_default_str(&mut var_datas)?;
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

    pub fn host_and_port(http_host: &str) -> (&str, &str) {
        let http_hosts = http_host.trim().split(":").collect::<Vec<_>>();
        let domain = http_hosts[0].trim();
        let port = if http_hosts.len() > 1 {
            http_hosts[1].trim()
        } else {
            ""
        };
        (domain, port)
    }

    pub fn var_name(var: &str) -> &str {
        var[2..var.len() - 1].trim()
    }
    pub fn var_to_number(var: &str) -> Result<usize> {
        let var_name = Var::var_name(var);
        return Ok(var_name.parse::<usize>()?);
    }
}
