use crate::proxy::proxy;
use crate::proxy::stream_info::StreamInfo;
use crate::proxy::stream_start;
use crate::proxy::stream_stream::StreamStream;
use crate::proxy::ServerArg;
use crate::proxy::StreamConfigContext;
use crate::stream::stream_flow;
use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
use std::cell::RefCell;
use std::rc::Rc;

pub struct HttpStream {
    http_arg: ServerArg,
    stream_config_context: Rc<StreamConfigContext>,
    upstream_stream: Option<stream_flow::StreamFlow>,
}

impl HttpStream {
    pub fn new(
        http_arg: ServerArg,
        stream_config_context: Rc<StreamConfigContext>,
        upstream_stream: stream_flow::StreamFlow,
    ) -> HttpStream {
        HttpStream {
            http_arg,
            stream_config_context,
            upstream_stream: Some(upstream_stream),
        }
    }

    pub async fn start(self, mut stream: stream_flow::StreamFlow) -> Result<()> {
        let (debug_print_access_log_time, debug_print_stream_flow_time, stream_so_singer_time) = {
            let stream_conf = &self.stream_config_context.stream;

            (
                stream_conf.debug_print_access_log_time,
                stream_conf.debug_print_stream_flow_time,
                stream_conf.stream_so_singer_time,
            )
        };

        let stream_info = self.http_arg.stream_info.clone();
        stream.set_stream_info(Some(stream_info.borrow().client_stream_flow_info.clone()));
        let shutdown_thread_rx = self.http_arg.executors.shutdown_thread_tx.subscribe();

        stream_start::do_start(
            self,
            stream_info,
            stream,
            shutdown_thread_rx,
            debug_print_access_log_time,
            debug_print_stream_flow_time,
            stream_so_singer_time,
        )
        .await
    }
}

#[async_trait(?Send)]
impl proxy::Stream for HttpStream {
    async fn do_start(
        &mut self,
        stream_info: Rc<RefCell<StreamInfo>>,
        client_stream: stream_flow::StreamFlow,
    ) -> Result<()> {
        let upstream_stream = self.upstream_stream.take().unwrap();
        #[cfg(unix)]
        let client_sendfile = None;
        #[cfg(unix)]
        let upstream_sendfile = None;

        let ret = StreamStream::stream_to_stream(
            self.stream_config_context.clone(),
            stream_info.clone(),
            client_stream,
            upstream_stream,
            #[cfg(unix)]
            client_sendfile,
            #[cfg(unix)]
            upstream_sendfile,
            self.http_arg.executors.thread_id.clone(),
            self.http_arg.tmp_file_id.clone(),
        )
        .await
        .map_err(|e| {
            anyhow!(
                "err:stream_to_stream => request_id:{}, e:{}",
                self.http_arg.stream_info.borrow().request_id,
                e
            )
        });

        self.http_arg.stream_info.borrow_mut().add_work_time("end");

        ret
    }
}
