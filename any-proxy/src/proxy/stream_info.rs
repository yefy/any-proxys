use super::StreamConfigContext;
use crate::protopack::anyproxy::AnyproxyHello;
use crate::stream::connect::ConnectInfo;
use crate::stream::server::ServerStreamInfo;
use crate::stream::stream_flow::StreamFlowInfo;
use crate::Protocol77;
use any_base::executor_local_spawn::ExecutorsLocal;
use any_base::future_wait::FutureWait;
use any_base::typ::{ArcMutex, Share, ShareRw};
use std::sync::Arc;
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
    fn is_ups_err(&self) -> bool;
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
    fn is_ups_err(&self) -> bool {
        false
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
    fn is_ups_err(&self) -> bool {
        true
    }
}

pub struct StreamInfo {
    pub executors: Option<ExecutorsLocal>,
    pub server_stream_info: Arc<ServerStreamInfo>,
    pub ssl_domain: Option<String>,
    pub local_domain: Option<String>,
    pub remote_domain: Option<String>,
    pub session_time: f32,
    pub debug_is_open_print: bool,
    pub request_id: String,
    pub protocol_hello: ArcMutex<Arc<AnyproxyHello>>,
    pub protocol_hello_size: usize,
    pub err_status: ErrStatus,
    pub err_status_str: Option<String>,
    pub client_stream_flow_info: ArcMutex<StreamFlowInfo>,
    pub upstream_connect_flow_info: ArcMutex<StreamFlowInfo>,
    pub upstream_stream_flow_info: ArcMutex<StreamFlowInfo>,
    pub stream_work_times: Vec<(String, f32)>,
    pub stream_work_time: Option<Instant>,
    pub is_discard_flow: bool,
    pub is_open_ebpf: bool,
    pub open_sendfile: Option<String>,
    pub ups_dispatch: Option<String>,
    pub is_proxy_protocol_hello: bool,
    pub scc: ShareRw<StreamConfigContext>,
    pub is_discard_timeout: bool,
    pub buffer_cache: Option<Arc<String>>,
    pub upstream_connect_info: Share<ConnectInfo>,
    pub debug_is_open_stream_work_times: bool,
    pub close_num: usize,
    pub write_max_block_time_ms: u128,
    pub client_protocol_hello_size: usize,
    pub is_timeout_exit: bool,
    pub total_read_size: u64,
    pub total_write_size: u64,
    pub protocol77: Option<Protocol77>,
    pub upload_read: FutureWait,
    pub download_read: FutureWait,
    pub upload_close: FutureWait,
    pub download_close: FutureWait,
}

impl StreamInfo {
    pub fn new(
        server_stream_info: Arc<ServerStreamInfo>,
        debug_is_open_stream_work_times: bool,
        executors: Option<ExecutorsLocal>,
    ) -> StreamInfo {
        StreamInfo {
            executors,
            server_stream_info,
            ssl_domain: None,
            local_domain: None,
            remote_domain: None,
            session_time: 0.0,
            debug_is_open_print: false,
            request_id: "".to_string(),
            protocol_hello: ArcMutex::default(),
            protocol_hello_size: 0,
            err_status: ErrStatus::ClientProtoErr,
            err_status_str: None,
            client_stream_flow_info: ArcMutex::new(StreamFlowInfo::new()),
            upstream_connect_flow_info: ArcMutex::new(StreamFlowInfo::new()),
            upstream_stream_flow_info: ArcMutex::new(StreamFlowInfo::new()),
            stream_work_times: Vec::new(),
            stream_work_time: Some(Instant::now()),
            is_discard_flow: false,
            is_open_ebpf: false,
            open_sendfile: None,
            ups_dispatch: None,
            is_proxy_protocol_hello: false,
            scc: ShareRw::default(),
            is_discard_timeout: false,
            buffer_cache: None,
            upstream_connect_info: Share::default(),
            debug_is_open_stream_work_times,
            close_num: 0,
            write_max_block_time_ms: 0,
            client_protocol_hello_size: 0,
            is_timeout_exit: false,
            total_read_size: 0,
            total_write_size: 0,
            protocol77: None,
            upload_read: FutureWait::new(),
            download_read: FutureWait::new(),
            upload_close: FutureWait::new(),
            download_close: FutureWait::new(),
        }
    }

    pub fn add_work_time(&mut self, name: &str) {
        if self.debug_is_open_stream_work_times {
            let stream_work_time = self
                .stream_work_time
                .as_ref()
                .unwrap()
                .elapsed()
                .as_secs_f32();
            self.stream_work_times
                .push((name.to_string(), stream_work_time));
            self.stream_work_time = Some(Instant::now());
        }
    }
}
