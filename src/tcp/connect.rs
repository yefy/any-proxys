use super::client as tcp_lient;
use crate::config::config_toml::TcpConfig;
use crate::stream::client::Client;
use crate::stream::connect;
use crate::stream::stream_flow;
use crate::util::util;
use crate::Protocol7;
use async_trait::async_trait;
use std::net::SocketAddr;
use std::time::Instant;

pub struct Connect {
    address: String, //ip:port, domain:port
    timeout: tokio::time::Duration,
    tcp_config: TcpConfig,
}

impl Connect {
    pub fn new(
        address: String, //ip:port, domain:port
        timeout: tokio::time::Duration,
        tcp_config: TcpConfig,
    ) -> anyhow::Result<Connect> {
        Ok(Connect {
            address,
            timeout,
            tcp_config,
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

        let client = tcp_lient::Client::new(addr, self.timeout, self.tcp_config.clone())?;
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
