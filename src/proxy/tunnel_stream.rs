use super::stream_info::StreamInfo;
use crate::io::buf_reader::BufReader;
use crate::io::buf_stream::BufStream;
use crate::io::buf_writer::BufWriter;
use crate::stream::stream_flow;
use crate::{protopack, Protocol7};
use any_tunnel::protopack::TUNNEL_VERSION;
use any_tunnel::server as tunnel_server;
use any_tunnel2::protopack::TUNNEL_VERSION as TUNNEL2_VERSION;
use any_tunnel2::server as tunnel2_server;
use anyhow::anyhow;
use anyhow::Result;
use std::net::SocketAddr;

pub struct TunnelStream {}

impl TunnelStream {
    pub async fn tunnel_stream(
        tunnel_publish: tunnel_server::Publish,
        tunnel2_publish: tunnel2_server::Publish,
        local_addr: SocketAddr,
        remote_addr: SocketAddr,
        protocol7: Protocol7,
        stream_info: &mut StreamInfo,
        client_stream: stream_flow::StreamFlow,
    ) -> Result<(
        bool,
        Option<(Box<[u8]>, usize, usize)>,
        Option<stream_flow::StreamFlow>,
    )> {
        log::debug!("server protocol7:{}", protocol7.to_string());
        let mut client_buf_reader = BufReader::new(&client_stream);

        if !protocol7.is_tunnel() {
            let tunnel_hello = protopack::anyproxy::read_tunnel_hello(&mut client_buf_reader)
                .await
                .map_err(|e| anyhow!("err:read_tunnel_hello => e:{}", e))?;
            if tunnel_hello.is_some() {
                let tunnel_hello = tunnel_hello.unwrap();
                if &tunnel_hello.version == TUNNEL_VERSION {
                    stream_info.is_discard_flow = true;
                    log::debug!("tunnel_hello:{:?}", tunnel_hello);
                    let buffer = client_buf_reader.table_buffer();
                    let buf_reader = BufReader::new_from_buffer(client_stream, buffer);
                    let buf_writer = BufWriter::new(buf_reader);
                    let buf_stream = BufStream::from(buf_writer);
                    tunnel_publish
                        .push_peer_stream(buf_stream, local_addr, remote_addr)
                        .await
                        .map_err(|e| {
                            anyhow!("err:self.tunnel_publish.push_peer_stream => e:{}", e)
                        })?;
                    return Ok((true, None, None));
                }
                return Err(anyhow!(
                    "err:&tunnel_hello.version != TUNNEL_VERSION => {} != {}",
                    &tunnel_hello.version,
                    TUNNEL_VERSION
                ));
            }

            let tunnel_hello = protopack::anyproxy::read_tunnel2_hello(&mut client_buf_reader)
                .await
                .map_err(|e| anyhow!("err:anyproxy::read_tunnel2_hello => e:{}", e))?;
            if tunnel_hello.is_some() {
                let tunnel_hello = tunnel_hello.unwrap();
                if &tunnel_hello.version == TUNNEL2_VERSION {
                    stream_info.is_discard_flow = true;
                    log::debug!("tunnel_hello:{:?}", tunnel_hello);
                    let buffer = client_buf_reader.table_buffer();
                    let buf_reader = BufReader::new_from_buffer(client_stream, buffer);
                    let buf_writer = BufWriter::new(buf_reader);
                    let buf_stream = BufStream::from(buf_writer);
                    tunnel2_publish
                        .push_peer_stream(buf_stream, local_addr, remote_addr)
                        .await
                        .map_err(|e| {
                            anyhow!("err:self.tunnel2_publish.push_peer_stream => e:{}", e)
                        })?;
                    return Ok((true, None, None));
                }
                return Err(anyhow!(
                    "err:&tunnel_hello.version != TUNNEL2_VERSION => {} != {}",
                    &tunnel_hello.version,
                    TUNNEL2_VERSION
                ));
            }
        }
        let buffer = client_buf_reader.table_buffer();
        Ok((false, Some(buffer), Some(client_stream)))
    }
}
