use crate::proxy::stream_info::StreamInfo;
use crate::proxy::stream_start;
use crate::proxy::stream_stream::StreamStream;
use crate::proxy::ServerArg;
use crate::proxy::StreamConfigContext;
use crate::proxy::{proxy, StreamCloseType};
use any_base::stream_flow::StreamFlow;
use any_base::typ::{Share, ShareRw};
use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;

pub struct HttpStream {
    http_arg: ServerArg,
    scc: ShareRw<StreamConfigContext>,
    upstream_stream: Option<StreamFlow>,
    is_single: bool,
    is_client_sendfile: bool,
    is_upstream_sendfile: bool,
}

impl HttpStream {
    pub fn new(
        http_arg: ServerArg,
        scc: ShareRw<StreamConfigContext>,
        upstream_stream: StreamFlow,
        is_single: bool,
        is_client_sendfile: bool,
        is_upstream_sendfile: bool,
    ) -> HttpStream {
        HttpStream {
            http_arg,
            scc,
            upstream_stream: Some(upstream_stream),
            is_single,
            is_client_sendfile,
            is_upstream_sendfile,
        }
    }

    pub async fn start(mut self, mut stream: StreamFlow) -> Result<()> {
        self.upstream_stream.as_mut().unwrap().set_stream_info(Some(
            self.http_arg
                .stream_info
                .get()
                .upstream_stream_flow_info
                .clone(),
        ));

        let stream_info = self.http_arg.stream_info.clone();
        let client_stream_flow_info = {
            let mut stream_info = stream_info.get_mut();
            let scc = self.scc.get();
            let http_core_conf = scc.http_core_conf();
            stream_info.debug_is_open_print = http_core_conf.debug_is_open_print;
            stream_info.debug_is_open_stream_work_times =
                http_core_conf.debug_is_open_stream_work_times;
            stream_info.debug_print_access_log_time = http_core_conf.debug_print_access_log_time;
            stream_info.debug_print_stream_flow_time = http_core_conf.debug_print_stream_flow_time;
            stream_info.stream_so_singer_time = http_core_conf.stream_so_singer_time;
            stream_info.client_stream_flow_info.clone()
        };

        stream.set_stream_info(Some(client_stream_flow_info));
        let shutdown_thread_rx = self
            .http_arg
            .executors
            .context
            .shutdown_thread_tx
            .subscribe();
        let ret = stream_start::do_start(self, stream_info, stream, shutdown_thread_rx).await;
        ret
    }
}

#[async_trait]
impl proxy::Stream for HttpStream {
    async fn do_start(
        &mut self,
        stream_info: Share<StreamInfo>,
        client_stream: StreamFlow,
    ) -> Result<()> {
        let upstream_stream = self.upstream_stream.take().unwrap();

        let ret = if self.is_single {
            stream_info
                .get_mut()
                .add_work_time1("stream_to_stream_single");
            StreamStream::stream_to_stream_single(
                self.scc.clone(),
                stream_info.clone(),
                client_stream,
                upstream_stream,
                self.is_client_sendfile,
                self.is_upstream_sendfile,
            )
            .await
            .map_err(|e| {
                anyhow!(
                    "err:stream_to_stream => request_id:{}, e:{}",
                    self.http_arg.stream_info.get().request_id,
                    e
                )
            })
        } else {
            stream_info.get_mut().add_work_time1("stream_to_stream");
            StreamStream::stream_to_stream(
                self.scc.clone(),
                stream_info.clone(),
                client_stream,
                upstream_stream,
                self.is_client_sendfile,
                self.is_upstream_sendfile,
                StreamCloseType::Shutdown,
                StreamCloseType::Fast,
            )
            .await
            .map_err(|e| {
                anyhow!(
                    "err:stream_to_stream => request_id:{}, e:{}",
                    self.http_arg.stream_info.get().request_id,
                    e
                )
            })
        };

        self.http_arg
            .stream_info
            .get_mut()
            .add_work_time1("stream_to_stream end");

        ret
    }
}
