use crate::config as conf;
use crate::config::http_core_plugin::PluginHandleStream;
use crate::proxy::http_proxy::http_context::HttpContext;
use crate::proxy::{StreamCacheBuffer, StreamCloseType};
use any_base::module::module;
use any_base::typ;
use any_base::typ::{ArcRwLockTokio, ArcUnsafeAny};
use any_base::util::ArcString;
use anyhow::Result;
use lazy_static::lazy_static;
use std::sync::atomic::Ordering;
use std::sync::Arc;

lazy_static! {
    pub static ref HTTP_CONTEXT: Arc<HttpContext> = Arc::new(HttpContext::new());
}

pub struct ConfStream {
    pub buffer_cache: Arc<String>,
    pub stream_cache_size: i64,
    pub is_open_stream_cache_merge: bool,
    pub open_stream_cache_merge_mil_time: u64,
    pub is_open_stream_cache: bool,
    pub tmp_file_size: i64,
    pub limit_rate_after: i64,
    pub max_limit_rate: u64,
    pub max_stream_cache_size: u64,
    pub max_tmp_file_size: u64,
    pub is_tmp_file_io_page: bool,
    pub page_size: usize,
    pub min_cache_buffer_size: usize,
    pub min_read_buffer_size: usize,
    pub min_cache_file_size: usize,
    pub plugin_handle_stream: ArcRwLockTokio<PluginHandleStream>,
    pub read_buffer_size: usize,
}

impl ConfStream {
    pub fn new(
        tmp_file_size: u64,
        limit_rate_after: u64,
        limit_rate: u64,
        stream_cache_size: usize,
        is_open_stream_cache_merge: bool,
        open_stream_cache_merge_mil_time: u64,
        is_open_stream_cache: bool,
        is_tmp_file_io_page: bool,
        read_buffer_page_size: usize,
    ) -> Self {
        use crate::util::default_config;

        let buffer_cache = if is_open_stream_cache {
            if tmp_file_size > 0 && stream_cache_size > 0 {
                "file_and_cache".to_string()
            } else if tmp_file_size > 0 && stream_cache_size <= 0 {
                "file".to_string()
            } else {
                "cache".to_string()
            }
        } else {
            "memory".to_string()
        };

        let page_size = default_config::PAGE_SIZE.load(Ordering::Relaxed);
        let mut ssc = ConfStream {
            buffer_cache: Arc::new(buffer_cache),
            stream_cache_size: stream_cache_size as i64,
            is_open_stream_cache_merge,
            open_stream_cache_merge_mil_time,
            is_open_stream_cache,
            tmp_file_size: tmp_file_size as i64,
            limit_rate_after: limit_rate_after as i64,
            max_limit_rate: limit_rate,
            max_stream_cache_size: stream_cache_size as u64,
            max_tmp_file_size: tmp_file_size,
            is_tmp_file_io_page,
            page_size,
            min_cache_buffer_size: StreamCacheBuffer::buffer_size(),
            min_read_buffer_size: page_size,
            min_cache_file_size: page_size * 256,
            plugin_handle_stream: ArcRwLockTokio::default(),
            read_buffer_size: read_buffer_page_size * page_size,
        };

        if ssc.stream_cache_size > 0 && ssc.stream_cache_size < ssc.min_cache_buffer_size as i64 {
            ssc.stream_cache_size = ssc.min_cache_buffer_size as i64;
            ssc.max_stream_cache_size = ssc.min_cache_buffer_size as u64;
        }

        if ssc.tmp_file_size > 0 && ssc.tmp_file_size < ssc.min_cache_file_size as i64 {
            ssc.tmp_file_size = ssc.min_cache_file_size as i64;
            ssc.max_tmp_file_size = ssc.min_cache_file_size as u64;
        }

        if ssc.stream_cache_size > 0 {
            let stream_cache_size =
                (ssc.stream_cache_size / ssc.page_size as i64 + 1) * ssc.page_size as i64;
            ssc.stream_cache_size = stream_cache_size;
            ssc.max_stream_cache_size = stream_cache_size as u64;
        }

        if ssc.tmp_file_size > 0 {
            let tmp_file_size =
                (ssc.tmp_file_size / ssc.page_size as i64 + 1) * ssc.page_size as i64;
            ssc.tmp_file_size = tmp_file_size;
            ssc.max_tmp_file_size = tmp_file_size as u64;
        }

        ssc
    }
}

pub struct Conf {
    pub stream_cache_size: usize,
    pub is_open_stream_cache_merge: bool,
    pub open_stream_cache_merge_mil_time: u64,
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
    pub debug_is_open_print: bool,
    pub is_open_sendfile: bool,
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
    pub is_port_direct_ebpf: bool,
    pub client_timeout_mil_time_ebpf: u64,
    pub upstream_timeout_mil_time_ebpf: u64,
    pub close_type: StreamCloseType,
}
const STREAM_CACHE_SIZE_DEFAULT: usize = 131072;
const STREAM_SO_SINGER_TIME_DEFAULT: usize = 0;
const TCP_CONFIG_NAME_DEFAULT: &str = "tcp_config_default";
const QUIC_CONFIG_NAME_DEFAULT: &str = "quic_config_default";
const IS_TMP_FILE_IO_PAGE_DEFAULT: bool = true;
const READ_BUFFER_PAGE_SIZE_DEFAULT: usize = 2;
const OPEN_STREAM_CACHE_MERGE_MIL_TIME_DEFAULT: u64 = 100;
const CLIENT_TIMEOUT_MIL_TIME_EBPF_DEFAULT: u64 = 120000;
const UPSTREAM_TIMEOUT_MIL_TIME_EBPF_DEFAULT: u64 = 2000;
const CLOSE_TYPE_DEFAULT: StreamCloseType = StreamCloseType::Shutdown;
const CLOSE_TYPE_FAST: &str = "fast";
const CLOSE_TYPE_SHUTDOWM: &str = "shutdown";
const CLOSE_TYPE_WAIT_EMPTY: &str = "wait_empty";
const CLOSE_TYPE_ALL: &str = "fast shutdown wait_empty";

impl Conf {
    pub fn new() -> Self {
        Conf {
            stream_cache_size: STREAM_CACHE_SIZE_DEFAULT,
            is_open_stream_cache_merge: true,
            open_stream_cache_merge_mil_time: OPEN_STREAM_CACHE_MERGE_MIL_TIME_DEFAULT,
            is_upload_open_stream_cache: false,
            is_download_open_stream_cache: false,
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
            debug_is_open_print: false,
            is_open_sendfile: false,
            tcp_config_name: TCP_CONFIG_NAME_DEFAULT.to_string(),
            quic_config_name: QUIC_CONFIG_NAME_DEFAULT.to_string(),
            is_proxy_protocol_hello: false,
            domain: ArcString::default(),
            is_open_ebpf: false,
            http_context: HTTP_CONTEXT.clone(),
            is_disable_share_http_context: false,
            download: Arc::new(ConfStream::new(1, 1, 1, 1, true, 100, false, true, 2)),
            upload: Arc::new(ConfStream::new(1, 1, 1, 1, true, 100, false, true, 2)),
            read_buffer_page_size: READ_BUFFER_PAGE_SIZE_DEFAULT,
            is_port_direct_ebpf: true,
            client_timeout_mil_time_ebpf: CLIENT_TIMEOUT_MIL_TIME_EBPF_DEFAULT,
            upstream_timeout_mil_time_ebpf: UPSTREAM_TIMEOUT_MIL_TIME_EBPF_DEFAULT,
            close_type: CLOSE_TYPE_DEFAULT,
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
            name: "is_open_stream_cache_merge".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(is_open_stream_cache_merge(
                ms, conf_arg, cmd, conf
            )),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "open_stream_cache_merge_mil_time".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(open_stream_cache_merge_mil_time(
                ms, conf_arg, cmd, conf
            )),
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
            name: "tcp_config_name".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(tcp_config_name(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
        },
        module::Cmd {
            name: "quic_config_name".to_string(),
            set: |ms, conf_arg, cmd, conf| Box::pin(quic_config_name(ms, conf_arg, cmd, conf)),
            typ: module::CMD_TYPE_DATA,
            conf_typ: conf::CMD_CONF_TYPE_MAIN
                | conf::CMD_CONF_TYPE_SERVER
                | conf::CMD_CONF_TYPE_LOCAL,
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
    });
}

lazy_static! {
    pub static ref M: typ::ArcRwLock<module::Module> = typ::ArcRwLock::new(module::Module {
        name: "http_core".to_string(),
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
        typ: conf::MODULE_TYPE_HTTP,
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
        if child_conf.is_open_stream_cache_merge == true {
            child_conf.is_open_stream_cache_merge = parent_conf.is_open_stream_cache_merge;
        }
        if child_conf.open_stream_cache_merge_mil_time == OPEN_STREAM_CACHE_MERGE_MIL_TIME_DEFAULT {
            child_conf.open_stream_cache_merge_mil_time =
                parent_conf.open_stream_cache_merge_mil_time;
        }
        if child_conf.is_upload_open_stream_cache == false {
            child_conf.is_upload_open_stream_cache = parent_conf.is_upload_open_stream_cache;
        }
        if child_conf.is_download_open_stream_cache == false {
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
        if child_conf.debug_is_open_print == false {
            child_conf.debug_is_open_print = parent_conf.debug_is_open_print;
        }
        if child_conf.is_open_sendfile == false {
            child_conf.is_open_sendfile = parent_conf.is_open_sendfile;
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

        child_conf.download = Arc::new(ConfStream::new(
            child_conf.download_tmp_file_size,
            child_conf.download_limit_rate_after,
            child_conf.download_limit_rate,
            child_conf.stream_cache_size,
            child_conf.is_open_stream_cache_merge,
            child_conf.open_stream_cache_merge_mil_time,
            child_conf.is_download_open_stream_cache,
            child_conf.is_tmp_file_io_page,
            child_conf.read_buffer_page_size,
        ));

        child_conf.upload = Arc::new(ConfStream::new(
            child_conf.upload_tmp_file_size,
            child_conf.upload_limit_rate_after,
            child_conf.upload_limit_rate,
            child_conf.stream_cache_size,
            child_conf.is_open_stream_cache_merge,
            child_conf.open_stream_cache_merge_mil_time,
            child_conf.is_upload_open_stream_cache,
            child_conf.is_tmp_file_io_page,
            child_conf.read_buffer_page_size,
        ));

        use super::http_core_plugin;
        let http_core_plugin_conf = http_core_plugin::main_conf(&ms).await;

        let plugin_handle_stream = if child_conf.is_download_open_stream_cache {
            http_core_plugin_conf.plugin_handle_stream_cache.clone()
        } else {
            http_core_plugin_conf.plugin_handle_stream_memory.clone()
        };
        child_conf
            .download
            .plugin_handle_stream
            .set(plugin_handle_stream.get().await.clone())
            .await;

        let plugin_handle_stream = if child_conf.is_upload_open_stream_cache {
            http_core_plugin_conf.plugin_handle_stream_cache.clone()
        } else {
            http_core_plugin_conf.plugin_handle_stream_memory.clone()
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
    let conf = conf.get_mut::<Conf>();
    if conf.read_buffer_page_size > 0 {
        use crate::util::default_config::MIN_CACHE_BUFFER_NUM;
        MIN_CACHE_BUFFER_NUM.store(conf.read_buffer_page_size, Ordering::SeqCst);
    }
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
    if c.stream_cache_size <= 0 {
        c.stream_cache_size = StreamCacheBuffer::buffer_size();
    }
    log::trace!("c.stream_cache_size:{:?}", c.stream_cache_size);
    return Ok(());
}

async fn is_open_stream_cache_merge(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.is_open_stream_cache_merge = *conf_arg.value.get::<bool>();
    log::trace!(
        "c.is_open_stream_cache_merge:{:?}",
        c.is_open_stream_cache_merge
    );
    return Ok(());
}

async fn open_stream_cache_merge_mil_time(
    _ms: module::Modules,
    conf_arg: module::ConfArg,
    _cmd: module::Cmd,
    conf: typ::ArcUnsafeAny,
) -> Result<()> {
    let c = conf.get_mut::<Conf>();
    c.open_stream_cache_merge_mil_time = *conf_arg.value.get::<u64>();
    log::trace!(
        "c.open_stream_cache_merge_mil_time:{:?}",
        c.open_stream_cache_merge_mil_time
    );
    if c.open_stream_cache_merge_mil_time <= 0 {
        c.open_stream_cache_merge_mil_time = OPEN_STREAM_CACHE_MERGE_MIL_TIME_DEFAULT;
    }
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
    log::trace!(
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
    log::trace!(
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
    log::trace!(
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
    log::trace!(
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

    log::trace!(
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

    log::trace!("c.is_tmp_file_io_page:{:?}", c.is_tmp_file_io_page);
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

    log::trace!("c.stream_so_singer_time:{:?}", c.stream_so_singer_time);
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

    log::trace!(
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

    log::trace!("c.download_limit_rate:{:?}", c.download_limit_rate);
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

    log::trace!("c.upload_limit_rate_after:{:?}", c.upload_limit_rate_after);
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

    log::trace!("c.upload_limit_rate:{:?}", c.upload_limit_rate);
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

    log::trace!("c.download_tmp_file_size:{:?}", c.download_tmp_file_size);
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

    log::trace!("c.upload_tmp_file_size:{:?}", c.upload_tmp_file_size);
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

    log::trace!("c.debug_is_open_print:{:?}", c.debug_is_open_print);
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
    #[cfg(windows)]
    {
        if c.is_open_sendfile {
            return Err(anyhow::anyhow!("windows not support open_sendfile"));
        }
    };

    log::trace!("c.is_open_sendfile:{:?}", c.is_open_sendfile);
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
    log::trace!("c.tcp_config_name:{:?}", c.tcp_config_name);
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

    log::trace!("c.quic_config_name:{:?}", c.quic_config_name);
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

    log::trace!("c.is_proxy_protocol_hello:{:?}", c.is_proxy_protocol_hello);
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

    log::trace!("c.domain:{:?}", c.domain);
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
    #[cfg(windows)]
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

    log::trace!("c.is_open_ebpf:{:?}", c.is_open_ebpf);
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

    log::trace!(
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

    log::trace!("c.read_buffer_page_size:{:?}", c.read_buffer_page_size);
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

    log::trace!("c.is_port_direct_ebpf:{:?}", c.is_port_direct_ebpf);
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

    log::trace!(
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

    log::trace!(
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

    log::trace!("c.close_type:{:?}", c.close_type);
    return Ok(());
}
