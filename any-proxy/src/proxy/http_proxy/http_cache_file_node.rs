use crate::proxy::http_proxy::bitmap::BitMap;
use crate::proxy::http_proxy::http_cache_file::ProxyCacheFileInfo;
use crate::proxy::http_proxy::http_header_parse::http_parse;
use crate::proxy::http_proxy::http_header_parse::{
    content_length, content_range, e_tag, last_modified,
};
use crate::proxy::http_proxy::http_stream_request::{CacheFileStatus, HttpResponseInfo};
use any_base::file_ext::{FileExt, FileExtFix, FileUniq};
use any_base::io::async_write_msg::MsgReadBufFile;
use any_base::typ::{ArcMutex, ArcMutexTokio, ArcRwLock};
use any_base::util::{bytes_index, bytes_split, bytes_split_once, ArcString};
use anyhow::anyhow;
use anyhow::Result;
use bytes::{Bytes, BytesMut};
use http::{Response, Uri};
use hyper::Body;
use std::collections::{HashMap, VecDeque};
use std::io::{Seek, Write};
use std::sync::atomic::AtomicI64;
use std::sync::Arc;
#[cfg(feature = "anyio-file")]
use std::time::Instant;
use std::time::UNIX_EPOCH;

pub const CACHE_FILE_KEY: &'static str = "@#anypryx#@:%&#@\r\n";

pub struct ProxyCacheFileNodeUpstream {
    pub is_upstream: bool,
    pub version: i64,
    pub upstream_count: Arc<AtomicI64>,
    pub upstream_waits: VecDeque<tokio::sync::oneshot::Sender<()>>,
}

pub struct ProxyCacheFileNodeFix {
    pub response: Response<Body>,
    pub file_head_size: usize,
    pub http_head_size: usize,
    pub bitmap_start: usize,
    pub body_start: usize,
}

pub struct ProxyCacheFileNodeContext {
    pub cache_file_status: CacheFileStatus,
    pub cache_file_node_version: u64,
    pub bitmap: ArcRwLock<BitMap>,
    pub bitmap_to_file: VecDeque<ArcRwLock<BitMap>>,
    pub cache_control_time: i64,
    //文件实际过期时间，会被更新
    pub expires_time: u64,
    pub slice_upstream_map: ArcRwLock<HashMap<usize, ArcMutex<ProxyCacheFileNodeUpstream>>>,
    //每次访问更新时间， 不能删除
    pub expires_time_del: u64,
}

pub struct ProxyCacheFileNode {
    //这个是多线程共享锁, 只能做简单的业务
    pub ctx_thread: ArcRwLock<ProxyCacheFileNodeContext>,
    pub fix: Arc<ProxyCacheFileNodeFix>,
    pub file_ext: ArcRwLock<Arc<FileExt>>,
    pub buf_file: MsgReadBufFile,
    pub cache_file_info: Arc<ProxyCacheFileInfo>,
    pub response_info: Arc<HttpResponseInfo>,
}

impl ProxyCacheFileNode {
    pub fn get_file_ext(&self) -> Arc<FileExt> {
        self.file_ext.get().clone()
    }

    pub async fn create_file(
        client_uri: Uri,
        cache_file_info: Arc<ProxyCacheFileInfo>,
        response_info: Arc<HttpResponseInfo>,
        response: Response<Body>,
        curr_file_node_version: u64,
    ) -> Result<ProxyCacheFileNode> {
        let _directio = cache_file_info.directio;
        let content_range = &response_info.range;
        let bitmap = BitMap::from_slice(
            content_range.raw_content_length,
            cache_file_info.cache_file_slice,
        )?;
        log::debug!(
            "create bitmap: size:{}, slice_size:{}, str:{}",
            bitmap.size(),
            bitmap.slice_size,
            bitmap.to_string()
        );

        let bitmap_size = bitmap.size();

        let mut head = Vec::with_capacity(1024 * 16);

        head.extend_from_slice(CACHE_FILE_KEY.as_bytes());

        head.extend_from_slice("cache_control_time:".as_bytes());
        head.extend_from_slice(response_info.cache_control_time.to_be_bytes().as_slice());
        head.extend_from_slice("\r\n".as_bytes());

        head.extend_from_slice("expires_time:".as_bytes());
        head.extend_from_slice(response_info.expires_time.to_be_bytes().as_slice());
        head.extend_from_slice("\r\n".as_bytes());

        head.extend_from_slice("client_uri:".as_bytes());
        head.extend_from_slice(client_uri.to_string().as_bytes());
        head.extend_from_slice("\r\n".as_bytes());

        head.extend_from_slice("proxy_cache_path:".as_bytes());
        head.extend_from_slice(cache_file_info.proxy_cache_path.as_bytes());
        head.extend_from_slice("\r\n".as_bytes());

        head.extend_from_slice("proxy_cache_path_tmp:".as_bytes());
        head.extend_from_slice(cache_file_info.proxy_cache_path_tmp.as_bytes());
        head.extend_from_slice("\r\n".as_bytes());

        head.extend_from_slice("md5:".as_bytes());
        head.extend_from_slice(cache_file_info.md5.as_ref());
        head.extend_from_slice("\r\n".as_bytes());

        head.extend_from_slice("crc32:".as_bytes());
        head.extend_from_slice((cache_file_info.crc32 as u64).to_be_bytes().as_slice());
        head.extend_from_slice("\r\n".as_bytes());

        head.extend_from_slice("last_modified_time:".as_bytes());
        head.extend_from_slice(response_info.last_modified_time.to_be_bytes().as_slice());
        head.extend_from_slice("\r\n".as_bytes());

        head.extend_from_slice("last_modified:".as_bytes());
        head.extend_from_slice(response_info.last_modified.as_bytes());
        head.extend_from_slice("\r\n".as_bytes());

        head.extend_from_slice("e_tag:".as_bytes());
        head.extend_from_slice(response_info.e_tag.as_bytes());
        head.extend_from_slice("\r\n".as_bytes());

        head.extend_from_slice("raw_content_length:".as_bytes());
        head.extend_from_slice(
            response_info
                .range
                .raw_content_length
                .to_be_bytes()
                .as_slice(),
        );
        head.extend_from_slice("\r\n".as_bytes());

        head.extend_from_slice("directio:".as_bytes());
        head.extend_from_slice(_directio.to_be_bytes().as_slice());
        head.extend_from_slice("\r\n".as_bytes());

        head.extend_from_slice("\r\n".as_bytes());

        let file_head_size = head.len();
        let http_head_size = response_info.head.as_ref().unwrap().len();
        let bitmap_start = file_head_size + http_head_size;
        let mut body_start = file_head_size + http_head_size + bitmap_size + 4 + 4;
        let page = 4096;
        let align_body_start = (body_start % page) as u64;
        if align_body_start > 0 && align_body_start + content_range.raw_content_length > page as u64
        {
            body_start = (body_start / page + 1) * page;
        }

        log::debug!(
            "create file_head_size:{}, http_head_size:{}, bitmap_size:{}, self.body_star:{}",
            file_head_size,
            http_head_size,
            bitmap_size,
            body_start
        );

        head.extend_from_slice(response_info.head.as_ref().unwrap());
        head.extend_from_slice(bitmap.as_slice());
        head.extend_from_slice("\r\n".as_bytes());
        head.extend_from_slice("\r\n".as_bytes());
        head.resize(body_start as usize - 4, 0);
        head.extend_from_slice("\r\n".as_bytes());
        head.extend_from_slice("\r\n".as_bytes());

        let path = cache_file_info.proxy_cache_path_tmp.clone();
        let file_len = body_start as u64 + response_info.range.raw_content_length;
        let file_ext: Result<FileExt> = tokio::task::spawn_blocking(move || {
            #[cfg(feature = "anyio-file")]
            let start_time = Instant::now();

            let file = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .read(true)
                .truncate(true)
                .open(path.as_str())
                .map_err(|e| anyhow!("err:file.open => file_name:{}, e:{}", path, e))?;

            #[cfg(feature = "anyio-file")]
            if start_time.elapsed().as_millis() > 100 {
                log::info!(
                    "open file:{} => {}",
                    start_time.elapsed().as_millis(),
                    path.as_str()
                );
            }

            #[cfg(not(unix))]
            let file_fd = 0;
            #[cfg(unix)]
            use std::os::unix::io::AsRawFd;
            #[cfg(unix)]
            let file_fd = file.as_raw_fd();

            // let metadata = file
            //     .metadata()
            //     .map_err(|e| anyhow!("err:file.metadata => e:{}", e))?;
            // let _file_len = metadata.len();

            let file_uniq = FileUniq::new(&file)?;
            let file_ext = FileExt {
                async_lock: ArcMutexTokio::new(()),
                file: ArcMutex::new(file),
                fix: FileExtFix::new_arc(file_fd, file_uniq),
                file_path: path.clone(),
                file_len,
            };

            #[cfg(unix)]
            if _directio > 0 && file_len >= _directio {
                file_ext.directio_on()?;
            }

            #[cfg(feature = "anyio-file")]
            let start_time = Instant::now();
            file_ext.create_sparse_file(file_len)?;
            #[cfg(feature = "anyio-file")]
            if start_time.elapsed().as_millis() > 100 {
                log::info!(
                    "create_sparse_file:{} => file_name:{}",
                    start_time.elapsed().as_millis(),
                    path
                );
            }

            #[cfg(feature = "anyio-file")]
            let start_time = Instant::now();
            file_ext.file.get_mut().seek(std::io::SeekFrom::Start(0))?;
            file_ext.file.get_mut().write_all(head.as_slice())?;
            #[cfg(feature = "anyio-file")]
            if start_time.elapsed().as_millis() > 100 {
                log::info!(
                    "create file write head:{} => file_name:{}",
                    start_time.elapsed().as_millis(),
                    path
                );
            }

            Ok(file_ext)
        })
        .await?;
        let file_ext = file_ext?;
        let file_ext = Arc::new(file_ext);

        let content_range = &response_info.range;
        let buf_file = MsgReadBufFile::new(
            file_ext.clone(),
            body_start as u64,
            content_range.raw_content_length,
        );

        return Ok(ProxyCacheFileNode {
            ctx_thread: ArcRwLock::new(ProxyCacheFileNodeContext {
                cache_file_status: CacheFileStatus::Exist,
                cache_file_node_version: curr_file_node_version,
                bitmap: ArcRwLock::new(bitmap),
                bitmap_to_file: VecDeque::with_capacity(10),
                cache_control_time: response_info.cache_control_time,
                expires_time: response_info.expires_time,
                slice_upstream_map: ArcRwLock::new(HashMap::new()),
                expires_time_del: response_info.expires_time,
            }),
            fix: Arc::new(ProxyCacheFileNodeFix {
                response,
                file_head_size,
                http_head_size,
                bitmap_start,
                body_start,
            }),
            file_ext: ArcRwLock::new(file_ext),
            buf_file,
            cache_file_info,
            response_info,
        });
    }

    pub fn update_file_node_status(&self) {
        let curr_time = std::time::SystemTime::now();
        let curr_time = curr_time.duration_since(UNIX_EPOCH).unwrap().as_secs();

        let ctx = &mut *self.ctx_thread.get_mut();
        if ctx.expires_time <= curr_time {
            ctx.cache_file_status = CacheFileStatus::Expire;
        }
    }

    pub fn update_file_node_expires_time_del(&self) {
        let curr_time = std::time::SystemTime::now();
        let curr_time = curr_time.duration_since(UNIX_EPOCH).unwrap().as_secs();

        let ctx = &mut *self.ctx_thread.get_mut();
        let cache_control_time = if ctx.cache_control_time <= 0 {
            0
        } else {
            ctx.cache_control_time as u64
        };
        ctx.expires_time_del = curr_time + cache_control_time;
    }

    pub fn is_file_node_expires_del(&self) -> bool {
        let curr_time = std::time::SystemTime::now();
        let curr_time = curr_time.duration_since(UNIX_EPOCH).unwrap().as_secs();
        let ctx = &mut *self.ctx_thread.get_mut();
        if ctx.expires_time_del <= curr_time {
            return true;
        }
        return false;
    }

    pub fn get_file_head_time_str(cache_control_time: i64, expires_time: u64) -> Vec<u8> {
        let mut head = Vec::with_capacity(64);
        head.extend_from_slice(CACHE_FILE_KEY.as_ref());
        head.extend_from_slice("cache_control_time:".as_bytes());
        head.extend_from_slice(cache_control_time.to_be_bytes().as_slice());
        head.extend_from_slice("\r\n".as_bytes());

        head.extend_from_slice("expires_time:".as_bytes());
        head.extend_from_slice(expires_time.to_be_bytes().as_slice());
        head.extend_from_slice("\r\n".as_bytes());
        return head;
    }

    pub async fn from_file(
        cache_file_info: Arc<ProxyCacheFileInfo>,
        curr_file_node_version: u64,
    ) -> Result<Option<ProxyCacheFileNode>> {
        let ret = Self::open_http_cache_node_file(
            cache_file_info.proxy_cache_path.clone(),
            cache_file_info.directio,
        )
        .await;
        if ret.is_err() {
            return Ok(None);
        }
        let (file_ext, http_head_start, buf, file_head_size, expires_time, cache_control_time) =
            ret?;
        let file_ext = Arc::new(file_ext);

        let (file_res, http_head_size) =
            http_parse(&http_head_start).map_err(|_| anyhow!("http_parse"))?;
        let http_head = http_head_start.slice(0..http_head_size);

        let mut content_range = content_range(file_res.headers())
            .map_err(|e| anyhow!("err:content_length =>e:{}", e))?;

        let content_length = content_length(file_res.headers())
            .map_err(|e| anyhow!("err:content_length =>e:{}", e))?;

        if content_range.is_range {
            if content_range.content_length != content_length {
                return Err(anyhow!("content_range"));
            }
        } else {
            content_range.raw_content_length = content_length;
            content_range.content_length = content_length;
            if content_length > 0 {
                content_range.range_start = 0;
                content_range.range_end = content_length - 1;
            }
        }

        //let (cache_control_time, expires_time) = cache_control_time(file_res.headers())?;
        let e_tag = e_tag(file_res.headers()).map_err(|e| anyhow!("err:e_tag =>e:{}", e))?;
        let (last_modified, last_modified_time) = last_modified(file_res.headers())
            .map_err(|e| anyhow!("err:last_modified =>e:{}", e))?;

        let mut bitmap = BitMap::from_slice(
            content_range.raw_content_length,
            cache_file_info.cache_file_slice,
        )?;
        log::debug!(
            "create bitmap: size:{}, slice_size:{}, str:{}",
            bitmap.size(),
            bitmap.slice_size,
            bitmap.to_string()
        );

        let bitmap_size = bitmap.size();

        if buf.len() < file_head_size + http_head_size + bitmap_size {
            return Err(anyhow!(
                "buf.len() < file_head_size + http_head_size + bitmap_size"
            ));
        }
        let bitmap_start = file_head_size + http_head_size;

        bitmap
            .as_mut_slice()
            .copy_from_slice(&buf.slice(bitmap_start..bitmap_start + bitmap_size));
        bitmap.repair();

        log::debug!(
            "read bitmap: size:{}, slice_size:{}, str:{}",
            bitmap.size(),
            bitmap.slice_size,
            bitmap.to_string()
        );

        let mut body_start = file_head_size + http_head_size + bitmap_size + 4 + 4;
        let page = 4096;
        let align_body_start = (body_start % page) as u64;
        if align_body_start > 0 && align_body_start + content_range.raw_content_length > page as u64
        {
            body_start = (body_start / page + 1) * page;
        }
        log::debug!(
            "read file_head_size:{}, http_head_size:{}, bitmap_size:{}, self.body_star:{}",
            file_head_size,
            http_head_size,
            bitmap_size,
            body_start
        );

        let curr_time = std::time::SystemTime::now();
        let curr_time = curr_time.duration_since(UNIX_EPOCH).unwrap().as_secs();

        let cache_file_status = if expires_time <= curr_time {
            CacheFileStatus::Expire
        } else {
            CacheFileStatus::Exist
        };

        let buf_file = MsgReadBufFile::new(
            file_ext.clone(),
            body_start as u64,
            content_range.raw_content_length,
        );

        return Ok(Some(ProxyCacheFileNode {
            ctx_thread: ArcRwLock::new(ProxyCacheFileNodeContext {
                cache_file_status,
                cache_file_node_version: curr_file_node_version,
                bitmap: ArcRwLock::new(bitmap),
                bitmap_to_file: VecDeque::with_capacity(10),
                cache_control_time,
                expires_time,
                slice_upstream_map: ArcRwLock::new(HashMap::new()),
                expires_time_del: expires_time,
            }),
            fix: Arc::new(ProxyCacheFileNodeFix {
                response: file_res,
                file_head_size,
                http_head_size,
                bitmap_start,
                body_start,
            }),
            file_ext: ArcRwLock::new(file_ext),
            buf_file,
            cache_file_info,
            response_info: Arc::new(HttpResponseInfo {
                last_modified_time,
                last_modified,
                e_tag,
                cache_control_time,
                expires_time,
                range: content_range,
                head: Some(http_head),
            }),
        }));
    }

    pub fn copy(
        other: &Self,
        file_ext: FileExt,
        cache_file_info: Arc<ProxyCacheFileInfo>,
    ) -> Result<ProxyCacheFileNode> {
        let file_ext = Arc::new(file_ext);
        let buf_file = MsgReadBufFile::new(
            file_ext.clone(),
            other.fix.body_start as u64,
            other.response_info.range.raw_content_length,
        );
        return Ok(ProxyCacheFileNode {
            ctx_thread: other.ctx_thread.clone(),
            fix: other.fix.clone(),
            file_ext: ArcRwLock::new(file_ext),
            buf_file,
            cache_file_info,
            response_info: other.response_info.clone(),
        });
    }

    pub async fn open_http_cache_node_file(
        file_name: ArcString,
        _directio: u64,
    ) -> Result<(FileExt, Bytes, Bytes, usize, u64, i64)> {
        let file_ext = Self::open_file(file_name, 0).await?;
        let file_r = file_ext.file.clone();
        let ret: Result<BytesMut> = tokio::task::spawn_blocking(move || {
            use std::io::Read;
            #[cfg(feature = "anyio-file")]
            let start_time = Instant::now();
            let mut buf = BytesMut::zeroed(1024 * 16);
            let file_r = &mut *file_r.get_mut();
            file_r.seek(std::io::SeekFrom::Start(0))?;
            let size = file_r
                .read(buf.as_mut())
                .map_err(|e| anyhow!("err:file.read => e:{}", e))?;
            unsafe { buf.set_len(size) };

            #[cfg(feature = "anyio-file")]
            if start_time.elapsed().as_millis() > 100 {
                log::info!("open file read head:{}", start_time.elapsed().as_millis());
            }
            Ok(buf)
        })
        .await?;
        let buf = ret?;
        let buf = buf.freeze();

        //let mut client_uri  = Bytes::new();
        //let mut proxy_cache_path = Bytes::new();
        //let mut proxy_cache_path_tmp = Bytes::new();
        //let mut md5 = Bytes::new();
        //let mut crc32 = Bytes::new();
        let mut cache_control_time = Bytes::new();
        let mut expires_time = Bytes::new();
        let mut directio = Bytes::new();
        //let mut last_modified_time = Bytes::new();
        //let mut last_modified = Bytes::new();
        //let mut e_tag = Bytes::new();

        let pattern = "\r\n\r\n";
        let position = bytes_index(&buf, pattern.as_ref()).ok_or(anyhow!("read_head"))?;
        let file_head_size = position + pattern.len();
        let file_head = buf.slice(0..file_head_size);
        let http_head_start = buf.slice(file_head_size..);
        let mut is_file_ok = false;

        let pattern = "\r\n";
        let file_heads = bytes_split(&file_head.slice(0..position), pattern.as_ref());
        let pattern = ":";
        for v in file_heads {
            let vv = bytes_split_once(&v, pattern.as_ref());
            if vv.is_none() {
                return Err(anyhow!("v.is_none()"));
            }
            let (key, value) = vv.unwrap();

            if key == "cache_control_time" {
                cache_control_time = value.clone();
            } else if key == "expires_time" {
                expires_time = value.clone();
            } else if key == "directio" {
                directio = value.clone();
            } else if v.as_ref() == &CACHE_FILE_KEY.as_bytes()[0..CACHE_FILE_KEY.len() - 2] {
                is_file_ok = true;
            }

            // if key == "proxy_cache_path" {
            //     proxy_cache_path = value.clone();
            // } else if key == "proxy_cache_path_tmp" {
            //     proxy_cache_path_tmp = value.clone();
            // }else if key == "md5" {
            //     md5 = value.clone();
            // }else if key == "crc32" {
            //     crc32 = value.clone();
            // }else if key == "cache_control_time" {
            //     cache_control_time = value.clone();
            // }else if key == "last_modified_time" {
            //     last_modified_time = value.clone();
            // }else if key == "last_modified" {
            //     last_modified = value.clone();
            // }else if key == "e_tag" {
            //     e_tag = value.clone();
            // }
        }

        if !is_file_ok {
            return Err(anyhow!("err:open file fail"));
        }

        if cache_control_time.is_empty() {
            return Err(anyhow!("cache_control_time.is_empty"));
        }
        let mut fixed_bytes = [0u8; 8];
        fixed_bytes.copy_from_slice(&cache_control_time.as_ref()[0..8]);
        let cache_control_time = i64::from_be_bytes(fixed_bytes);

        if expires_time.is_empty() {
            return Err(anyhow!("expires_time.is_empty"));
        }
        let mut fixed_bytes = [0u8; 8];
        fixed_bytes.copy_from_slice(&expires_time.as_ref()[0..8]);
        let expires_time = u64::from_be_bytes(fixed_bytes);

        if directio.is_empty() {
            return Err(anyhow!("directio.is_empty"));
        }
        let mut fixed_bytes = [0u8; 8];
        fixed_bytes.copy_from_slice(&directio.as_ref()[0..8]);
        let _directio_from_file = u64::from_be_bytes(fixed_bytes);

        #[cfg(unix)]
        if _directio > 0 && _directio_from_file > 0 && file_ext.file_len >= _directio_from_file {
            file_ext.directio_on()?;
        }
        Ok((
            file_ext,
            http_head_start,
            buf,
            file_head_size,
            expires_time,
            cache_control_time,
        ))
    }

    pub async fn open_file(file_name: ArcString, _directio: u64) -> Result<FileExt> {
        let file_ext: Result<FileExt> = tokio::task::spawn_blocking(move || {
            #[cfg(feature = "anyio-file")]
            let start_time = Instant::now();
            let file = std::fs::OpenOptions::new()
                .write(true)
                .read(true)
                .open(file_name.as_str())
                .map_err(|e| anyhow!("err:file.open => file_name:{}, e:{}", file_name, e))?;

            #[cfg(feature = "anyio-file")]
            if start_time.elapsed().as_millis() > 100 {
                log::info!(
                    "open file:{} => {}",
                    start_time.elapsed().as_millis(),
                    file_name.as_str()
                );
            }

            #[cfg(not(unix))]
            let file_fd = 0;
            #[cfg(unix)]
            use std::os::unix::io::AsRawFd;
            #[cfg(unix)]
            let file_fd = file.as_raw_fd();

            let metadata = file
                .metadata()
                .map_err(|e| anyhow!("err:file.metadata => e:{}", e))?;
            let file_len = metadata.len();

            let file_uniq = FileUniq::new(&file)?;
            let file_ext = FileExt {
                async_lock: ArcMutexTokio::new(()),
                file: ArcMutex::new(file),
                fix: Arc::new(FileExtFix::new(file_fd, file_uniq)),
                file_path: file_name.clone(),
                file_len,
            };

            #[cfg(unix)]
            if _directio > 0 && file_len >= _directio {
                file_ext.directio_on()?;
            }

            Ok(file_ext)
        })
        .await?;
        let file_ext = file_ext?;

        return Ok(file_ext);
    }

    #[cfg(windows)]
    pub fn window_create_file(str: &str, file_len: u64) -> Result<std::fs::File> {
        #![allow(non_snake_case)]
        use std::io;
        use std::os::windows::fs::OpenOptionsExt;
        use std::os::windows::io::AsRawHandle;
        use winapi::um::winbase::FILE_FLAG_NO_BUFFERING;
        use winapi::um::winnt::HANDLE;
        use winapi::um::winnt::LARGE_INTEGER;
        let file_len = file_len as i64;

        let file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .truncate(true)
            .custom_flags(FILE_FLAG_NO_BUFFERING)
            .open(str)
            .map_err(|e| anyhow!("err:file.open => file_name:{}, e:{}", str, e))?;

        // 设置文件大小
        let file_handle = file.as_raw_handle() as HANDLE;
        let distance_to_move: LARGE_INTEGER = unsafe { std::mem::transmute(file_len) };
        let result = unsafe {
            winapi::um::fileapi::SetFilePointerEx(
                file_handle,
                distance_to_move,
                std::ptr::null_mut(),
                winapi::um::winbase::FILE_BEGIN,
            )
        };
        if result == 0 {
            return Err(io::Error::last_os_error().into());
        }

        // 确保文件大小设置成功
        let result = unsafe { winapi::um::fileapi::SetEndOfFile(file_handle) };
        if result == 0 {
            return Err(io::Error::last_os_error().into());
        }

        // 将文件标记为已分配但未实际分配磁盘空间
        let result = unsafe { winapi::um::fileapi::SetFileValidData(file_handle, file_len) };
        if result == 0 {
            return Err(io::Error::last_os_error().into());
        }

        Ok(file)
    }
}
