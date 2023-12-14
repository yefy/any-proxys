/*
域名解析器，支持全域名， 范域名， 全匹配
 */
use any_base::util::ArcString;
use anyhow::anyhow;
use anyhow::Result;
use regex::Regex;
use std::collections::HashMap;

#[derive(Clone)]
pub struct DomainIndex {
    domain_map: HashMap<String, i32>,
    domain_regex_map: HashMap<String, (String, i32)>,
    full_match_index: Option<i32>,
}

impl DomainIndex {
    pub fn new(domains: &HashMap<i32, (ArcString, i32)>) -> Result<DomainIndex> {
        let mut domain_map = HashMap::new();
        let mut domain_regex_map = HashMap::new();
        let mut full_match_index: Option<i32> = None;

        for (_, (domain, index)) in domains.iter() {
            let domains = domain.trim().split(" ").collect::<Vec<_>>();
            for v in domains.iter() {
                let v = v.trim();
                if v.len() <= 0 {
                    continue;
                }
                if v.len() >= 3 && &v[0..2] == "$$" {
                    let v = &v[2..];
                    log::trace!("v:{}", v);
                    if v == "(.*)" {
                        if full_match_index.is_some() {
                            return Err(anyhow!("err:domain exist => domain:{}", v));
                        }
                        full_match_index = Some(*index);
                        continue;
                    }
                    let _ = Regex::new(v).map_err(|e| anyhow!("err:Regex::new => e:{}", e))?;

                    let func = |vars_str: &str| -> Result<(String, String)> {
                        let vars = &vars_str[..];
                        let var_start = "(";
                        let var_end = ")";
                        let var_start_index = vars.find(var_start);
                        if var_start_index.is_none() {
                            return Err(anyhow!("err:domain => domain:{}", vars_str));
                        }
                        let var_start_index = var_start_index.unwrap();
                        if var_start_index != 0 {
                            return Err(anyhow!("err:domain => domain:{}", vars_str));
                        }

                        let var_end_index = vars_str.rfind(var_end);
                        if var_end_index.is_none() {
                            return Err(anyhow!("err:domain => domain:{}", vars_str));
                        }
                        let var_end_index = var_end_index.unwrap();

                        if var_end_index == vars_str.len() - 1 {
                            return Err(anyhow!("err:domain => domain:{}", vars_str));
                        }

                        if &vars_str[var_end_index + 1..var_end_index + 2] != "." {
                            return Err(anyhow!("err:domain => domain:{}", vars_str));
                        }

                        let regex = vars_str[var_start_index..var_end_index + 1].to_string();
                        let data = vars_str[var_end_index + 1..].to_string();
                        let flag = data.find(".");
                        if flag.is_none() {
                            return Err(anyhow!("err:domain => domain:{}", vars_str));
                        }

                        if flag.unwrap() == data.len() - 1 {
                            return Err(anyhow!("err:domain => domain:{}", vars_str));
                        }
                        let flag2 = data.rfind(".");
                        if flag2.is_none() {
                            return Err(anyhow!("err:domain => domain:{}", vars_str));
                        }

                        if flag.unwrap() == flag2.unwrap() {
                            return Err(anyhow!("err:domain => domain:{}", vars_str));
                        }

                        Ok((regex, data))
                    };

                    let (_, data) = func(v).map_err(|e| anyhow!("err:func => e:{}", e))?;

                    //log::debug!("data:{}, v:{}, domain:{}", data, v, domain);

                    if domain_regex_map.get(&data).is_some() {
                        return Err(anyhow!("err:domain => domain:{}", v));
                    }

                    domain_regex_map.insert(data, (v.to_string(), *index));
                } else {
                    log::trace!("v:{}", v);
                    if domain_map.get(v).is_some() {
                        return Err(anyhow!("err:domain exist => domain:{}", domain));
                    }
                    domain_map.insert(v.to_string(), *index);
                }
            }
        }

        Ok(DomainIndex {
            domain_map,
            domain_regex_map,
            full_match_index,
        })
    }

    pub fn index(&self, domain: &str) -> Result<i32> {
        //log::debug!("domain_map:{:?}", domain_map);
        //log::debug!("domain_regex_map:{:?}", domain_regex_map);
        //log::debug!("domain:{}", domain);
        let domain_index = self.domain_map.get(domain);
        match domain_index {
            Some(domain_index) => return Ok(domain_index.clone()),
            None => {
                let vars = &domain[..];
                let find = vars.rfind(".");
                if find.is_none() {
                    return Err(anyhow!("err:domain not found => domain:{}", domain))?;
                }
                let mut vars = &vars[..find.unwrap()];
                //log::info!("vars:{}", vars);

                loop {
                    let find = vars.rfind(".");
                    if find.is_none() {
                        if self.full_match_index.is_some() {
                            return Ok(self.full_match_index.clone().unwrap());
                        }
                        return Err(anyhow!("err:domain not found => domain:{}", domain))?;
                    }
                    vars = &vars[..find.unwrap()];
                    //log::debug!("vars:{}", vars);
                    let suffix = &domain[find.unwrap()..];
                    //log::debug!("suffix:{}", suffix);
                    let value = self.domain_regex_map.get(suffix);
                    if value.is_none() {
                        continue;
                    }
                    let (regex, domain_index) = value.unwrap();
                    //log::debug!("regex:{}, domain_index:{}", regex, domain_index);

                    let re = Regex::new(regex.as_str())
                        .map_err(|e| anyhow!("err:Regex::new => e:{}", e))?;
                    let caps = re.captures(domain);
                    if caps.is_none() {
                        continue;
                    } else {
                        return Ok(domain_index.clone());
                    }
                }
            }
        };
    }
}
