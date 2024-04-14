use crate::wasm_tcp;
use crate::wasm_std;
use crate::info;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WasmConf {
    pub name: String,
}

pub fn run(config: Option<String>) -> Result<wasm_std::Error, String> {
    if config.is_none() {
        return Ok(wasm_std::Error::Ok);
    }
    let wasm_conf: WasmConf = toml::from_str(&config.unwrap()).map_err(|e| e.to_string())?;
    info!("wasm_conf:{:?}", wasm_conf);

    let line = "GET /1.txt HTTP/1.1\r\n";
    let host = "Host: www.example.cn:20001\r\n";
    let user_agent = "User-Agent: curl/7.60.0\r\n"; // 响应内容长度
    let accept = "Accept: */*\r\n"; // 表示短连接

    // 拼接状态行、头部信息和内容
    let request = format!(
        "{}{}{}{}\r\n", // 注意末尾的 \r\n 表示换行
        line, host, user_agent, accept
    );

    //let fd =
    //    wasm_tcp::socket_connect(wasm_tcp::SocketType::Tcp, "www.upstream.cn:10001", None)?;
    let fd =
        wasm_tcp::socket_connect(wasm_tcp::SocketType::Tcp, "192.168.192.139:10001", None)?;
    wasm_tcp::socket_write_all(fd, &request)?;
    wasm_tcp::socket_flush(fd)?;
    let data = wasm_tcp::socket_read(fd, 1024)?;
    wasm_tcp::socket_flush(fd)?;
    info!("data:{:?}", data);

    // 响应的内容
    let body = "Hello, serverless!";
    // 构建 HTTP 响应的状态行和头部信息
    let status_line = "HTTP/1.1 200 OK\r\n";
    let content_type = "Content-Type: text/plain\r\n";
    let content_length = format!("Content-Length: {}\r\n", body.len()); // 响应内容长度
    let connection = "Connection: close\r\n"; // 表示短连接

    // 拼接状态行、头部信息和内容
    let response = format!(
        "{}{}{}{}\r\n{}", // 注意末尾的 \r\n 表示换行
        status_line, content_type, content_length, connection, body
    );
    // 打印生成的 HTTP 响应字符串
    info!("response:{:?}", response);
    wasm_tcp::socket_write_all(0, &response)?;
    wasm_tcp::socket_flush(0)?;

    Ok(wasm_std::Error::Ok)
}