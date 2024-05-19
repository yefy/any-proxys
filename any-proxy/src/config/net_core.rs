use crate::config as conf;
use crate::config::net_core_plugin::PluginHandleStream;
use crate::proxy::http_proxy::http_context::HttpContext;
use crate::proxy::StreamCloseType;
use any_base::file_ext::{FileExt, FileExtFix, FileUniq};
use any_base::module::module;
use any_base::typ;
use any_base::typ::{ArcMutex, ArcMutexTokio, ArcRwLock, ArcRwLockTokio, ArcUnsafeAny};
use any_base::util::ArcString;
use anyhow::anyhow;
use anyhow::Result;
use lazy_static::lazy_static;
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
#[cfg(feature = "anyio-file")]
use std::time::Instant;

lazy_static! {
    pub static ref HTTP_CONTEXT: Arc<HttpContext> = Arc::new(HttpContext::new());
}

lazy_static! {
    pub static ref TMP_FILE_CACHES: ArcMutex<VecDeque<Arc<FileExt>>> =
        ArcMutex::new(VecDeque::with_capacity(300));
}
lazy_static! {
    pub static ref IS_OPEN_TMP_FILE_CACHE: Arc<ArcMutex<bool>> = Arc::new(ArcMutex::new(false));
}

lazy_static! {
    pub static ref TMP_FILE_CACHE_SIZE: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
}

fn default_domain_from_http_v1_is_open() -> bool {
    false
}
fn default_domain_from_http_v1_check_methods() -> HashMap<String, bool> {
    HashMap::new()
}
use crate::Deserialize;
use crate::Serialize;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DomainFromHttpV1 {
    #[serde(default = "default_domain_from_http_v1_is_open")]
    pub is_open: bool,
    #[serde(default = "default_domain_from_http_v1_check_methods")]
    pub check_methods: HashMap<String, bool>,
}

impl DomainFromHttpV1 {
    pub fn new() -> Self {
        DomainFromHttpV1 {
            is_open: default_domain_from_http_v1_is_open(),
            check_methods: default_domain_from_http_v1_check_methods(),
        }
    }
}

pub struct ConfStream {
    pub buffer_cache: Arc<String>,
    pub stream_cache_size: i64,
    pub is_open_stream_cache: bool,
    pub tmp_file_size: i64,
    pub tmp_file_reopen_size: i64,
    pub limit_rate_after: i64,
    pub max_limit_rate: usize,
    pub max_stream_cache_size: i64,
    pub max_tmp_file_size: i64,
    pub is_tmp_file_io_page: bool,
    pub page_size: usize,
    pub min_cache_buffer_size: usize,
    pub min_read_buffer_size: usize,
    pub min_cache_file_size: usize,
    pub plugin_handle_stream: ArcRwLockTokio<PluginHandleStream>,
    pub read_buffer_size: usize,
    pub write_buffer_size: usize,
    pub stream_delay_mil_time: u64,
    pub stream_nopush: bool,
    pub stream_nodelay_size: i64,
    pub sendfile_max_write_size: usize,
}

impl ConfStream {
    pub fn new(
        tmp_file_size: u64,
        tmp_file_reopen_size: u64,
        limit_rate_after: u64,
        limit_rate: u64,
        stream_cache_size: usize,
        is_open_stream_cache: bool,
        is_tmp_file_io_page: bool,
        read_buffer_page_size: usize,
        mut write_buffer_page_size: usize,
        stream_delay_mil_time: u64,
        stream_nopush: bool,
        stream_nodelay_size: i64,
        sendfile_max_write_size: usize,
    ) -> Self {
        use crate::util::default_config;

        let buffer_cache = if is_open_stream_cache {
            if tmp_file_size > 0 {
                "file".to_string()
            } else {
                "cache".to_string()
            }
        } else {
            "memory".to_string()
        };

        if write_buffer_page_size < read_buffer_page_size {
            write_buffer_page_size = read_buffer_page_size;
        }

        let page_size = default_config::PAGE_SIZE.load(Ordering::Relaxed);
        let mut ssc = ConfStream {
            buffer_cache: Arc::new(buffer_cache),
            stream_cache_size: stream_cache_size as i64,
            is_open_stream_cache,
            tmp_file_size: tmp_file_size as i64,
            tmp_file_reopen_size: tmp_file_reopen_size as i64,
            limit_rate_after: limit_rate_after as i64,
            max_limit_rate: limit_rate as usize,
            max_stream_cache_size: stream_cache_size as i64,
            max_tmp_file_size: tmp_file_size as i64,
            is_tmp_file_io_page,
            page_size,
            min_cache_buffer_size: page_size * 4 * 8,
            min_read_buffer_size: page_size * 2,
            min_cache_file_size: page_size * 1024,
            plugin_handle_stream: ArcRwLockTokio::default(),
            read_buffer_size: read_buffer_page_size * page_size,
            write_buffer_size: write_buffer_page_size * page_size,
            stream_delay_mil_time,
            stream_nopush,
            stream_nodelay_size,
            sendfile_max_write_size,
        };

        if ssc.read_buffer_size < ssc.min_read_buffer_size {
            ssc.read_buffer_size = ssc.min_read_buffer_size;
        }

        if ssc.write_buffer_size < ssc.read_buffer_size {
            ssc.write_buffer_size = ssc.read_buffer_size;
        }

        if ssc.stream_cache_size < ssc.min_cache_buffer_size as i64 {
            ssc.stream_cache_size = ssc.min_cache_buffer_size as i64;
            ssc.max_stream_cache_size = ssc.min_cache_buffer_size as i64;
        }

        if ssc.tmp_file_size > 0 && ssc.tmp_file_size < ssc.min_cache_file_size as i64 {
            ssc.tmp_file_size = ssc.min_cache_file_size as i64;
            ssc.max_tmp_file_size = ssc.min_cache_file_size as i64;
        }

        if ssc.stream_cache_size > 0 {
            let stream_cache_size =
                (ssc.stream_cache_size / ssc.page_size as i64 + 1) * ssc.page_size as i64;
            ssc.stream_cache_size = stream_cache_size;
            ssc.max_stream_cache_size = stream_cache_size;
        }

        if ssc.tmp_file_size > 0 {
            let tmp_file_size =
                (ssc.tmp_file_size / ssc.page_size as i64 + 1) * ssc.page_size as i64;
            ssc.tmp_file_size = tmp_file_size;
            ssc.max_tmp_file_size = tmp_file_size;
        }

        ssc
    }
}

pub struct Conf {
    pub stream_cache_size: usize,
    pub is_upload_open_stream_cache: bool,
    pub is_download_open_stream_cache: bool,
    pub debug_is_open_stream_work_times: bool,
    pub debug_print_access_log_time: u64,
    pub debug_print_stream_flow_time: u64,
    pub is_tmp_file_io_page: bool,
    pub stream_so_singer_time: usize,
    pub download_limit_rate_after: u64,
    pub download_limit_rate: u64,
    pub upload_limit_rate_after: u64,
    pub upload_limit_rate: u64,
    pub download_tmp_file_size: u64,
    pub upload_tmp_file_size: u64,
    pub download_tmp_file_reopen_size: u64,
    pub upload_tmp_file_reopen_size: u64,
    pub debug_is_open_print: bool,
    pub is_open_sendfile: bool,
    pub sendfile_max_write_size: usize,
    pub tcp_config_name: String,
    pub quic_config_name: String,
    pub is_proxy_protocol_hello: bool,
    pub domain: ArcString,
    pub is_open_ebpf: bool,
    pub is_disable_share_http_context: bool,
    pub http_context: Arc<HttpContext>,
    pub download: Arc<ConfStream>,
    pub upload: Arc<ConfStream>,
    pub read_buffer_page_size: usize,
    pub write_buffer_page_size: usize,
    pub is_port_direct_ebpf: bool,
    pub client_timeout_mil_time_ebpf: u64,
    pub upstream_timeout_mil_time_ebpf: u64,
    pub close_type: StreamCloseType,
    pub stream_delay_mil_time: u64,
    pub stream_nopush: bool,
    pub stream_nodelay_size: i64,
    pub directio: u64,
    pub tmp_file_cache_size: usize,
    pub expires: usize,
    pub domain_from_http_v1: Arc<DomainFromHttpV1>,
}
const STREAM_CACHE_SIZE_DEFAULT: usize = 131072;
const STREAM_SO_SINGER_TIME_DEFAULT: usize = 0;
pub const TCP_CONFIG_NAME_DEFAULT: &str = "tcp_config_default";
pub const QUIC_CONFIG_NAME_DEFAULT: &str = "quic_config_default";
const IS_TMP_FILE_IO_PAGE_DEFAULT: bool = true;
const READ_BUFFER_PAGE_SIZE_DEFAULT: usize = 8;
const WRITE_BUFFER_PAGE_SIZE_DEFAULT: usize = 16;
const CLIENT_TIMEOUT_MIL_TIME_EBPF_DEFAULT: u64 = 120000;
const UPSTREAM_TIMEOUT_MIL_TIME_EBPF_DEFAULT: u64 = 2000;
const CLOSE_TYPE_DEFAULT: StreamCloseType = StreamCloseType::Shutdown;
const CLOSE_TYPE_FAST: &str = "fast";
const CLOSE_TYPE_SHUTDOWM: &str = "shutdown";
const CLOSE_TYPE_WAIT_EMPTY: &str = "wait_empty";
const CLOSE_TYPE_ALL: &str = "fast shutdown wait_empty";
const IS_OPEN_STREAM_CACHE: bool = true;
const STREAM_MAX_DELAY_MIL_TIME_DEFAULT: u64 = 200;
const STREAM_NODELAY_SIZE_DEFAULT: i64 = 8192;
const SENDFILE_MAX_WRITE_SIZE_DEFAULT: usize = 1048576;

impl Conf {
    pub fn new() -> Self {
        Conf {
            stream_cache_size: STREAM_CACHE_SIZE_DEFAULT,
            is_upload_open_stream_cache: IS_OPEN_STREAM_CACHE,
            is_download_open_stream_cache: IS_OPEN_STREAM_CACHE,
            debug_is_open_stream_work_times: false,
            debug_print_access_log_time: 0,
            debug_print_stream_flow_time: 0,
            is_tmp_file_io_page: IS_TMP_FILE_IO_PAGE_DEFAULT,
            stream_so_singer_time: STREAM_SO_SINGER_TIME_DEFAULT,
            download_limit_rate_after: 0,
            download_limit_rate: 0,
            upload_limit_rate_after: 0,
            upload_limit_rate: 0,
            download_tmp_file_size: 0,
            upload_tmp_file_size: 0,
            download_tmp_file_reopen_size: 0,
            upload_tmp_file_reopen_size: 0,
            debug_is_open_print: false,
            is_open_sendfile: false,
            sendfile_max_write_size: SENDFILE_MAX_WRITE_SIZE_DEFAULT,
            tcp_config_name: TCP_CONFIG_NAME_DEFAULT.to_string(),
            quic_config_name: QUIC_CONFIG_NAME_DEFAULT.to_string(),
            is_proxy_protocol_hello: false,
            domain: ArcString::default(),
            is_open_ebpf: false,
            http_context: HTTP_CONTEXT.clone(),
            is_disable_share_http_context: false,
            download: Arc::new(ConfStream::new(
                1, 0, 1, 1, 1, false, true, 2, 2, 0, false, 8192, 0,
            )),
            upload: Arc::new(ConfStream::new(
                1, 0, 1, 1, 1, false, true, 2, 2, 0, false, 8192, 0,
            )),
            read_buffer_page_size: READ_BUFFER_PAGE_SIZE_DEFAULT,
            write_buffer_page_size: WRITE_BUFFER_PAGE_SIZE_DEFAULT,
            is_port_direct_ebpf: true,
            client_timeout_mil_time_ebpf: CLIENT_TIMEOUT_MIL_TIME_EBPF_DEFAULT,
            upstream_timeout_mil_time_ebpf: UPSTREAM_TIMEOUT_MIL_TIME_EBPF_DEFAULT,
            close_type: CLOSE_TYPE_DEFAULT,
            stream_delay_mil_time: 0,
            stream_nopush: false,
            stream_nodelay_size: STREAM_NODELAY_SIZE_DEFAULT,
            directio: 0,
            tmp_file_cache_size: 100,
            expires: 0,
            domain_from_http_v1: Arc::new(DomainFromHttpV1::new()),
        }
    }
}

lazy_static! {
    pub static ref MODULE_CMDS: Arc<Vec<module::Cmd>> = Arc::new(vec![
        module::Cmd {
            name: "stream_cache_size".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(stream_cache_size(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "is_upload_open_stream_cache".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(is_upload_open_stream_cache(
                ms, conf_arg, cmd, conf
            )),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "is_download_open_stream_cache".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(is_download_open_stream_cache(
                ms, conf_arg, cmd, conf
            )),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "debug_is_open_stream_work_times".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(debug_is_open_stream_work_times(
                ms, conf_arg, cmd, conf
            )),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "debug_print_access_log_time".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(debug_print_access_log_time(
                ms, conf_arg, cmd, conf
            )),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "debug_print_stream_flow_time".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(debug_print_stream_flow_time(
                ms, conf_arg, cmd, conf
            )),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "is_tmp_file_io_page".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(is_tmp_file_io_page(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "stream_so_singer_time".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(stream_so_singer_time(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "download_limit_rate_after".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(download_limit_rate_after(
                ms, conf_arg, cmd, conf
            )),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "download_limit_rate".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(download_limit_rate(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "upload_limit_rate_after".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(upload_limit_rate_after(
                ms, conf_arg, cmd, conf
            )),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "upload_limit_rate".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(upload_limit_rate(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "download_tmp_file_size".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(download_tmp_file_size(
                ms, conf_arg, cmd, conf
            )),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "upload_tmp_file_size".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(upload_tmp_file_size(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "download_tmp_file_reopen_size".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(download_tmp_file_reopen_size(
                ms, conf_arg, cmd, conf
            )),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "upload_tmp_file_reopen_size".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(upload_tmp_file_reopen_size(
                ms, conf_arg, cmd, conf
            )),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "debug_is_open_print".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(debug_is_open_print(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "is_open_sendfile".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(is_open_sendfile(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "sendfile_max_write_size".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(sendfile_max_write_size(
                ms, conf_arg, cmd, conf
            )),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "tcp_config_name".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(tcp_config_name(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_SERVER,
        },
        module::Cmd {
            name: "quic_config_name".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(quic_config_name(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_SERVER,
        },
        module::Cmd {
            name: "is_proxy_protocol_hello".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(is_proxy_protocol_hello(
                ms, conf_arg, cmd, conf
            )),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "domain".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(domain(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_SERVER
        },
        module::Cmd {
            name: "is_open_ebpf".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(is_open_ebpf(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "is_disable_share_http_context".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(is_disable_share_http_context(
                ms, conf_arg, cmd, conf
            )),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "read_buffer_page_size".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(read_buffer_page_size(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "write_buffer_page_size".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(write_buffer_page_size(
                ms, conf_arg, cmd, conf
            )),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "is_port_direct_ebpf".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(is_port_direct_ebpf(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "client_timeout_mil_time_ebpf".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(client_timeout_mil_time_ebpf(
                ms, conf_arg, cmd, conf
            )),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "upstream_timeout_mil_time_ebpf".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(upstream_timeout_mil_time_ebpf(
                ms, conf_arg, cmd, conf
            )),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "close_type".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(close_type(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "stream_delay_mil_time".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(stream_delay_mil_time(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "stream_nopush".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(stream_nopush(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "stream_nodelay_size".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(stream_nodelay_size(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "directio".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(directio(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "tmp_file_cache_size".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(tmp_file_cache_size(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN,
        },
        module::Cmd {
            name: "expires".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(expires(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "domain_from_http_v1".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(domain_from_http_v1(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_SERVER,
        },
    ]);
}

lazy_static! {
    pub static ref MODULE_FUNC: Arc<module::Func> = Arc::new(module::Func {
        create_conf: |ms| Box::pin(create_conf(ms)),
        merge_conf: |ms, parent_conf, child_conf| Box::pin(merge_conf(ms, parent_conf, child_conf)),

        init_conf: |ms, parent_conf, child_conf| Box::pin(init_conf(ms, parent_conf, child_conf)),
        merge_old_conf: |old_ms, old_main_conf, old_conf, ms, main_conf, conf| Box::pin(
            merge_old_conf(old_ms, old_main_conf, old_conf, ms, main_conf, conf)
        ),
        init_master_thread: None,
        init_work_thread: None,
        drop_conf: None,
    });
}

lazy_static! {
    pub static ref M: typ::ArcRwLock<module::Module> = typ::ArcRwLock::new(module::Module {
        name: "net_core".to_string(),
        main_index: -1,
        ctx_index: -1,
        index: -1,
        ctx_index_len: -1,
        func: MODULE_FUNC.clone(),
        cmds: MODULE_CMDS.clone(),
        create_main_confs: None,
        init_main_confs: None,
        merge_old_main_confs: None,
        merge_confs: None,
        init_master_thread_confs: None,
        init_work_thread_confs: None,
        drop_confs: None,
        typ: conf::MODULE_TYPE_NET,
        create_server: None,
    });
}

pub fn module() -> typ::ArcRwLock<module::Module> {
    return M.clone();
}

pub async fn main_conf(ms: &module::Modules) -> &Conf {
    ms.get_main_conf::<Conf>(module()).await
}

pub async fn main_conf_mut(ms: &module::Modules) -> &mut Conf {
    ms.get_main_conf_mut::<Conf>(module()).await
}

pub async fn main_any_conf(ms: &module::Modules) -> ArcUnsafeAny {
    ms.get_main_any_conf(module()).await
}

pub fn curr_conf(curr: &ArcUnsafeAny) -> &Conf {
    module::Modules::get_curr_conf(curr, module())
}

pub fn curr_conf_mut(curr: &ArcUnsafeAny) -> &mut Conf {
    module::Modules::get_curr_conf_mut(curr, module())
}

pub fn currs_conf(curr: &Vec<ArcUnsafeAny>) -> &Conf {
    module::Modules::get_currs_conf(curr, module())
}

pub fn currs_conf_mut(curr: &Vec<ArcUnsafeAny>) -> &mut Conf {
    module::Modules::get_currs_conf_mut(curr, module())
}

pub fn curr_any_conf(curr: &ArcUnsafeAny) -> ArcUnsafeAny {
    module::Modules::get_curr_any_conf(curr, module())
}

pub fn currs_any_conf(curr: &Vec<ArcUnsafeAny>) -> ArcUnsafeAny {
    module::Modules::get_currs_any_conf(curr, module())
}

async fn create_conf(_ms: module::Modules) -> Result<typ::ArcUnsafeAny> {
    return Ok(typ::ArcUnsafeAny::new(Box::new(Conf::new())));
}

async fn merge_conf(
    ms: module::Modules,
    parent_conf: Option<typ::ArcUnsafeAny>,
    child_conf: typ::ArcUnsafeAny,
) -> Result<()> {
    if parent_conf.is_some() {
        let parent_conf = parent_conf.unwrap();
        let parent_conf = parent_conf.get_mut::<Conf>();
        let child_conf = child_conf.get_mut::<Conf>();
        if child_conf.stream_cache_size == STREAM_CACHE_SIZE_DEFAULT {
            child_conf.stream_cache_size = parent_conf.stream_cache_size;
        }
        if child_conf.is_upload_open_stream_cache == IS_OPEN_STREAM_CACHE {
            child_conf.is_upload_open_stream_cache = parent_conf.is_upload_open_stream_cache;
        }
        if child_conf.is_download_open_stream_cache == IS_OPEN_STREAM_CACHE {
            child_conf.is_download_open_stream_cache = parent_conf.is_download_open_stream_cache;
        }
        if child_conf.debug_is_open_stream_work_times == false {
            child_conf.debug_is_open_stream_work_times =
                parent_conf.debug_is_open_stream_work_times;
        }
        if child_conf.debug_print_access_log_time == 0 {
            child_conf.debug_print_access_log_time = parent_conf.debug_print_access_log_time;
        }
        if child_conf.debug_print_stream_flow_time == 0 {
            child_conf.debug_print_stream_flow_time = parent_conf.debug_print_stream_flow_time;
        }
        if child_conf.is_tmp_file_io_page == IS_TMP_FILE_IO_PAGE_DEFAULT {
            child_conf.is_tmp_file_io_page = parent_conf.is_tmp_file_io_page;
        }
        if child_conf.stream_so_singer_time == STREAM_SO_SINGER_TIME_DEFAULT {
            child_conf.stream_so_singer_time = parent_conf.stream_so_singer_time;
        }
        if child_conf.download_limit_rate_after == 0 {
            child_conf.download_limit_rate_after = parent_conf.download_limit_rate_after;
        }
        if child_conf.download_limit_rate == 0 {
            child_conf.download_limit_rate = parent_conf.download_limit_rate;
        }
        if child_conf.upload_limit_rate_after == 0 {
            child_conf.upload_limit_rate_after = parent_conf.upload_limit_rate_after;
        }
        if child_conf.upload_limit_rate == 0 {
            child_conf.upload_limit_rate = parent_conf.upload_limit_rate;
        }
        if child_conf.download_tmp_file_size == 0 {
            child_conf.download_tmp_file_size = parent_conf.download_tmp_file_size;
        }
        if child_conf.upload_tmp_file_size == 0 {
            child_conf.upload_tmp_file_size = parent_conf.upload_tmp_file_size;
        }
        if child_conf.download_tmp_file_reopen_size == 0 {
            child_conf.download_tmp_file_reopen_size = parent_conf.download_tmp_file_reopen_size;
        }
        if child_conf.upload_tmp_file_reopen_size == 0 {
            child_conf.upload_tmp_file_reopen_size = parent_conf.upload_tmp_file_reopen_size;
        }
        if child_conf.debug_is_open_print == false {
            child_conf.debug_is_open_print = parent_conf.debug_is_open_print;
        }
        if child_conf.is_open_sendfile == false {
            child_conf.is_open_sendfile = parent_conf.is_open_sendfile;
        }
        if child_conf.sendfile_max_write_size == SENDFILE_MAX_WRITE_SIZE_DEFAULT {
            child_conf.sendfile_max_write_size = parent_conf.sendfile_max_write_size;
        }
        if child_conf.tcp_config_name == TCP_CONFIG_NAME_DEFAULT
            && parent_conf.tcp_config_name.len() > 0
        {
            child_conf.tcp_config_name = parent_conf.tcp_config_name.clone();
        }
        if child_conf.quic_config_name == QUIC_CONFIG_NAME_DEFAULT
            && parent_conf.quic_config_name.len() > 0
        {
            child_conf.quic_config_name = parent_conf.quic_config_name.clone();
        }
        if child_conf.is_proxy_protocol_hello == false {
            child_conf.is_proxy_protocol_hello = parent_conf.is_proxy_protocol_hello;
        }
        if child_conf.domain == ArcString::default() {
            child_conf.domain = parent_conf.domain.clone();
        }
        if child_conf.is_open_ebpf == false {
            child_conf.is_open_ebpf = parent_conf.is_open_ebpf.clone();
        }

        if child_conf.is_disable_share_http_context == false {
            child_conf.is_disable_share_http_context =
                parent_conf.is_disable_share_http_context.clone();
        }

        if child_conf.read_buffer_page_size == READ_BUFFER_PAGE_SIZE_DEFAULT {
            child_conf.read_buffer_page_size = parent_conf.read_buffer_page_size.clone();
        }

        if child_conf.write_buffer_page_size == WRITE_BUFFER_PAGE_SIZE_DEFAULT {
            child_conf.write_buffer_page_size = parent_conf.write_buffer_page_size.clone();
        }

        if child_conf.is_port_direct_ebpf == true {
            child_conf.is_port_direct_ebpf = parent_conf.is_port_direct_ebpf.clone();
        }

        if child_conf.client_timeout_mil_time_ebpf == CLIENT_TIMEOUT_MIL_TIME_EBPF_DEFAULT {
            child_conf.client_timeout_mil_time_ebpf =
                parent_conf.client_timeout_mil_time_ebpf.clone();
        }

        if child_conf.upstream_timeout_mil_time_ebpf == UPSTREAM_TIMEOUT_MIL_TIME_EBPF_DEFAULT {
            child_conf.upstream_timeout_mil_time_ebpf =
                parent_conf.upstream_timeout_mil_time_ebpf.clone();
        }

        if child_conf.close_type == CLOSE_TYPE_DEFAULT {
            child_conf.close_type = parent_conf.close_type.clone();
        }

        if child_conf.stream_delay_mil_time == 0 {
            child_conf.stream_delay_mil_time = parent_conf.stream_delay_mil_time.clone();
        }

        if child_conf.stream_nopush == false {
            child_conf.stream_nopush = parent_conf.stream_nopush.clone();
        }
        if child_conf.stream_nodelay_size == STREAM_NODELAY_SIZE_DEFAULT {
            child_conf.stream_nodelay_size = parent_conf.stream_nodelay_size.clone();
        }
        if child_conf.directio == 0 {
            child_conf.directio = parent_conf.directio.clone();
        }
        if child_conf.expires == 0 {
            child_conf.expires = parent_conf.expires.clone();
        }

        if !child_conf.domain_from_http_v1.is_open {
            child_conf.domain_from_http_v1 = parent_conf.domain_from_http_v1.clone();
        }

        child_conf.download = Arc::new(ConfStream::new(
            child_conf.download_tmp_file_size,
            child_conf.download_tmp_file_reopen_size,
            child_conf.download_limit_rate_after,
            child_conf.download_limit_rate,
            child_conf.stream_cache_size,
            child_conf.is_download_open_stream_cache,
            child_conf.is_tmp_file_io_page,
            child_conf.read_buffer_page_size,
            child_conf.write_buffer_page_size,
            child_conf.stream_delay_mil_time,
            child_conf.stream_nopush,
            child_conf.stream_nodelay_size,
            child_conf.sendfile_max_write_size,
        ));

        child_conf.upload = Arc::new(ConfStream::new(
            child_conf.upload_tmp_file_size,
            child_conf.upload_tmp_file_reopen_size,
            child_conf.upload_limit_rate_after,
            child_conf.upload_limit_rate,
            child_conf.stream_cache_size,
            child_conf.is_upload_open_stream_cache,
            child_conf.is_tmp_file_io_page,
            child_conf.read_buffer_page_size,
            child_conf.write_buffer_page_size,
            child_conf.stream_delay_mil_time,
            child_conf.stream_nopush,
            child_conf.stream_nodelay_size,
            child_conf.sendfile_max_write_size,
        ));

        use super::net_core_plugin;
        let net_core_plugin_conf = net_core_plugin::main_conf(&ms).await;

        let plugin_handle_stream = if child_conf.is_download_open_stream_cache {
            net_core_plugin_conf.plugin_handle_stream_cache.clone()
        } else {
            net_core_plugin_conf.plugin_handle_stream_memory.clone()
        };
        child_conf
            .download
            .plugin_handle_stream
            .set(plugin_handle_stream.get().await.clone())
            .await;

        let plugin_handle_stream = if child_conf.is_upload_open_stream_cache {
            net_core_plugin_conf.plugin_handle_stream_cache.clone()
        } else {
            net_core_plugin_conf.plugin_handle_stream_memory.clone()
        };
        child_conf
            .upload
            .plugin_handle_stream
            .set(plugin_handle_stream.get().await.clone())
            .await;
    }
    return Ok(());
}

async fn merge_old_conf(
    _old_ms: Option<module::Modules>,
    _old_main_conf: Option<typ::ArcUnsafeAny>,
    _old_conf: Option<typ::ArcUnsafeAny>,
    _ms: module::Modules,
    _main_conf: typ::ArcUnsafeAny,
    _conf: typ::ArcUnsafeAny,
) -> Result<()> {
    return Ok(());
}

async fn init_conf(
    _ms: module::Modules,
    _main_confs: typ::ArcUnsafeAny,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    let tmp_file_cache_size = c.tmp_file_cache_size;
    if tmp_file_cache_size <= 0 {
        return Ok(());
    }

    {
        let is_open_tmp_file_cache = &mut *IS_OPEN_TMP_FILE_CACHE.get_mut();
        if *is_open_tmp_file_cache {
            return Ok(());
        }
        *is_open_tmp_file_cache = true;
    }

    TMP_FILE_CACHE_SIZE.store(tmp_file_cache_size, Ordering::SeqCst);

    log::info!("async_create_tmp_file_fd size:{}", tmp_file_cache_size);
    async_create_tmp_file_fd(tmp_file_cache_size);
    return Ok(());
}

async fn stream_cache_size(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.stream_cache_size = *conf_arg.value.get::<usize>();
    log::trace!(target: "main", "c.stream_cache_size:{:?}", c.stream_cache_size);
    return Ok(());
}

async fn is_upload_open_stream_cache(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.is_upload_open_stream_cache = *conf_arg.value.get::<bool>();
    log::trace!(target: "main",
        "c.is_upload_open_stream_cache:{:?}",
        c.is_upload_open_stream_cache
    );
    return Ok(());
}

async fn is_download_open_stream_cache(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.is_download_open_stream_cache = *conf_arg.value.get::<bool>();
    log::trace!(target: "main",
        "c.is_download_open_stream_cache:{:?}",
        c.is_download_open_stream_cache
    );
    return Ok(());
}

async fn debug_is_open_stream_work_times(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.debug_is_open_stream_work_times = *conf_arg.value.get::<bool>();
    log::trace!(target: "main",
        "c.debug_is_open_stream_work_times:{:?}",
        c.debug_is_open_stream_work_times
    );
    return Ok(());
}
async fn debug_print_access_log_time(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.debug_print_access_log_time = *conf_arg.value.get::<u64>();
    log::trace!(target: "main",
        "c.debug_print_access_log_time:{:?}",
        c.debug_print_access_log_time
    );
    return Ok(());
}
async fn debug_print_stream_flow_time(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.debug_print_stream_flow_time = *conf_arg.value.get::<u64>();

    log::trace!(target: "main",
        "c.debug_print_stream_flow_time:{:?}",
        c.debug_print_stream_flow_time
    );
    return Ok(());
}

async fn is_tmp_file_io_page(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.is_tmp_file_io_page = *conf_arg.value.get::<bool>();

    log::trace!(target: "main", "c.is_tmp_file_io_page:{:?}", c.is_tmp_file_io_page);
    return Ok(());
}

async fn stream_so_singer_time(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.stream_so_singer_time = *conf_arg.value.get::<usize>();

    log::trace!(target: "main", "c.stream_so_singer_time:{:?}", c.stream_so_singer_time);
    return Ok(());
}

async fn download_limit_rate_after(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.download_limit_rate_after = *conf_arg.value.get::<u64>();

    log::trace!(target: "main",
        "c.download_limit_rate_after:{:?}",
        c.download_limit_rate_after
    );
    return Ok(());
}

async fn download_limit_rate(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.download_limit_rate = *conf_arg.value.get::<u64>();

    log::trace!(target: "main", "c.download_limit_rate:{:?}", c.download_limit_rate);
    return Ok(());
}

async fn upload_limit_rate_after(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.upload_limit_rate_after = *conf_arg.value.get::<u64>();

    log::trace!(target: "main", "c.upload_limit_rate_after:{:?}", c.upload_limit_rate_after);
    return Ok(());
}

async fn upload_limit_rate(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.upload_limit_rate = *conf_arg.value.get::<u64>();

    log::trace!(target: "main", "c.upload_limit_rate:{:?}", c.upload_limit_rate);
    return Ok(());
}

async fn download_tmp_file_size(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.download_tmp_file_size = *conf_arg.value.get::<u64>();

    log::trace!(target: "main", "c.download_tmp_file_size:{:?}", c.download_tmp_file_size);
    return Ok(());
}
async fn upload_tmp_file_size(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.upload_tmp_file_size = *conf_arg.value.get::<u64>();

    log::trace!(target: "main", "c.upload_tmp_file_size:{:?}", c.upload_tmp_file_size);
    return Ok(());
}

async fn download_tmp_file_reopen_size(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.download_tmp_file_reopen_size = *conf_arg.value.get::<u64>();

    log::trace!(target: "main",
        "c.download_tmp_file_reopen_size:{:?}",
        c.download_tmp_file_reopen_size
    );
    return Ok(());
}
async fn upload_tmp_file_reopen_size(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.upload_tmp_file_reopen_size = *conf_arg.value.get::<u64>();

    log::trace!(target: "main",
        "c.upload_tmp_file_reopen_size:{:?}",
        c.upload_tmp_file_reopen_size
    );
    return Ok(());
}
async fn debug_is_open_print(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.debug_is_open_print = *conf_arg.value.get::<bool>();

    log::trace!(target: "main", "c.debug_is_open_print:{:?}", c.debug_is_open_print);
    return Ok(());
}
async fn is_open_sendfile(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.is_open_sendfile = *conf_arg.value.get::<bool>();
    #[cfg(not(unix))]
    {
        if c.is_open_sendfile {
            return Err(anyhow::anyhow!("windows not support open_sendfile"));
        }
    };

    log::trace!(target: "main", "c.is_open_sendfile:{:?}", c.is_open_sendfile);
    return Ok(());
}
async fn sendfile_max_write_size(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.sendfile_max_write_size = *conf_arg.value.get::<usize>();
    log::trace!(target: "main", "c.sendfile_max_write_size:{:?}", c.sendfile_max_write_size);
    return Ok(());
}
async fn tcp_config_name(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    let tcp_config_name = conf_arg.value.get::<String>().clone();
    if tcp_config_name.len() > 0 {
        c.tcp_config_name = tcp_config_name;
    }
    log::trace!(target: "main", "c.tcp_config_name:{:?}", c.tcp_config_name);
    return Ok(());
}
async fn quic_config_name(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    let quic_config_name = conf_arg.value.get::<String>().clone();
    if quic_config_name.len() > 0 {
        c.quic_config_name = quic_config_name;
    }

    log::trace!(target: "main", "c.quic_config_name:{:?}", c.quic_config_name);
    return Ok(());
}

async fn is_proxy_protocol_hello(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.is_proxy_protocol_hello = *conf_arg.value.get::<bool>();

    log::trace!(target: "main", "c.is_proxy_protocol_hello:{:?}", c.is_proxy_protocol_hello);
    return Ok(());
}

async fn domain(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.domain = conf_arg.value.get::<String>().clone().into();

    log::trace!(target: "main", "c.domain:{:?}", c.domain);
    return Ok(());
}

async fn is_open_ebpf(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.is_open_ebpf = conf_arg.value.get::<bool>().clone();
    #[cfg(not(unix))]
    {
        if c.is_open_ebpf {
            return Err(anyhow::anyhow!("windows not support open_ebpf"));
        }
    };

    #[cfg(feature = "anyproxy-ebpf")]
    if c.is_open_ebpf {
        use crate::config::any_ebpf_core;
        let any_ebpf_core_conf = any_ebpf_core::main_conf(&_ms).await;
        let ebpf_tx = any_ebpf_core_conf.ebpf();
        if ebpf_tx.is_none() {
            return Err(anyhow::anyhow!("err:is_open_ebpf is close"));
        }
    }

    log::trace!(target: "main", "c.is_open_ebpf:{:?}", c.is_open_ebpf);
    return Ok(());
}

async fn is_disable_share_http_context(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.is_disable_share_http_context = conf_arg.value.get::<bool>().clone();

    log::trace!(target: "main",
        "c.is_disable_share_http_context:{:?}",
        c.is_disable_share_http_context
    );
    return Ok(());
}

async fn read_buffer_page_size(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.read_buffer_page_size = conf_arg.value.get::<usize>().clone();
    if c.read_buffer_page_size < 2 {
        c.read_buffer_page_size = 2
    }

    log::trace!(target: "main", "c.read_buffer_page_size:{:?}", c.read_buffer_page_size);
    return Ok(());
}

async fn write_buffer_page_size(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.write_buffer_page_size = conf_arg.value.get::<usize>().clone();
    if c.write_buffer_page_size < 2 {
        c.write_buffer_page_size = 2
    }

    log::trace!(target: "main", "c.write_buffer_page_size:{:?}", c.write_buffer_page_size);
    return Ok(());
}

async fn is_port_direct_ebpf(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.is_port_direct_ebpf = conf_arg.value.get::<bool>().clone();

    log::trace!(target: "main", "c.is_port_direct_ebpf:{:?}", c.is_port_direct_ebpf);
    return Ok(());
}

async fn client_timeout_mil_time_ebpf(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.client_timeout_mil_time_ebpf = conf_arg.value.get::<u64>().clone();

    log::trace!(target: "main",
        "c.client_timeout_mil_time_ebpf:{:?}",
        c.client_timeout_mil_time_ebpf
    );
    return Ok(());
}

async fn upstream_timeout_mil_time_ebpf(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.upstream_timeout_mil_time_ebpf = conf_arg.value.get::<u64>().clone();

    log::trace!(target: "main",
        "c.upstream_timeout_mil_time_ebpf:{:?}",
        c.upstream_timeout_mil_time_ebpf
    );
    return Ok(());
}

async fn close_type(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    let close_type = conf_arg.value.get::<String>().clone();
    match close_type.as_str() {
        CLOSE_TYPE_FAST => {
            c.close_type = StreamCloseType::Fast;
        }
        CLOSE_TYPE_SHUTDOWM => {
            c.close_type = StreamCloseType::Shutdown;
        }
        CLOSE_TYPE_WAIT_EMPTY => {
            c.close_type = StreamCloseType::WaitEmpty;
        }
        _ => {
            return Err(anyhow::anyhow!(
                "err:close_type {} not find, please use:{}",
                close_type,
                CLOSE_TYPE_ALL
            ));
        }
    }

    log::trace!(target: "main", "c.close_type:{:?}", c.close_type);
    return Ok(());
}

async fn stream_delay_mil_time(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.stream_delay_mil_time = conf_arg.value.get::<u64>().clone();
    if c.stream_delay_mil_time > STREAM_MAX_DELAY_MIL_TIME_DEFAULT {
        c.stream_delay_mil_time = STREAM_MAX_DELAY_MIL_TIME_DEFAULT;
    }

    log::trace!(target: "main", "c.stream_delay_mil_time:{:?}", c.stream_delay_mil_time);
    return Ok(());
}

async fn stream_nopush(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.stream_nopush = conf_arg.value.get::<bool>().clone();

    log::trace!(target: "main", "c.stream_nopush:{:?}", c.stream_nopush);
    return Ok(());
}

async fn stream_nodelay_size(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.stream_nodelay_size = conf_arg.value.get::<usize>().clone() as i64;

    log::trace!(target: "main", "c.stream_nodelay_size:{:?}", c.stream_nodelay_size);
    return Ok(());
}

async fn directio(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.directio = conf_arg.value.get::<u64>().clone();

    log::trace!(target: "main", "c.directio:{:?}", c.directio);
    return Ok(());
}

async fn tmp_file_cache_size(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    let tmp_file_cache_size = conf_arg.value.get::<usize>().clone();
    c.tmp_file_cache_size = tmp_file_cache_size;
    log::trace!(target: "main", "c.tmp_file_cache_size:{:?}", c.tmp_file_cache_size);
    return Ok(());
}

async fn expires(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    let expires = conf_arg.value.get::<usize>().clone();
    c.expires = expires;
    log::trace!(target: "main", "c.expires:{:?}", c.expires);
    return Ok(());
}

async fn domain_from_http_v1(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    let str = conf_arg.value.get::<String>().clone();
    let mut domain_from_http_v1: DomainFromHttpV1 =
        toml::from_str(&str).map_err(|e| anyhow!("err:str {} => e:{}", str, e))?;

    let mut check_methods = HashMap::new();
    for (k, v) in domain_from_http_v1.check_methods.clone() {
        let k = k.trim().to_lowercase();
        check_methods.insert(k, v);
    }
    domain_from_http_v1.check_methods = check_methods;

    c.domain_from_http_v1 = Arc::new(domain_from_http_v1);
    log::trace!(target: "main", "c.domain_from_http_v1:{:?}", c.domain_from_http_v1);
    return Ok(());
}

pub async fn get_tmp_file_fd() -> Result<Arc<FileExt>> {
    let file = TMP_FILE_CACHES.get_mut().pop_front();
    if file.is_some() {
        async_create_tmp_file_fd(1);
        return Ok(file.unwrap());
    }
    let mut files = create_tmp_file_fd(1, false).await?;
    return Ok(files.pop_front().unwrap());
}

pub fn async_create_tmp_file_fd(size: usize) {
    tokio::task::spawn(async move {
        let files = create_tmp_file_fd(size, true).await;
        if let Err(e) = &files {
            log::error!("err:create_tmp_file_fd => e:{}", e);
        }
        for file in files.unwrap() {
            TMP_FILE_CACHES.get_mut().push_back(file);
        }
    });
}

pub async fn create_tmp_file_fd(size: usize, is_sleep: bool) -> Result<VecDeque<Arc<FileExt>>> {
    let mut files = VecDeque::with_capacity(size);
    for _ in 0..size {
        let ret: Result<VecDeque<Arc<FileExt>>> =
            tokio::task::spawn_blocking(move || do_create_tmp_file_fd(1)).await?;
        let ret = ret?;
        for v in ret {
            files.push_back(v);
        }
        if is_sleep {
            tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
        }
    }
    Ok(files)
}

pub fn do_create_tmp_file_fd(size: usize) -> Result<VecDeque<Arc<FileExt>>> {
    let pid = unsafe { libc::getpid() };
    use crate::config::common_core;
    use crate::util::default_config;
    let tmp_path = default_config::ANYPROXY_TMP_FULL_PATH.get().clone();
    let mut files = VecDeque::with_capacity(size);
    for _ in 0..size {
        let mut tmp_path = tmp_path.clone();
        let id = common_core::TMPFILE_ID.fetch_add(1, Ordering::SeqCst);
        let name = format!("{}_{}", pid, id);
        let levels = vec![2, 2];
        let mut levels_len = name.len();
        for v in &levels {
            let mut v = *v;
            if v >= name.len() {
                v = 2;
            }
            tmp_path.push_str(&name[levels_len - v..levels_len]);
            tmp_path.push_str("/");
            levels_len -= v;
        }
        if !Path::new(&tmp_path).exists() {
            std::fs::create_dir_all(&tmp_path)
                .map_err(|e| anyhow!("err:create_dir_all => e:{}", e))?;
        }
        tmp_path.push_str(&name);

        tmp_path.push_str(".tmp");
        #[cfg(feature = "anyio-file")]
        let start_time = Instant::now();

        let file_ext = open_tmp_file_fd(&tmp_path)?;
        let file_ext = Arc::new(file_ext);

        #[cfg(feature = "anyio-file")]
        if start_time.elapsed().as_millis() > 100 {
            log::info!(
                "open file:{} => {}",
                start_time.elapsed().as_millis(),
                tmp_path.as_str()
            );
        }

        files.push_back(file_ext);
    }
    return Ok(files);
}

pub fn open_tmp_file_fd(file_name: &str) -> Result<FileExt> {
    let _ = std::fs::remove_file(file_name);
    let file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(true)
        .open(file_name)
        .map_err(|e| anyhow!("err:file.open => file_name:{}, e:{}", file_name, e))?;

    #[cfg(not(unix))]
    let file_fd = 0;
    #[cfg(unix)]
    use std::os::unix::io::AsRawFd;
    #[cfg(unix)]
    let file_fd = file.as_raw_fd();

    let file_uniq = FileUniq::new(&file)?;
    let file_ext = FileExt {
        async_lock: ArcMutexTokio::new(()),
        file: ArcMutex::new(file),
        fix: Arc::new(FileExtFix::new(file_fd, file_uniq)),
        file_path: ArcRwLock::new(file_name.into()),
        file_len: 0,
    };
    file_ext.unlink(None);

    Ok(file_ext)
}
