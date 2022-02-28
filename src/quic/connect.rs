use super::client as quic_client;
use crate::quic::endpoints;
use crate::stream::client::Client;
use crate::stream::connect;
use crate::stream::stream_flow;
use crate::util::util;
use crate::Protocol7;
use async_trait::async_trait;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

pub struct Connect {
    address: String, //ip:port, domain:port
    ssl_domain: String,
    timeout: tokio::time::Duration,
    endpoints: Arc<endpoints::Endpoints>,
}

impl Connect {
    pub fn new(
        address: String, //ip:port, domain:port
        ssl_domain: String,
        timeout: tokio::time::Duration,
        endpoints: Arc<endpoints::Endpoints>,
    ) -> anyhow::Result<Connect> {
        Ok(Connect {
            address,
            ssl_domain,
            timeout,
            endpoints,
        })
    }
}

#[async_trait(?Send)]
impl connect::Connect for Connect {
    async fn connect(
        &self,
        stream_info: &mut Option<&mut stream_flow::StreamFlowInfo>,
    ) -> anyhow::Result<(
        Protocol7,
        stream_flow::StreamFlow,
        String,
        f32,
        SocketAddr,
        SocketAddr,
    )> {
        let start_time = Instant::now();
        let addr = util::lookup_host(self.timeout, &self.address).await?;

        let client = quic_client::Client::new(
            addr,
            self.timeout,
            self.endpoints.endpoint(),
            &self.ssl_domain,
        )?;
        let connection = { client.connect(stream_info).await? };
        let (protocol_name, stream, local_addr, remote_addr) =
            { connection.stream(stream_info).await? };
        let elapsed = start_time.elapsed().as_secs_f32();
        Ok((
            protocol_name,
            stream,
            self.address.clone(),
            elapsed,
            local_addr,
            remote_addr,
        ))
    }
}
