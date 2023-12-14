use super::StreamConfigContext;
use crate::protopack::anyproxy::AnyproxyHello;
use crate::stream::connect::ConnectInfo;
use crate::stream::server::ServerStreamInfo;
use crate::stream::stream_flow::StreamFlowInfo;
use crate::Protocol77;
use any_base::executor_local_spawn::ExecutorsLocal;
use any_base::future_wait::FutureWait;
use any_base::typ::{ArcMutex, Share, ShareRw};
use any_base::util::ArcString;
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
use lazy_static::lazy_static;
lazy_static! {
pub static ref CLI_WRITE_CLOSE: ArcString = "cli_write_close".into();
pub static ref CLI_READ_CLOSE: ArcString = "cli_read_close".into();
pub static ref CLI_WRITE_RESET: ArcString = "cli_write_reset".into();
pub static ref CLI_READ_RESET: ArcString = "cli_read_reset".into();
pub static ref CLI_WRITE_TIMEOUT: ArcString = "cli_write_timeout".into();
pub static ref CLI_READ_TIMEOUT: ArcString = "cli_read_timeout".into();
pub static ref CLI_WRITE_ERR: ArcString = "cli_write_err".into();
pub static ref CLI_READ_ERR: ArcString = "cli_read_err".into();

pub static ref UPS_WRITE_CLOSE: ArcString = "ups_write_close".into();
pub static ref UPS_READ_CLOSE: ArcString = "ups_read_close".into();
pub static ref UPS_WRITE_RESET: ArcString = "ups_write_reset".into();
pub static ref UPS_READ_RESET: ArcString = "ups_read_reset".into();
pub static ref UPS_WRITE_TIMEOUT: ArcString = "ups_write_timeout".into();
pub static ref UPS_READ_TIMEOUT: ArcString = "ups_read_timeout".into();
pub static ref UPS_WRITE_ERR: ArcString = "ups_write_err".into();
pub static ref UPS_READ_ERR: ArcString = "ups_read_err".into();

/// 503 对应的详细错误
//503
pub static ref UPS_CONN_RESET: ArcString = "ups_conn_reset".into();
pub static ref UPS_CONN_ERR: ArcString = "ups_conn_err".into();

}

pub trait ErrStatus200 {
    fn write_close(&self) -> ArcString;
    fn read_close(&self) -> ArcString;
    fn write_reset(&self) -> ArcString;
    fn read_reset(&self) -> ArcString;
    fn write_timeout(&self) -> ArcString;
    fn read_timeout(&self) -> ArcString;
    fn write_err(&self) -> ArcString;
    fn read_err(&self) -> ArcString;
    fn is_ups_err(&self) -> bool;
}

pub struct ErrStatusClient {}

impl ErrStatus200 for ErrStatusClient {
    fn write_close(&self) -> ArcString {
        CLI_WRITE_CLOSE.clone()
    }
    fn read_close(&self) -> ArcString {
        CLI_READ_CLOSE.clone()
    }
    fn write_reset(&self) -> ArcString {
        CLI_WRITE_RESET.clone()
    }
    fn read_reset(&self) -> ArcString {
        CLI_READ_RESET.clone()
    }
    fn write_timeout(&self) -> ArcString {
        CLI_WRITE_TIMEOUT.clone()
    }
    fn read_timeout(&self) -> ArcString {
        CLI_READ_TIMEOUT.clone()
    }
    fn write_err(&self) -> ArcString {
        CLI_WRITE_ERR.clone()
    }
    fn read_err(&self) -> ArcString {
        CLI_READ_ERR.clone()
    }
    fn is_ups_err(&self) -> bool {
        false
    }
}

pub struct ErrStatusUpstream {}

impl ErrStatus200 for ErrStatusUpstream {
    fn write_close(&self) -> ArcString {
        UPS_WRITE_CLOSE.clone()
    }
    fn read_close(&self) -> ArcString {
        UPS_READ_CLOSE.clone()
    }
    fn write_reset(&self) -> ArcString {
        UPS_WRITE_RESET.clone()
    }
    fn read_reset(&self) -> ArcString {
        UPS_READ_RESET.clone()
    }
    fn write_timeout(&self) -> ArcString {
        UPS_WRITE_TIMEOUT.clone()
    }
    fn read_timeout(&self) -> ArcString {
        UPS_READ_TIMEOUT.clone()
    }
    fn write_err(&self) -> ArcString {
        UPS_WRITE_ERR.clone()
    }
    fn read_err(&self) -> ArcString {
        UPS_READ_ERR.clone()
    }
    fn is_ups_err(&self) -> bool {
        true
    }
}

pub struct StreamInfo {
    pub executors: Option<ExecutorsLocal>,
    pub server_stream_info: Arc<ServerStreamInfo>,
    pub ssl_domain: Option<ArcString>,
    pub local_domain: Option<ArcString>,
    pub remote_domain: Option<ArcString>,
    pub session_time: f32,
    pub debug_is_open_print: bool,
    pub request_id: ArcString,
    pub protocol_hello: ArcMutex<Arc<AnyproxyHello>>,
    pub protocol_hello_size: usize,
    pub err_status: ErrStatus,
    pub err_status_str: Option<ArcString>,
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
    pub client_protocol77: Option<Protocol77>,
    pub upstream_protocol77: Option<Protocol77>,
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
            request_id: ArcString::default(),
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
            client_protocol77: None,
            upstream_protocol77: None,
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
