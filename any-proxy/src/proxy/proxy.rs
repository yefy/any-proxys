use crate::proxy::stream_info::StreamInfo;
use crate::util::var;
use any_base::stream_flow::StreamFlow;
use any_base::typ::Share;
use anyhow::Result;
use async_trait::async_trait;
use std::fs::File;
use std::sync::Arc;

#[async_trait]
pub trait Stream: Send + Sync {
    async fn do_start(&mut self, stream_info: Share<StreamInfo>, stream: StreamFlow) -> Result<()>;
}

pub struct AccessContext {
    pub access_format_vars: var::Var,
    pub access_log_file: Arc<File>,
}
