use super::config_parse;
use super::config_toml;
use crate::util::default_config;
use anyhow::anyhow;
use anyhow::Result;
use std::fs;
use std::path::Path;
use std::str;

pub struct Config {}

impl Config {
    /// 删除注释
    pub fn drop_remark<'a>(toml: &'a str) -> Option<&'a str> {
        if toml.len() <= 0 {
            return None;
        }
        let index = toml.find("#");
        if index.is_none() {
            return Some(toml);
        }
        let index = index.unwrap();
        if index <= 0 {
            return None;
        }
        Some(&toml[..index])
    }

    pub fn find_protocol(toml: &str, vars: &Vec<config_parse::ConfigVar>) -> String {
        let toml = toml.trim();
        if toml.len() <= 2 {
            return "".to_string();
        }

        if &toml[..1] == "[" && &toml[toml.len() - 1..] == "]" {
            let toml = toml[1..toml.len() - 1].trim();
            for var in vars {
                if toml == var.protocol {
                    return toml.to_string();
                }
            }
        }

        "".to_string()
    }

    pub fn find_server(toml: &str, vars: &Vec<config_parse::ConfigVar>) -> String {
        let toml = toml.trim();
        if toml.len() <= 4 {
            return "".to_string();
        }

        if &toml[..2] == "[[" && &toml[toml.len() - 2..] == "]]" {
            let toml = toml[2..toml.len() - 2].trim();
            for var in vars {
                let index = toml.find(var.server);
                if index.is_some() && index.unwrap() == 0 {
                    return toml.to_string();
                }
            }
        }

        "".to_string()
    }

    pub fn find_struct(toml: &str) -> String {
        let toml = toml.trim();
        if toml.len() <= 2 {
            return "".to_string();
        }

        if &toml[..1] == "[" && &toml[toml.len() - 1..] == "]" {
            let toml = toml[1..toml.len() - 1].trim();
            return toml.to_string();
        }

        "".to_string()
    }

    pub fn find_list(toml: &str) -> String {
        let toml = toml.trim();
        if toml.len() <= 4 {
            return "".to_string();
        }

        if &toml[..2] == "[[" && &toml[toml.len() - 2..] == "]]" {
            let toml = toml[2..toml.len() - 2].trim();
            return toml.to_string();
        }

        "".to_string()
    }

    pub fn parse(toml: &String, vars: &Vec<config_parse::ConfigVar>) -> Result<String> {
        let mut datas = Vec::new();
        let tomls = toml.split("\n").collect::<Vec<_>>();
        let mut protocol = "".to_string();
        let mut server = "".to_string();
        for toml in tomls.iter() {
            let line = Config::drop_remark(toml);
            if line.is_none() {
                datas.push(toml.to_string());
                continue;
            }
            let line = line.unwrap();
            let _protocol = Config::find_protocol(line, vars);
            if _protocol.len() > 0 {
                protocol = _protocol + ".";
                server = "".to_string();
                datas.push(toml.to_string());
                continue;
            }

            if protocol.len() <= 0 {
                datas.push(toml.to_string());
                continue;
            }

            let _server = Config::find_server(line, vars);
            if _server.len() > 0 {
                server = protocol.clone() + _server.as_str();
                let toml = &toml.replace(_server.as_str(), server.as_str());
                datas.push(toml.to_string());
                server = server + ".";
                continue;
            }

            let replace_data = if server.len() > 0 {
                server.clone()
            } else {
                protocol.clone()
            };

            let _list = Config::find_list(line);
            if _list.len() > 0 {
                let replace_data = replace_data + _list.as_str();
                let toml = &toml.replace(_list.as_str(), replace_data.as_str());
                datas.push(toml.to_string());
                continue;
            }

            let _struct = Config::find_struct(line);
            if _struct.len() > 0 {
                let replace_data = replace_data + _struct.as_str();
                let toml = &toml.replace(_struct.as_str(), replace_data.as_str());

                datas.push(toml.to_string());
                continue;
            }

            datas.push(toml.to_string());
        }
        Ok(datas.join("\n"))
    }

    pub fn get_dir_file_info(config_dir: &str) -> Result<Vec<std::ffi::OsString>> {
        let mut file_infos = Vec::with_capacity(50);
        for entry in std::fs::read_dir(config_dir)
            .map_err(|e| anyhow!("err:std::fs::read_dir => e:{}", e))?
        {
            let entry = entry.map_err(|e| anyhow!("err:entry => e:{}", e))?;
            let path = entry.path();
            let file_name = path
                .file_name()
                .ok_or(anyhow!("err:reload file_name nil"))?
                .to_owned();

            file_infos.push(file_name);
        }
        Ok(file_infos)
    }

    pub fn parse_include(toml: &String) -> Result<String> {
        let tomls = toml.split("\n").collect::<Vec<_>>();
        let flag = "include ";
        let mut datas = Vec::new();
        for toml in tomls.iter() {
            let line = Config::drop_remark(toml);
            if line.is_none() {
                datas.push(toml.to_string());
                continue;
            }

            let line = line.unwrap();
            let mut stuff_index = 0;
            for v in line.chars() {
                if v == ' ' || v == '\t' {
                    stuff_index += 1;
                    continue;
                }
                break;
            }
            let stuff = line[..stuff_index].to_string();
            let line = line.trim();
            if line.len() <= 0 {
                datas.push(toml.to_string());
                continue;
            }

            let include = line.find(flag);
            if !(include.is_some() && include.unwrap() == 0) {
                datas.push(toml.to_string());
                continue;
            }

            let include_remark = stuff.clone() + "#" + toml.trim();
            datas.push(include_remark.clone() + " {");

            let file_full_path = line[flag.len()..].trim();
            let path = Path::new(file_full_path);
            let file_name_regex = path.file_name();
            if file_name_regex.is_none() {
                return Err(anyhow!(
                    "err:file_name_regex nil => file_full_path:{}",
                    file_full_path
                ));
            }
            let file_name_regex = file_name_regex.unwrap().to_string_lossy().to_string();
            let file_name_regexs = file_name_regex.split("*").collect::<Vec<_>>();

            let dir = path.parent();
            if dir.is_none() {
                return Err(anyhow!("err:dir nil => file_full_path:{}", file_full_path));
            }
            let dir = dir.unwrap().to_string_lossy().to_string();
            let dir = { default_config::ANYPROXY_CONF_PATH.lock().unwrap().clone() + dir.as_str() };

            let file_names = Config::get_dir_file_info(&dir)
                .map_err(|e| anyhow!("err:Config::get_dir_file_info => e:{}", e))?;
            for file_name in file_names.iter() {
                let file_name = file_name.to_string_lossy().to_string();
                let mut file_name_str = &file_name[..];
                let mut is_find = true;
                for (index, file_name_regex) in file_name_regexs.iter().enumerate() {
                    if file_name_regex.len() <= 0 {
                        continue;
                    }
                    let find = file_name_str.find(file_name_regex);
                    if find.is_none() {
                        is_find = false;
                        break;
                    }

                    if index == 0 {
                        if find.unwrap() != 0 {
                            is_find = false;
                            break;
                        }
                    } else if index == file_name_regexs.len() - 1 {
                        if find.unwrap() + file_name_regex.len() != file_name_str.len() {
                            is_find = false;
                            break;
                        }
                    }

                    file_name_str = &file_name_str[file_name_regex.len()..];
                }

                if !is_find {
                    continue;
                }

                let find_path = dir.clone() + "/" + file_name.as_str();
                let content = fs::read_to_string(&find_path)
                    .map_err(|e| anyhow!("err:fs::read_to_string => e:{}", e))?;
                let contents = content.split("\n").collect::<Vec<_>>();
                for content in contents.iter() {
                    datas.push(stuff.clone() + content);
                }
            }
            datas.push(include_remark + " }\n");
        }

        Ok(datas.join("\n"))
    }

    pub fn new() -> Result<config_toml::ConfigToml> {
        let file_name = {
            default_config::ANYPROXY_CONF_FULL_PATH
                .lock()
                .unwrap()
                .clone()
        };
        let contents = fs::read_to_string(&file_name)
            .map_err(|e| anyhow!("err:fs::read_to_string => e:{}", e))?;

        // 支持配置include
        let contents = Config::parse_include(&contents)
            .map_err(|e| anyhow!("err:Config::parse_include => e:{}", e))?;
        // 支持配置二级include
        let contents = Config::parse_include(&contents)
            .map_err(|e| anyhow!("err:Config::parse_include => e:{}", e))?;
        // 支持配置三级include
        let contents = Config::parse_include(&contents)
            .map_err(|e| anyhow!("err:Config::parse_include => e:{}", e))?;

        // 最终生成的原始配置文件
        fs::write(
            default_config::ANYPROXY_CONF_LOG_RAW_PATH
                .lock()
                .unwrap()
                .as_str(),
            contents.as_str(),
        )
        .map_err(|e| anyhow!("err:ANYPROXY_CONF_LOG_RAW_PATH => e:{}", e))?;

        // 配置解析成toml格式，提供给toml解析
        let contents = Config::parse(&contents, &config_parse::CONFIG_VARS)
            .map_err(|e| anyhow!("err:Config::parse => e:{}", e))?;

        // 最终生成的配置文件，提供给错误排查
        fs::write(
            default_config::ANYPROXY_CONF_LOG_FULL_PATH
                .lock()
                .unwrap()
                .as_str(),
            contents.as_str(),
        )
        .map_err(|e| anyhow!("err:ANYPROXY_CONF_LOG_FULL_PATH => e:{}", e))?;
        let config: config_toml::ConfigToml = toml::from_str(contents.as_str())
            .map_err(|e| anyhow!("err:parse {} => e:{}", file_name, e))?;
        let config = config_parse::check(config)
            .map_err(|e| anyhow!("err:config_parse::check => e:{}", e))?;
        let config = config_parse::merger(config)
            .map_err(|e| anyhow!("err:config_parse::merger => e:{}", e))?;
        Ok(config)
    }
}
