/*
配置变量解析
 */
use anyhow::anyhow;
use anyhow::Result;
use std::cell::RefCell;
use std::rc::Rc;

struct VarItem {
    is_var: bool,
    data: String,
}

pub struct VarData {
    item: Rc<VarItem>,
    var_data: RefCell<Option<String>>,
}

impl Clone for VarData {
    fn clone(&self) -> Self {
        Self {
            item: self.item.clone(),
            var_data: RefCell::new(None),
        }
    }
}

pub struct VarContext {
    vars: String,
    //items: Vec<Rc<VarItem>>,
    pub default_str: Rc<Option<String>>,
}

pub struct Var {
    pub context: Rc<VarContext>,
    pub datas: Vec<VarData>,
}

impl Var {
    pub fn new(vars_str: &str, default_str: Option<&str>) -> Result<Var> {
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
                let item = Rc::new(VarItem {
                    is_var: false,
                    data: vars.to_string(),
                });
                //items.push(item.clone());
                let data = VarData {
                    item: item,
                    var_data: RefCell::new(None),
                };
                datas.push(data);
                break;
            }
            let var_start_index = var_start_index.unwrap();
            if var_start_index > 0 {
                let data = &vars[..var_start_index];
                let item = Rc::new(VarItem {
                    is_var: false,
                    data: data.to_string(),
                });
                //items.push(item.clone());
                let data = VarData {
                    item: item,
                    var_data: RefCell::new(None),
                };
                datas.push(data);
                vars = &vars[var_start_index..];
            }

            let var_end_index = vars.find(var_end);
            let var_end_index =
                var_end_index.ok_or(anyhow!("err:var invalid => var:{}", vars_str))?;
            let data = &vars[..var_end_index + 1];
            let item = Rc::new(VarItem {
                is_var: true,
                data: data.to_string(),
            });
            //items.push(item.clone());
            let data = VarData {
                item: item,
                var_data: RefCell::new(None),
            };
            datas.push(data);

            if var_end_index == vars.len() - 1 {
                break;
            }

            vars = &vars[var_end_index + 1..];
        }

        Ok(Var {
            context: Rc::new(VarContext {
                vars: vars_str.to_string(),
                //items,
                default_str: {
                    if default_str.is_none() {
                        Rc::new(None)
                    } else {
                        Rc::new(Some(default_str.unwrap().to_string()))
                    }
                },
            }),
            datas,
        })
    }

    pub fn copy(var_parse: &Var) -> Result<Var> {
        Ok(Var {
            context: var_parse.context.clone(),
            datas: var_parse.datas.to_vec(),
        })
    }

    pub fn for_each<'b, S>(&mut self, mut service: S) -> Result<()>
    where
        S: FnMut(&str) -> Result<Option<String>>,
    {
        for v in self.datas.iter_mut() {
            if v.item.is_var && v.var_data.borrow().is_none() {
                let var_data = service(v.item.data.as_str());
                match var_data {
                    Err(e) => return Err(e)?,
                    Ok(var_data) => {
                        if var_data.is_some() {
                            *v.var_data.borrow_mut() = Some(var_data.unwrap());
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub fn join(&self) -> Result<String> {
        let mut var_datas = String::with_capacity(128);
        for v in self.datas.iter() {
            let var_data = v.var_data.borrow();
            let data = if v.item.is_var {
                if let Some(data) = var_data.as_ref() {
                    data.as_str()
                } else {
                    self.context
                        .default_str
                        .as_ref()
                        .as_ref()
                        .ok_or(anyhow!(
                            "err:var invalid => var:{}, vars:{}",
                            v.item.data,
                            self.context.vars
                        ))?
                        .as_str()
                }
            } else {
                v.item.data.as_str()
            };
            var_datas.push_str(data);
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
