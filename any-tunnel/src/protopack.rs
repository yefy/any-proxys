use any_base::util::ArcString;
use anyhow::anyhow;
use anyhow::Result;
use bytes::Bytes;
use serde::ser;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub const MIN_CACHE_BUFFER: usize = 8192 * 2;
pub const TUNNEL_MAX_HEADER_SIZE: usize = 4096;
pub const TUNNEL_FLAG: &'static str = "${7tunnel_anyproxy}";
pub const TUNNEL_VERSION: &'static str = "tunnel.0.1.0";

//TunnelHeaderSize_u16 TunnelHeader TunnelHello
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TunnelHeader {
    pub header_type: u8,
    pub body_size: u16,
}

#[derive(Clone, Debug, PartialEq, num_derive::FromPrimitive)]
pub enum TunnelHeaderType {
    TunnelHello = 0,
    TunnelHelloAck = 1,
    TunnelData = 2,
    TunnelAddConnect = 3,
    TunnelMaxConnect = 4,
    TunnelClose = 5,
}

pub enum TunnelPack {
    TunnelHello(TunnelHello),
    TunnelHelloAck(TunnelHelloAck),
    TunnelData(DynamicTunnelData),
    TunnelAddConnect(TunnelAddConnect),
    TunnelMaxConnect(TunnelMaxConnect),
    TunnelClose(TunnelClose),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TunnelHello {
    pub version: String,
    pub session_id: ArcString,
    pub min_stream_cache_size: usize,
    pub channel_size: usize,
    pub client_peer_stream_index: usize,
    pub is_ack: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TunnelHelloAck {}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TunnelDataHeader {
    pub pack_id: u32,
    pub pack_size: u32,
}

pub type DynamicTunnelData = TunnelData;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TunnelDataVec {
    pub header: TunnelDataHeader,
    pub datas: Vec<u8>,
}

impl Default for TunnelDataVec {
    fn default() -> Self {
        TunnelDataVec {
            header: TunnelDataHeader {
                pack_id: 0,
                pack_size: 0,
            },
            datas: Vec::with_capacity(MIN_CACHE_BUFFER),
        }
    }
}

impl TunnelDataVec {
    fn resize(&mut self) {
        if self.datas.len() < self.header.pack_size as usize {
            self.datas.resize(self.header.pack_size as usize, 0);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TunnelData {
    pub header: TunnelDataHeader,
    pub datas: Bytes,
}

impl From<Bytes> for TunnelData {
    fn from(datas: Bytes) -> Self {
        TunnelData {
            header: TunnelDataHeader {
                pack_id: 0,
                pack_size: 0,
            },
            datas,
        }
    }
}

impl TunnelData {
    pub fn new(data: TunnelDataVec) -> Self {
        let TunnelDataVec { header, datas } = data;
        TunnelData {
            header,
            datas: Bytes::from(datas),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TunnelAddConnect {
    pub peer_stream_size: usize,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TunnelMaxConnect {
    pub peer_stream_size: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TunnelClose {
    pub is_client: bool,
}

pub async fn read_tunnel_hello<R: AsyncRead + std::marker::Unpin>(
    buf_reader: &mut R,
) -> Result<TunnelHello> {
    let mut slice = [0u8; TUNNEL_MAX_HEADER_SIZE];
    let pack = read_pack(buf_reader, &mut slice, Some(TunnelHeaderType::TunnelHello))
        .await
        .map_err(|e| anyhow!("err:read_pack => e:{}", e))?;
    if pack.is_none() {
        return Err(anyhow!("err:not tunnel_hello"));
    }
    let pack = pack.unwrap();
    match pack {
        TunnelPack::TunnelHello(tunnel_hello) => {
            return Ok(tunnel_hello);
        }
        _ => return Err(anyhow!("err:not tunnel_hello")),
    }
}

pub async fn read_tunnel_hello_ack<R: AsyncRead + std::marker::Unpin>(
    buf_reader: &mut R,
) -> Result<TunnelHelloAck> {
    let mut slice = [0u8; TUNNEL_MAX_HEADER_SIZE];
    let pack = read_pack(
        buf_reader,
        &mut slice,
        Some(TunnelHeaderType::TunnelHelloAck),
    )
    .await
    .map_err(|e| anyhow!("err:read_pack => e:{}", e))?;
    if pack.is_none() {
        return Err(anyhow!("err:not tunnel_hello_ack"));
    }
    let pack = pack.unwrap();
    match pack {
        TunnelPack::TunnelHelloAck(tunnel_hello_ack) => {
            return Ok(tunnel_hello_ack);
        }
        _ => return Err(anyhow!("err:not tunnel_hello_ack")),
    }
}

pub async fn write_tunnel_data<W: AsyncWrite + std::marker::Unpin>(
    buf_writer: &mut W,
    value: &TunnelData,
) -> Result<()> {
    let typ = TunnelHeaderType::TunnelData;
    write_pack(buf_writer, typ, &value.header, false)
        .await
        .map_err(|e| anyhow!("err:write_pack => e:{}", e))?;
    buf_writer
        .write_all(&value.datas)
        .await
        .map_err(|e| anyhow!("err:buf_writer.write_all => e:{}", e))?;
    log::trace!(target: "main", "write_pack datas len:{:?}", value.datas.len());
    buf_writer
        .flush()
        .await
        .map_err(|e| anyhow!("err:buf_writer.flush => e:{}", e))?;
    Ok(())
}

pub async fn write_pack<T: ?Sized, W: AsyncWrite + std::marker::Unpin>(
    buf_writer: &mut W,
    typ: TunnelHeaderType,
    value: &T,
    is_flush: bool,
) -> Result<()>
where
    T: ser::Serialize,
{
    let slice = toml::to_vec(value).map_err(|e| anyhow!("err:toml::to_vec => e:{}", e))?;
    let header = TunnelHeader {
        header_type: typ as u8,
        body_size: slice.len() as u16,
    };
    let header_slice = toml::to_vec(&header).map_err(|e| anyhow!("err:stream => e:{}", e))?;
    buf_writer
        .write_all(TUNNEL_FLAG.as_bytes())
        .await
        .map_err(|e| anyhow!("err:buf_writer.write_all => e:{}", e))?;
    buf_writer
        .write_u16(header_slice.len() as u16)
        .await
        .map_err(|e| anyhow!("err:buf_writer.write_u16 => e:{}", e))?;
    log::trace!(target: "main", "write_pack header len:{:?}", header_slice.len());
    buf_writer
        .write_all(&header_slice)
        .await
        .map_err(|e| anyhow!("err: buf_writer.write_all => e:{}", e))?;
    log::trace!(target: "main", "write_pack header:{:?}", header);
    buf_writer
        .write_all(&slice)
        .await
        .map_err(|e| anyhow!("err: buf_writer.write_all => e:{}", e))?;
    log::trace!(target: "main", "write_pack body len:{:?}", slice.len());
    if is_flush {
        buf_writer
            .flush()
            .await
            .map_err(|e| anyhow!("err:buf_writer.flush => e:{}", e))?;
    }
    Ok(())
}

pub async fn read_pack_all<R: AsyncRead + std::marker::Unpin>(
    buf_reader: &mut R,
    slice: &mut [u8],
) -> Result<TunnelPack> {
    let pack = read_pack(buf_reader, slice, None).await?;
    if pack.is_none() {
        return Err(anyhow!("err:pack.is_none"));
    }
    Ok(pack.unwrap())
}

pub async fn read_pack<R: AsyncRead + std::marker::Unpin>(
    buf_reader: &mut R,
    slice: &mut [u8],
    typ: Option<TunnelHeaderType>,
) -> Result<Option<TunnelPack>> {
    let mut flag_slice = [0u8; TUNNEL_FLAG.len() as usize];
    let mut read_size = 0;
    loop {
        if read_size == flag_slice.len() {
            break;
        }
        read_size += buf_reader
            .read(&mut flag_slice[read_size..])
            .await
            .map_err(|e| anyhow!("err:buf_reader.read_exact => e:{}", e))?;
        let flag_str = String::from_utf8_lossy(&flag_slice[0..read_size]);
        if &TUNNEL_FLAG[0..read_size] != &flag_str {
            return Ok(None);
        }
    }

    let header_size = buf_reader
        .read_u16()
        .await
        .map_err(|e| anyhow!("err:header_size => e:{}", e))?;

    log::trace!(target: "main", "header_size:{}", header_size);
    if header_size as usize > slice.len() || header_size <= 0 {
        return Err(anyhow!(
            "err:header_size as usize > slice.len() || header_size <= 0 => header_size:{}",
            header_size
        ));
    }

    let header_slice = &mut slice[..header_size as usize];
    buf_reader
        .read_exact(header_slice)
        .await
        .map_err(|e| anyhow!("err:header_slice => e:{}", e))?;
    let header: TunnelHeader =
        toml::from_slice(header_slice).map_err(|e| anyhow!("err:TunnelData header=> e:{}", e))?;
    log::trace!(target: "main", "read_pack header:{:?}", header);

    let header_type: Option<TunnelHeaderType> = num::FromPrimitive::from_u8(header.header_type);
    if header_type.is_none() {
        return Err(anyhow!("err:TunnelHeaderType:{}", header.header_type));
    }
    let header_type = header_type.unwrap();

    if typ.is_some() {
        if header.header_type != typ.unwrap() as u8 {
            log::error!("header.header_type:{}", header.header_type);
            return Ok(None);
        }
    }

    //header.body_size 可以为0
    if header.body_size as usize > slice.len() {
        return Err(anyhow!("err:TunnelData body_size"));
    }

    let body_slice = &mut slice[..header.body_size as usize];
    if header.body_size > 0 {
        buf_reader
            .read_exact(body_slice)
            .await
            .map_err(|e| anyhow!("err:body_slice => e:{}", e))?;
    }

    match header_type {
        TunnelHeaderType::TunnelHello => {
            let value: TunnelHello =
                toml::from_slice(body_slice).map_err(|e| anyhow!("err:TunnelHello=> e:{}", e))?;
            Ok(Some(TunnelPack::TunnelHello(value)))
        }
        TunnelHeaderType::TunnelHelloAck => {
            let value: TunnelHelloAck = toml::from_slice(body_slice)
                .map_err(|e| anyhow!("err:TunnelHelloAck=> e:{}", e))?;
            Ok(Some(TunnelPack::TunnelHelloAck(value)))
        }
        TunnelHeaderType::TunnelData => {
            let mut tunnel_data = TunnelDataVec::default();
            tunnel_data.header =
                toml::from_slice(body_slice).map_err(|e| anyhow!("err:TunnelData=> e:{}", e))?;
            log::trace!(target: "main", "read_pack body:{:?}", tunnel_data.header);

            tunnel_data.resize();
            let mut datas_slice = tunnel_data.datas.as_mut_slice();
            log::trace!(target: "main", "read_pack datas_slice.len:{:?}", datas_slice.len());

            buf_reader
                .read_exact(&mut datas_slice)
                .await
                .map_err(|e| anyhow!("err:TunnelData => e:{}", e))?;
            log::trace!(target: "main", "read_pack datas.len:{:?}", tunnel_data.datas.len());
            Ok(Some(TunnelPack::TunnelData(TunnelData::new(tunnel_data))))
        }
        TunnelHeaderType::TunnelAddConnect => {
            let value: TunnelAddConnect = toml::from_slice(body_slice)
                .map_err(|e| anyhow!("err:TunnelAddConnect=> e:{}", e))?;
            Ok(Some(TunnelPack::TunnelAddConnect(value)))
        }
        TunnelHeaderType::TunnelMaxConnect => {
            let value: TunnelMaxConnect = toml::from_slice(body_slice)
                .map_err(|e| anyhow!("err:TunnelMaxConnect=> e:{}", e))?;
            Ok(Some(TunnelPack::TunnelMaxConnect(value)))
        }
        TunnelHeaderType::TunnelClose => {
            let value: TunnelClose =
                toml::from_slice(body_slice).map_err(|e| anyhow!("err:TunnelHello=> e:{}", e))?;
            Ok(Some(TunnelPack::TunnelClose(value)))
        }
    }
}
