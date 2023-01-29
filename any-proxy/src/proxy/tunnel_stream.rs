use super::stream_info::StreamInfo;
use crate::io::buf_reader::BufReader;
use crate::io::buf_stream::BufStream;
use crate::io::buf_writer::BufWriter;
use crate::stream::stream_flow;
use crate::{protopack, Protocol7};
use any_tunnel::server as tunnel_server;
use any_tunnel2::server as tunnel2_server;
use anyhow::anyhow;
use anyhow::Result;
use std::cell::RefCell;
use std::net::SocketAddr;
use std::rc::Rc;

pub struct TunnelStream {}

impl TunnelStream {
    pub async fn tunnel_stream(
        tunnel_publish: tunnel_server::Publish,
        tunnel2_publish: tunnel2_server::Publish,
        local_addr: SocketAddr,
        remote_addr: SocketAddr,
        protocol7: Protocol7,
        mut client_buf_reader: BufReader<stream_flow::StreamFlow>,
        stream_info: Rc<RefCell<StreamInfo>>,
    ) -> Result<(bool, Option<BufReader<stream_flow::StreamFlow>>)> {
        log::debug!("server protocol7:{}", protocol7.to_string());
        if protocol7.is_tunnel() {
            return Ok((false, Some(client_buf_reader)));
        }

        let tunnel_hello = protopack::anyproxy::read_tunnel_hello(&mut client_buf_reader)
            .await
            .map_err(|e| anyhow!("err:read_tunnel_hello => e:{}", e))?;

        if tunnel_hello.is_some() {
            let tunnel_hello = tunnel_hello.unwrap();
            log::debug!("tunnel_hello:{:?}", tunnel_hello);
            stream_info.borrow_mut().is_discard_flow = true;
            stream_info.borrow_mut().is_discard_timeout = true;
            let client_buf_stream = BufStream::from(BufWriter::new(client_buf_reader));
            tunnel_publish
                .push_peer_stream(client_buf_stream, local_addr, remote_addr)
                .await
                .map_err(|e| anyhow!("err:self.tunnel_publish.push_peer_stream => e:{}", e))?;
            return Ok((true, None));
        }

        let tunnel_hello = protopack::anyproxy::read_tunnel2_hello(&mut client_buf_reader)
            .await
            .map_err(|e| anyhow!("err:anyproxy::read_tunnel2_hello => e:{}", e))?;

        if tunnel_hello.is_some() {
            let tunnel_hello = tunnel_hello.unwrap();
            log::debug!("tunnel_hello:{:?}", tunnel_hello);
            stream_info.borrow_mut().is_discard_flow = true;
            stream_info.borrow_mut().is_discard_timeout = true;
            let client_buf_stream = BufStream::from(BufWriter::new(client_buf_reader));
            tunnel2_publish
                .push_peer_stream(client_buf_stream, local_addr, remote_addr)
                .await
                .map_err(|e| anyhow!("err:self.tunnel2_publish.push_peer_stream => e:{}", e))?;
            return Ok((true, None));
        }

        Ok((false, Some(client_buf_reader)))
    }
}
