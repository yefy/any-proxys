use crate::protopack::anyproxy::AnyproxyHello;
use crate::stream::stream_flow::StreamFlowErr;
use crate::stream::stream_flow::StreamFlowInfo;
use std::cell::RefCell;
use std::net::SocketAddr;
use std::rc::Rc;
use std::time::Instant;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrStatus {
    Ok = 200,                 //正常响应
    ClientProtoErr = 400,     //客户端请求协议出错
    AccessLimit = 403,        //访问限制
    ServerErr = 500,          //服务器内部错误
    ServiceUnavailable = 503, //upstream 链接失败
    GatewayTimeout = 504,     //upstream 链接超时
}

/// 200 对应的详细错误
//200
pub const CLI_WRITE_CLOSE: &'static str = "cli_write_close";
pub const CLI_READ_CLOSE: &'static str = "cli_read_close";
pub const CLI_WRITE_RESET: &'static str = "cli_write_reset";
pub const CLI_READ_RESET: &'static str = "cli_read_reset";
pub const CLI_WRITE_TIMEOUT: &'static str = "cli_write_timeout";
pub const CLI_READ_TIMEOUT: &'static str = "cli_read_timeout";
pub const CLI_WRITE_ERR: &'static str = "cli_write_err";
pub const CLI_READ_ERR: &'static str = "cli_read_err";

pub const UPS_WRITE_CLOSE: &'static str = "ups_write_close";
pub const UPS_READ_CLOSE: &'static str = "ups_read_close";
pub const UPS_WRITE_RESET: &'static str = "ups_write_reset";
pub const UPS_READ_RESET: &'static str = "ups_read_reset";
pub const UPS_WRITE_TIMEOUT: &'static str = "ups_write_timeout";
pub const UPS_READ_TIMEOUT: &'static str = "ups_read_timeout";
pub const UPS_WRITE_ERR: &'static str = "ups_write_err";
pub const UPS_READ_ERR: &'static str = "ups_read_err";

/// 503 对应的详细错误
//503
pub const UPS_CONN_RESET: &'static str = "ups_conn_reset";
pub const UPS_CONN_ERR: &'static str = "ups_conn_err";

pub trait ErrStatus200 {
    fn write_close(&self) -> String;
    fn read_close(&self) -> String;
    fn write_reset(&self) -> String;
    fn read_reset(&self) -> String;
    fn write_timeout(&self) -> String;
    fn read_timeout(&self) -> String;
    fn write_err(&self) -> String;
    fn read_err(&self) -> String;
}

pub struct ErrStatusClient {}

impl ErrStatus200 for ErrStatusClient {
    fn write_close(&self) -> String {
        CLI_WRITE_CLOSE.to_string()
    }
    fn read_close(&self) -> String {
        CLI_READ_CLOSE.to_string()
    }
    fn write_reset(&self) -> String {
        CLI_WRITE_RESET.to_string()
    }
    fn read_reset(&self) -> String {
        CLI_READ_RESET.to_string()
    }
    fn write_timeout(&self) -> String {
        CLI_WRITE_TIMEOUT.to_string()
    }
    fn read_timeout(&self) -> String {
        CLI_READ_TIMEOUT.to_string()
    }
    fn write_err(&self) -> String {
        CLI_WRITE_ERR.to_string()
    }
    fn read_err(&self) -> String {
        CLI_READ_ERR.to_string()
    }
}

pub struct ErrStatusUpstream {}

impl ErrStatus200 for ErrStatusUpstream {
    fn write_close(&self) -> String {
        UPS_WRITE_CLOSE.to_string()
    }
    fn read_close(&self) -> String {
        UPS_READ_CLOSE.to_string()
    }
    fn write_reset(&self) -> String {
        UPS_WRITE_RESET.to_string()
    }
    fn read_reset(&self) -> String {
        UPS_READ_RESET.to_string()
    }
    fn write_timeout(&self) -> String {
        UPS_WRITE_TIMEOUT.to_string()
    }
    fn read_timeout(&self) -> String {
        UPS_READ_TIMEOUT.to_string()
    }
    fn write_err(&self) -> String {
        UPS_WRITE_ERR.to_string()
    }
    fn read_err(&self) -> String {
        UPS_READ_ERR.to_string()
    }
}

pub struct StreamInfo {
    pub ssl_domain: Option<String>,
    pub local_domain: Option<String>,
    pub remote_domain: Option<String>,
    pub local_protocol_name: String,
    pub upstream_protocol_name: Option<String>,
    pub local_addr: SocketAddr,
    pub remote_addr: SocketAddr,
    pub is_max_connect: bool,
    pub session_time: f32,
    pub protocol_hello: Option<AnyproxyHello>,
    pub err_status: ErrStatus,
    pub err_status_str: Option<String>,
    pub client_stream_info: std::sync::Arc<std::sync::Mutex<StreamFlowInfo>>,
    pub upstream_addr: Option<String>,
    pub upstream_connect_info: Rc<RefCell<StreamFlowInfo>>,
    pub upstream_connect_time: Option<f32>,
    pub upstream_stream_info: std::sync::Arc<std::sync::Mutex<StreamFlowInfo>>,
    pub stream_work_times: Vec<(String, f32)>,
    pub stream_work_time: Option<Instant>,
    pub is_discard_flow: bool,
    pub is_ebpf: bool,
}

impl StreamInfo {
    pub fn new(
        local_protocol_name: String,
        local_addr: SocketAddr,
        remote_addr: SocketAddr,
    ) -> StreamInfo {
        StreamInfo {
            ssl_domain: None,
            local_domain: None,
            remote_domain: None,
            local_protocol_name,
            upstream_protocol_name: None,
            local_addr,
            remote_addr,
            is_max_connect: false,
            session_time: 0.0,
            protocol_hello: None,
            err_status: ErrStatus::ClientProtoErr,
            err_status_str: None,
            client_stream_info: std::sync::Arc::new(std::sync::Mutex::new(StreamFlowInfo {
                write: 0,
                read: 0,
                err: StreamFlowErr::Init,
            })),
            upstream_addr: None,
            upstream_connect_info: Rc::new(RefCell::new(StreamFlowInfo {
                write: 0,
                read: 0,
                err: StreamFlowErr::Init,
            })),
            upstream_connect_time: None,
            upstream_stream_info: std::sync::Arc::new(std::sync::Mutex::new(StreamFlowInfo {
                write: 0,
                read: 0,
                err: StreamFlowErr::Init,
            })),
            stream_work_times: Vec::new(),
            stream_work_time: Some(Instant::now()),
            is_discard_flow: false,
            is_ebpf: false,
        }
    }
}
