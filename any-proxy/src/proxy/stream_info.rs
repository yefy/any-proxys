use super::StreamConfigContext;
use crate::protopack::anyproxy::AnyproxyHello;
use crate::stream::connect::ConnectInfo;
use crate::stream::server::ServerStreamInfo;
use crate::stream::stream_flow::StreamFlowInfo;
use crate::{Protocol7, Protocol77};
use any_base::executor_local_spawn::ExecutorsLocal;
use any_base::typ::{ArcMutex, OptionExt, Share, ShareRw};
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

use crate::proxy::StreamStreamContext;
use any_base::stream_flow::StreamFlowErr;
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

use crate::proxy::http_proxy::http_stream_request::HttpStreamRequest;
use any_base::stream_flow;

pub struct ErrStatusInfo {
    pub err: StreamFlowErr,
    pub err_status_200: Box<dyn ErrStatus200>,
    pub is_close: bool,
    pub err_str: Option<ArcString>,
    pub is_ups_close: bool,
}

impl ErrStatusInfo {
    pub fn new(err: StreamFlowErr, err_status_200: Box<dyn ErrStatus200>) -> ErrStatusInfo {
        let (is_close, err_str, is_ups_close) = Self::err(&err, &err_status_200);
        ErrStatusInfo {
            err,
            err_status_200,
            is_close,
            err_str,
            is_ups_close,
        }
    }

    pub fn err(
        err: &StreamFlowErr,
        err_status_200: &Box<dyn ErrStatus200>,
    ) -> (bool, Option<ArcString>, bool) {
        if err == &stream_flow::StreamFlowErr::WriteClose {
            (
                true,
                Some(err_status_200.write_close()),
                err_status_200.is_ups_err(),
            )
        } else if err == &stream_flow::StreamFlowErr::ReadClose {
            (
                true,
                Some(err_status_200.read_close()),
                err_status_200.is_ups_err(),
            )
        } else if err == &stream_flow::StreamFlowErr::WriteReset {
            (
                true,
                Some(err_status_200.write_reset()),
                err_status_200.is_ups_err(),
            )
        } else if err == &stream_flow::StreamFlowErr::ReadReset {
            (
                true,
                Some(err_status_200.read_reset()),
                err_status_200.is_ups_err(),
            )
        } else if err == &stream_flow::StreamFlowErr::WriteTimeout {
            (false, Some(err_status_200.write_timeout()), false)
        } else if err == &stream_flow::StreamFlowErr::ReadTimeout {
            (false, Some(err_status_200.read_timeout()), false)
        } else if err == &stream_flow::StreamFlowErr::WriteErr {
            (false, Some(err_status_200.write_err()), false)
        } else if err == &stream_flow::StreamFlowErr::ReadErr {
            (false, Some(err_status_200.read_err()), false)
        } else {
            (true, None, false)
        }
    }
}

pub struct StreamInfo {
    pub executors: Option<ExecutorsLocal>,
    pub server_stream_info: Arc<ServerStreamInfo>,
    pub client_protocol7: Option<Protocol7>,
    pub ssl_domain: Option<ArcString>,
    pub local_domain: Option<ArcString>,
    pub remote_domain: Option<ArcString>,
    pub session_time: f32,
    pub debug_is_open_print: bool,
    pub request_id: ArcString,
    pub protocol_hello: OptionExt<Arc<AnyproxyHello>>,
    pub upstream_protocol_hello_size: usize,
    pub is_err: bool,
    pub err_status: ErrStatus,
    pub err_status_str: Option<ArcString>,
    pub client_stream_flow_info: ArcMutex<StreamFlowInfo>,
    pub upstream_connect_flow_info: ArcMutex<StreamFlowInfo>,
    pub upstream_stream_flow_info: ArcMutex<StreamFlowInfo>,
    pub stream_work_times: Vec<(String, f32)>,
    pub stream_work_times2: Vec<(bool, String, f32)>,
    pub stream_work_time: Option<Instant>,
    pub is_discard_flow: bool,
    pub is_open_ebpf: bool,
    pub open_sendfile: Option<String>,
    pub ups_balancer: Option<ArcString>,
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
    pub client_protocol77: Option<Protocol77>,
    pub upstream_protocol77: Option<Protocol77>,
    pub debug_print_access_log_time: u64,
    pub debug_print_stream_flow_time: u64,
    pub stream_so_singer_time: usize,
    pub ssc_download: OptionExt<Arc<StreamStreamContext>>,
    pub ssc_upload: OptionExt<Arc<StreamStreamContext>>,
    pub session_id: u64,
    pub http_r: OptionExt<Arc<HttpStreamRequest>>,
}

impl StreamInfo {
    pub fn new(
        server_stream_info: Arc<ServerStreamInfo>,
        debug_is_open_stream_work_times: bool,
        executors: Option<ExecutorsLocal>,
        debug_print_access_log_time: u64,
        debug_print_stream_flow_time: u64,
        stream_so_singer_time: usize,
        debug_is_open_print: bool,
        session_id: u64,
    ) -> StreamInfo {
        StreamInfo {
            executors,
            server_stream_info,
            client_protocol7: None,
            ssl_domain: None,
            local_domain: None,
            remote_domain: None,
            session_time: 0.0,
            debug_is_open_print,
            request_id: ArcString::default(),
            protocol_hello: OptionExt::default(),
            upstream_protocol_hello_size: 0,
            is_err: false,
            err_status: ErrStatus::ClientProtoErr,
            err_status_str: None,
            client_stream_flow_info: ArcMutex::new(StreamFlowInfo::new()),
            upstream_connect_flow_info: ArcMutex::new(StreamFlowInfo::new()),
            upstream_stream_flow_info: ArcMutex::new(StreamFlowInfo::new()),
            stream_work_times: Vec::new(),
            stream_work_times2: Vec::new(),
            stream_work_time: Some(Instant::now()),
            is_discard_flow: false,
            is_open_ebpf: false,
            open_sendfile: None,
            ups_balancer: None,
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
            client_protocol77: None,
            upstream_protocol77: None,
            debug_print_access_log_time,
            debug_print_stream_flow_time,
            stream_so_singer_time,
            ssc_download: OptionExt::default(),
            ssc_upload: OptionExt::default(),
            session_id,
            http_r: None.into(),
        }
    }

    pub fn add_work_time1(&mut self, name: &str) {
        if self.debug_is_open_stream_work_times {
            let stream_work_time = self
                .stream_work_time
                .as_ref()
                .unwrap()
                .elapsed()
                .as_secs_f32();
            self.stream_work_times
                .push((name.to_string(), stream_work_time));
        }
    }

    pub fn add_work_time2(&mut self, is_client: bool, name: &str) {
        if self.debug_is_open_stream_work_times {
            let stream_work_time = self
                .stream_work_time
                .as_ref()
                .unwrap()
                .elapsed()
                .as_secs_f32();
            self.stream_work_times2
                .push((is_client, name.to_string(), stream_work_time));
        }
    }
}
