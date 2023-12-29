use crate::io::buf_reader::BufReader;
use any_base::util::ArcString;
use any_tunnel::protopack;
use any_tunnel::protopack::TUNNEL_VERSION;
use any_tunnel2::protopack as protopack2;
use any_tunnel2::protopack::TUNNEL_VERSION as TUNNEL2_VERSION;
use anyhow::anyhow;
use anyhow::Result;
use bytes::BufMut;
use serde::ser;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::io::AsyncRead;
use tokio::io::AsyncWrite;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub const ANYPROXY_MAX_HEADER_SIZE: usize = 4096;
pub const ANYPROXY_FLAG: &'static str = "$${anyproxy}";
pub const ANYPROXY_VERSION: &'static str = "anyproxy.0.1.0";

//ANYPROXY_FLAG AnyproxyHeaderSize_u16 AnyproxyHeader AnyproxyHello
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AnyproxyHeader {
    pub header_type: u8,
    pub body_size: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnyproxyHello {
    pub version: ArcString,
    pub request_id: ArcString,
    pub client_addr: SocketAddr,
    pub domain: ArcString,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AnyproxyHeartbeat {
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AnyproxyHeartbeatR {
    pub version: String,
}

#[derive(num_derive::FromPrimitive)]
pub enum AnyproxyHeaderType {
    Hello = 0,
    Heartbeat = 1,
    HeartbeatR = 2,
}

pub enum AnyproxyPack {
    Hello(AnyproxyHello),
    Heartbeat(AnyproxyHeartbeat),
    HeartbeatR(AnyproxyHeartbeatR),
}

pub async fn read_tunnel_hello<R: AsyncRead + Unpin>(
    buf_reader: &mut BufReader<R>,
) -> Result<Option<protopack::TunnelHello>> {
    buf_reader.start();

    let tunnel_hello = protopack::read_tunnel_hello(buf_reader).await;
    if !buf_reader.rollback() {
        return Err(anyhow!("err:rollback"));
    }
    if tunnel_hello.is_err() {
        return Ok(None);
    }

    let tunnel_hello = tunnel_hello.unwrap();
    if &tunnel_hello.version != TUNNEL_VERSION {
        return Ok(None);
    }
    Ok(Some(tunnel_hello))
}

pub async fn read_tunnel2_hello<R: AsyncRead + Unpin>(
    buf_reader: &mut BufReader<R>,
) -> Result<Option<protopack2::TunnelHello>> {
    buf_reader.start();

    let tunnel_hello = protopack2::read_tunnel_hello(buf_reader).await;
    if !buf_reader.rollback() {
        return Err(anyhow!("err:rollback"));
    }
    if tunnel_hello.is_err() {
        return Ok(None);
    }

    let tunnel_hello = tunnel_hello.unwrap();

    if &tunnel_hello.version != TUNNEL2_VERSION {
        return Ok(None);
    }
    Ok(Some(tunnel_hello))
}

pub async fn read_heartbeat_rollback<R: AsyncRead + Unpin>(
    buf_reader: &mut BufReader<R>,
) -> Result<Option<AnyproxyHeartbeat>> {
    let anyproxy = read_pack_rollback(buf_reader, Some(AnyproxyHeaderType::Heartbeat))
        .await
        .map_err(|e| anyhow!("err:read_pack_rollback => e:{}", e))?;
    if anyproxy.is_none() {
        return Ok(None);
    }
    let (anyproxy, _) = anyproxy.unwrap();
    match anyproxy {
        AnyproxyPack::Heartbeat(heartbeat) => return Ok(Some(heartbeat)),
        _ => Err(anyhow!("read_heartbeat"))?,
    }
}

pub async fn read_heartbeat<R: AsyncRead + Unpin>(
    buf_reader: &mut R,
) -> Result<Option<AnyproxyHeartbeat>> {
    let anyproxy = read_pack(buf_reader, Some(AnyproxyHeaderType::Heartbeat))
        .await
        .map_err(|e| anyhow!("err:read_pack_rollback => e:{}", e))?;
    if anyproxy.is_none() {
        return Ok(None);
    }
    let (anyproxy, _) = anyproxy.unwrap();
    match anyproxy {
        AnyproxyPack::Heartbeat(heartbeat) => return Ok(Some(heartbeat)),
        _ => Err(anyhow!("read_heartbeat"))?,
    }
}

pub async fn read_heartbeat_r<R: AsyncRead + Unpin>(
    buf_reader: &mut R,
) -> Result<Option<AnyproxyHeartbeatR>> {
    let anyproxy = read_pack(buf_reader, Some(AnyproxyHeaderType::HeartbeatR))
        .await
        .map_err(|e| anyhow!("err:read_pack_rollback => e:{}", e))?;
    if anyproxy.is_none() {
        return Ok(None);
    }
    let (anyproxy, _) = anyproxy.unwrap();
    match anyproxy {
        AnyproxyPack::HeartbeatR(heartbeat_r) => return Ok(Some(heartbeat_r)),
        _ => Err(anyhow!("read_heartbeatR"))?,
    }
}

pub async fn read_hello<R: AsyncRead + Unpin>(
    buf_reader: &mut BufReader<R>,
) -> Result<Option<(AnyproxyHello, usize)>> {
    let anyproxy = read_pack_rollback(buf_reader, Some(AnyproxyHeaderType::Hello))
        .await
        .map_err(|e| anyhow!("err:read_pack_rollback => e:{}", e))?;
    if anyproxy.is_none() {
        return Ok(None);
    }
    let (anyproxy, pack_size) = anyproxy.unwrap();
    match anyproxy {
        AnyproxyPack::Hello(hello) => return Ok(Some((hello, pack_size))),
        _ => Err(anyhow!("read_hello"))?,
    }
}

pub async fn write_pack<T: ?Sized, W: AsyncWrite + Unpin>(
    buf_writer: &mut W,
    typ: AnyproxyHeaderType,
    value: &T,
) -> Result<usize>
where
    T: ser::Serialize,
{
    let mut pack_size = 0;
    let slice = toml::to_vec(value).map_err(|e| anyhow!("err:toml::to_vec => e:{}", e))?;
    let header_slice = toml::to_vec(&AnyproxyHeader {
        header_type: typ as u8,
        body_size: slice.len() as u16,
    })?;
    buf_writer
        .write(ANYPROXY_FLAG.as_bytes())
        .await
        .map_err(|e| anyhow!("err:buf_writer.write => e:{}", e))?;
    pack_size += ANYPROXY_FLAG.as_bytes().len();
    buf_writer
        .write_u16(header_slice.len() as u16)
        .await
        .map_err(|e| anyhow!("err:buf_writer.write_u16 => e:{}", e))?;
    pack_size += 2;
    buf_writer
        .write(&header_slice)
        .await
        .map_err(|e| anyhow!("err:buf_writer.write => e:{}", e))?;
    pack_size += header_slice.len();
    buf_writer
        .write(&slice)
        .await
        .map_err(|e| anyhow!("err:buf_writer.write => e:{}", e))?;
    pack_size += slice.len();
    buf_writer
        .flush()
        .await
        .map_err(|e| anyhow!("err:buf_writer.flush => e:{}", e))?;
    Ok(pack_size)
}

pub async fn pack_to_vec<T: ?Sized>(typ: AnyproxyHeaderType, value: &T) -> Result<Vec<u8>>
where
    T: ser::Serialize,
{
    let mut datas = Vec::with_capacity(100);
    let slice = toml::to_vec(value).map_err(|e| anyhow!("err:toml::to_vec => e:{}", e))?;
    let header_slice = toml::to_vec(&AnyproxyHeader {
        header_type: typ as u8,
        body_size: slice.len() as u16,
    })?;
    datas.extend_from_slice(ANYPROXY_FLAG.as_bytes());
    datas.put_u16(header_slice.len() as u16);
    datas.extend_from_slice(&header_slice);
    datas.extend_from_slice(&slice);
    Ok(datas)
}

pub async fn read_pack_rollback<R: AsyncRead + Unpin>(
    buf_reader: &mut BufReader<R>,
    typ: Option<AnyproxyHeaderType>,
) -> Result<Option<(AnyproxyPack, usize)>> {
    buf_reader.start();
    let anyproxy = read_pack(buf_reader, typ)
        .await
        .map_err(|e| anyhow!("err:read_pack => e:{}", e))?;
    if anyproxy.is_none() {
        if !buf_reader.rollback() {
            return Err(anyhow!("err:rollback"));
        }
        return Ok(None);
    }
    if !buf_reader.commit() {
        return Err(anyhow!("err:commit"));
    }
    Ok(anyproxy)
}

pub async fn read_pack<R: AsyncRead + Unpin>(
    buf_reader: &mut R,
    typ: Option<AnyproxyHeaderType>,
) -> Result<Option<(AnyproxyPack, usize)>> {
    let mut pack_size = 0;
    let mut slice = [0u8; ANYPROXY_MAX_HEADER_SIZE];
    let flag_slice = &mut slice[..ANYPROXY_FLAG.len() as usize];
    buf_reader
        .read_exact(flag_slice)
        .await
        .map_err(|e| anyhow!("err:buf_reader.read_exact => e:{}", e))?;
    pack_size += flag_slice.len();
    let flag_str = String::from_utf8_lossy(flag_slice);
    if ANYPROXY_FLAG != &flag_str {
        return Ok(None);
    }

    let header_size = buf_reader
        .read_u16()
        .await
        .map_err(|e| anyhow!("err:buf_reader.read_u16 => e:{}", e))?;
    pack_size += 2;
    if header_size as usize > ANYPROXY_MAX_HEADER_SIZE || header_size <= 0 {
        return Err(anyhow!(
            "err:header_size > ANYPROXY_MAX_HEADER_SIZE => header_size:{}",
            header_size
        ));
    }

    let header_slice = &mut slice[..header_size as usize];
    buf_reader
        .read_exact(header_slice)
        .await
        .map_err(|e| anyhow!("err:buf_reader.read_exact => e:{}", e))?;
    pack_size += header_slice.len();
    let header: AnyproxyHeader =
        toml::from_slice(header_slice).map_err(|e| anyhow!("err:AnyproxyPack header=> e:{}", e))?;

    if header.body_size as usize > ANYPROXY_MAX_HEADER_SIZE || header.body_size <= 0 {
        return Err(anyhow!("err:AnyproxyPack body_size"));
    }

    let header_type: Option<AnyproxyHeaderType> = num::FromPrimitive::from_u8(header.header_type);
    if header_type.is_none() {
        return Err(anyhow!("err:AnyproxyHeaderType:{}", header.header_type));
    }
    let header_type = header_type.unwrap();

    if typ.is_some() {
        if header.header_type != typ.unwrap() as u8 {
            return Ok(None);
        }
    }

    let body_slice = &mut slice[..header.body_size as usize];
    buf_reader
        .read_exact(body_slice)
        .await
        .map_err(|e| anyhow!("err:buf_reader.read_exact => e:{}", e))?;
    pack_size += body_slice.len();
    match header_type {
        AnyproxyHeaderType::Hello => {
            let value: AnyproxyHello =
                toml::from_slice(body_slice).map_err(|e| anyhow!("err:AnyproxyPack=> e:{}", e))?;
            Ok(Some((AnyproxyPack::Hello(value), pack_size)))
        }
        AnyproxyHeaderType::Heartbeat => {
            let value: AnyproxyHeartbeat =
                toml::from_slice(body_slice).map_err(|e| anyhow!("err:AnyproxyPack=> e:{}", e))?;
            Ok(Some((AnyproxyPack::Heartbeat(value), pack_size)))
        }
        AnyproxyHeaderType::HeartbeatR => {
            let value: AnyproxyHeartbeatR =
                toml::from_slice(body_slice).map_err(|e| anyhow!("err:AnyproxyPack=> e:{}", e))?;
            Ok(Some((AnyproxyPack::HeartbeatR(value), pack_size)))
        }
    }
}
