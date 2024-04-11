use crate::config::net_core_proxy::ProxyCacheConf;
use crate::proxy::http_proxy::http_cache_file_node::{ProxyCacheFileNode, CACHE_FILE_KEY};
use any_base::typ::ArcRwLock;
use any_base::typ::ArcRwLockTokio;
use any_base::util::{bytes_index, bytes_split, bytes_split_once, ArcString};
use anyhow::anyhow;
use anyhow::Result;
use bytes::{Bytes, BytesMut};
use lazy_static::lazy_static;
use std::collections::{HashMap, LinkedList, VecDeque};
use std::sync::atomic::{AtomicI64, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Instant;
//use hyper::body::HttpBody;
use crate::proxy::http_proxy::bitmap::align_bitset_ok;
use crate::proxy::http_proxy::http_stream_request::{
    CacheFileStatus, HttpCacheFileRequest, HttpStreamRequest,
};
use any_base::file_ext::FileExt;
use http::HeaderValue;
use hyper::{Body, Response};
use std::fs;
use std::io::Seek;
use std::mem::swap;
use std::path::Path;

pub const LOCAL_CACHE_REQ_KEY: &'static str = "local_cache_req_key";

lazy_static! {
    pub static ref FILE_CACHE_NODE_MANAGE_ID: Arc<AtomicU32> = Arc::new(AtomicU32::new(123456));
}

/*
net_core_proxy
状态值: MISS:未命中缓存 HIT:命中缓存 EXPIRED:缓存过期 STALE:命中了陈旧的缓存 REVALIDDATED:nginx验证陈旧缓存依然有效 UPDATING:内容陈旧,但正在更新 BYPASS:响应从原始服务器获取

proxy cache path 语法:proxy cache
path path
[levels=levels]
keys zone=name:size
[inactive=time1]
[max size=size2]
[loader files=number]
[loader sleep=time2]
[loader threshold=time3];
默认值:proxy cache path:off
*/

pub struct ProxyCacheFileNodeExpires {
    pub md5: Bytes,
    pub version_expires: u64,
    pub proxy_cache_path: ArcString,
    pub uri: hyper::Uri,
    pub raw_content_length: u64,
    pub expires_time: u64,
}
impl ProxyCacheFileNodeExpires {
    pub fn new(
        md5: Bytes,
        version_expires: u64,
        proxy_cache_path: ArcString,
        uri: hyper::Uri,
        raw_content_length: u64,
        expires_time: u64,
    ) -> Self {
        ProxyCacheFileNodeExpires {
            md5,
            version_expires,
            proxy_cache_path,
            uri,
            raw_content_length,
            expires_time,
        }
    }
}

pub struct ProxyCacheFileInfo {
    pub directio: u64,
    pub cache_file_slice: u64,
    pub md5: Bytes,
    pub crc32: usize,
    pub proxy_cache_path: ArcString,
    pub proxy_cache_path_tmp: ArcString,
}

pub struct ProxyCacheFileNodeHot {
    pub hot_minute_count: u64,
    pub hot_minute_time: Instant,
    pub hot_hour_count: u64,
    pub hot_hour_time: Instant,
    pub hot_history_count: u64,
}

pub struct ProxyCacheFileNodeManage {
    pub is_upstream: bool,
    pub upstream_version: i64,
    pub upstream_time: Instant,
    pub upstream_count: Arc<AtomicI64>,
    pub upstream_waits: VecDeque<tokio::sync::oneshot::Sender<()>>,
    pub cache_file_node_pool: LinkedList<Arc<ProxyCacheFileNode>>,
    pub cache_file_node: Option<Arc<ProxyCacheFileNode>>,
    pub cache_file_node_version: u64,
    pub version_expires: u64,
    pub is_last_upstream_cache: bool,
}

impl ProxyCacheFileNodeManage {
    pub fn new() -> Self {
        let cache_file_node_version =
            FILE_CACHE_NODE_MANAGE_ID.fetch_add(1, Ordering::Relaxed) as u64;
        let cache_file_node_version = (cache_file_node_version << 32) | 0;
        let version_expires = FILE_CACHE_NODE_MANAGE_ID.fetch_add(1, Ordering::Relaxed) as u64;
        let version_expires = (version_expires << 32) | 0;
        ProxyCacheFileNodeManage {
            is_upstream: false,
            upstream_version: 0,
            upstream_time: Instant::now(),
            upstream_count: Arc::new(AtomicI64::new(0)),
            cache_file_node_pool: LinkedList::new(),
            cache_file_node: None,
            cache_file_node_version,
            upstream_waits: VecDeque::with_capacity(10),
            version_expires,
            is_last_upstream_cache: true,
        }
    }
}

pub struct ProxyCacheContext {
    pub curr_size: i64,
    pub cache_file_node_expires: VecDeque<ProxyCacheFileNodeExpires>,
}

pub struct ProxyCache {
    pub ctx: ArcRwLock<ProxyCacheContext>,
    pub cache_file_node_map: ArcRwLock<HashMap<Bytes, ArcRwLockTokio<ProxyCacheFileNodeManage>>>,
    pub cache_conf: ProxyCacheConf,
    pub levels: Vec<usize>,
}

impl ProxyCache {
    pub fn new(cache_conf: ProxyCacheConf) -> Result<Self> {
        let mut level_vec = Vec::with_capacity(3);
        let levels = cache_conf.levels.split(":").collect::<Vec<&str>>();
        for v in levels {
            let n = v
                .trim()
                .parse::<usize>()
                .map_err(|e| anyhow!("err:proxy_cache => name:{}, e:{}", cache_conf.name, e))?;
            level_vec.push(n);
        }

        Ok(ProxyCache {
            ctx: ArcRwLock::new(ProxyCacheContext {
                curr_size: 0,
                cache_file_node_expires: VecDeque::with_capacity(1000),
            }),
            cache_file_node_map: ArcRwLock::new(HashMap::new()),
            cache_conf,
            levels: level_vec,
        })
    }

    pub async fn load(&mut self) -> Result<()> {
        let mut file_full_names = Vec::with_capacity(1000);
        let file_full_name = if &self.cache_conf.path[self.cache_conf.path.len() - 1..] == "/" {
            &self.cache_conf.path[0..self.cache_conf.path.len() - 1]
        } else {
            &self.cache_conf.path
        };
        Self::load_file_full_path(file_full_name, &mut file_full_names)?;
        log::debug!("file_full_names: {:?}", file_full_names);

        for file_full_name in file_full_names {
            let proxy_cache_path = ArcString::new(file_full_name);
            let (raw_content_length, uri, md5, expires_time) =
                Self::load_cache_file(proxy_cache_path.clone()).await?;
            let (_, manage) = HttpCacheFile::read_cache_file_node_manage(self, &md5);
            let version_expires = manage.get().await.version_expires;
            let uri = hyper::Uri::try_from(uri.as_ref())?;
            let proxy_cache_ctx = &mut *self.ctx.get_mut();
            proxy_cache_ctx.curr_size += raw_content_length as i64;
            proxy_cache_ctx
                .cache_file_node_expires
                .push_back(ProxyCacheFileNodeExpires::new(
                    md5,
                    version_expires,
                    proxy_cache_path,
                    uri,
                    raw_content_length,
                    expires_time,
                ));
        }
        let proxy_cache_ctx = &mut *self.ctx.get_mut();
        proxy_cache_ctx
            .cache_file_node_expires
            .make_contiguous()
            .sort_by(|a, b| a.expires_time.cmp(&b.expires_time));
        Ok(())
    }

    pub async fn load_cache_file(file_full_name: ArcString) -> Result<(u64, Bytes, Bytes, u64)> {
        let file_ext = ProxyCacheFileNode::open_file(file_full_name, 0).await?;
        let file_ext = ArcRwLock::new(file_ext);

        let file_r = file_ext.get().file.clone();
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

        let mut client_uri = Bytes::new();
        let mut raw_content_length = Bytes::new();
        let mut md5 = Bytes::new();
        let mut expires_time = Bytes::new();

        let pattern = "\r\n\r\n";
        let position = bytes_index(&buf, pattern.as_ref()).ok_or(anyhow!("read_head"))?;
        let file_head_size = position + pattern.len();
        let file_head = buf.slice(0..file_head_size);
        let _http_head_start = buf.slice(file_head_size..);
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

            if key == "client_uri" {
                client_uri = value.clone();
            } else if key == "raw_content_length" {
                raw_content_length = value.clone();
            } else if key == "md5" {
                md5 = value.clone();
            } else if key == "expires_time" {
                expires_time = value.clone();
            } else if v.as_ref() == &CACHE_FILE_KEY.as_bytes()[0..CACHE_FILE_KEY.len() - 2] {
                is_file_ok = true;
            }
        }

        if !is_file_ok {
            return Err(anyhow!("err:open file fail"));
        }

        if raw_content_length.is_empty() {
            return Err(anyhow!("raw_content_length.is_empty"));
        }
        let mut fixed_bytes = [0u8; 8];
        fixed_bytes.copy_from_slice(&raw_content_length.as_ref()[0..8]);
        let raw_content_length = u64::from_be_bytes(fixed_bytes);

        if expires_time.is_empty() {
            return Err(anyhow!("expires_time.is_empty"));
        }
        let mut fixed_bytes = [0u8; 8];
        fixed_bytes.copy_from_slice(&expires_time.as_ref()[0..8]);
        let expires_time = u64::from_be_bytes(fixed_bytes);

        if client_uri.is_empty() {
            return Err(anyhow!("client_uri.is_empty"));
        }

        if md5.is_empty() {
            return Err(anyhow!("md5.is_empty"));
        }
        Ok((raw_content_length, client_uri, md5, expires_time))
    }

    pub fn load_file_full_path(
        file_full_path: &str,
        file_full_names: &mut Vec<String>,
    ) -> Result<()> {
        let dir_path = Path::new(file_full_path);
        for entry in fs::read_dir(dir_path)? {
            let entry = entry?;
            let path = entry.path();
            // 判断是否是目录
            if path.is_dir() {
                let name = path.file_name().unwrap().to_str().unwrap();
                let file_full_path = format!("{}/{}", file_full_path, name);
                log::debug!("{}", file_full_path);
                Self::load_file_full_path(&file_full_path, file_full_names)?;
            } else {
                let name = path.file_name().unwrap().to_str().unwrap();
                let file_full_name = format!("{}/{}", file_full_path, name);
                log::debug!("{}", file_full_name);
                file_full_names.push(file_full_name);
            }
        }
        Ok(())
    }
}

pub struct HttpCacheFileContext {
    pub cache_file_node_manage: ArcRwLockTokio<ProxyCacheFileNodeManage>,
    pub cache_file_node: Option<Arc<ProxyCacheFileNode>>,
    pub cache_file_node_version: u64,
    pub cache_file_status: Option<CacheFileStatus>,
}

#[derive(Clone)]
pub struct HttpCacheFile {
    //这个是多线程共享锁, 只能做简单的业务
    pub ctx_thread: ArcRwLock<HttpCacheFileContext>,
    pub proxy_cache: Option<Arc<ProxyCache>>,
    pub cache_file_info: Arc<ProxyCacheFileInfo>,
}

impl HttpCacheFile {
    pub fn cache_file_request_head(&self) -> Result<HttpCacheFileRequest> {
        let cache_file_node = self.ctx_thread.get().cache_file_node.clone().unwrap();
        let response_info = &cache_file_node.response_info;
        let mut response = Response::builder().body(Body::empty())?;
        *response.status_mut() = cache_file_node.fix.response.status().clone();
        *response.version_mut() = cache_file_node.fix.response.version().clone();
        *response.headers_mut() = cache_file_node.fix.response.headers().clone();
        response.headers_mut().insert(
            http::header::CONTENT_RANGE,
            HeaderValue::from_bytes(
                format!(
                    "bytes {}-{}/{}",
                    0,
                    response_info.range.raw_content_length - 1,
                    response_info.range.raw_content_length
                )
                .as_bytes(),
            )?,
        );
        response.headers_mut().insert(
            http::header::CONTENT_LENGTH,
            HeaderValue::from(response_info.range.raw_content_length),
        );
        let is_body = false;
        let buf_file = if is_body {
            Some(cache_file_node.buf_file.clone())
        } else {
            None
        };
        Ok(HttpCacheFileRequest { response, buf_file })
    }

    pub fn cache_file_request_get(
        &self,
        range_start: u64,
        range_end: u64,
        slice_end: u64,
    ) -> Option<HttpCacheFileRequest> {
        let cache_file_node = self.ctx_thread.get().cache_file_node.clone().unwrap();
        let response_info = &cache_file_node.response_info;
        let range_end = {
            let bitmap = cache_file_node.ctx_thread.get().bitmap.clone();
            let bitmap = &*bitmap.get();
            if !bitmap.is_full() {
                let ret = align_bitset_ok(
                    bitmap,
                    range_start,
                    range_end,
                    cache_file_node.cache_file_info.cache_file_slice,
                );

                if ret.is_err() || !ret.unwrap() {
                    return None;
                }
                range_end
            } else {
                slice_end
            }
        };

        let mut response = Response::builder().body(Body::empty()).ok()?;
        *response.status_mut() = cache_file_node.fix.response.status().clone();
        *response.version_mut() = cache_file_node.fix.response.version().clone();
        *response.headers_mut() = cache_file_node.fix.response.headers().clone();
        response.headers_mut().insert(
            http::header::CONTENT_RANGE,
            HeaderValue::from_bytes(
                format!(
                    "bytes {}-{}/{}",
                    range_start, range_end, response_info.range.raw_content_length
                )
                .as_bytes(),
            )
            .ok()?,
        );

        response.headers_mut().insert(
            http::header::CONTENT_LENGTH,
            http::header::HeaderValue::from(range_end - range_start + 1),
        );
        let is_body = true;
        let buf_file = if is_body {
            Some(cache_file_node.buf_file.clone())
        } else {
            None
        };
        Some(HttpCacheFileRequest { response, buf_file })
    }

    pub fn cache_file_request_not_get(&self) -> Option<HttpCacheFileRequest> {
        let cache_file_node = self.ctx_thread.get().cache_file_node.clone().unwrap();
        let response_info = &cache_file_node.response_info;
        {
            let bitmap = cache_file_node.ctx_thread.get().bitmap.clone();
            let is_full = bitmap.get().is_full();
            if !is_full {
                log::error!("err:file_response_not_get !cache_file_node_ctx.bitmap.is_full()");
                return None;
            }
        }

        let mut response = Response::builder().body(Body::empty()).ok()?;
        *response.status_mut() = cache_file_node.fix.response.status().clone();
        *response.version_mut() = cache_file_node.fix.response.version().clone();
        *response.headers_mut() = cache_file_node.fix.response.headers().clone();
        response.headers_mut().insert(
            http::header::CONTENT_LENGTH,
            http::header::HeaderValue::from(response_info.range.raw_content_length),
        );
        let is_body = true;
        let buf_file = if is_body {
            Some(cache_file_node.buf_file.clone())
        } else {
            None
        };
        Some(HttpCacheFileRequest { response, buf_file })
    }

    pub async fn load_cache_file(
        r: &Arc<HttpStreamRequest>,
        proxy_cache: &Arc<ProxyCache>,
        cache_file_info: &Arc<ProxyCacheFileInfo>,
    ) -> Result<()> {
        let (is_new, cache_file_node_manage) =
            Self::read_cache_file_node_manage(proxy_cache, &cache_file_info.md5);
        let (is_load_cache_file, cache_file_node) =
            Self::do_get_cache_file_node(cache_file_info, &cache_file_node_manage, true).await?;
        if is_new && is_load_cache_file {
            let cache_file_node_manage_ = cache_file_node_manage.clone();
            let cache_file_node_manage_ = &*cache_file_node_manage_.get().await;
            let cache_file_node = cache_file_node.unwrap();
            let version_expires = cache_file_node_manage_.version_expires;
            let md5 = cache_file_node.cache_file_info.md5.clone();
            let proxy_cache_path = cache_file_node.cache_file_info.proxy_cache_path.clone();
            let uri = r.ctx.get().r_in.uri.clone();
            let proxy_cache = r.http_cache_file.proxy_cache.as_ref().unwrap();
            let proxy_cache_ctx = &mut *proxy_cache.ctx.get_mut();
            proxy_cache_ctx.curr_size +=
                cache_file_node.response_info.range.raw_content_length as i64;
            let expires_time = cache_file_node.ctx_thread.get().expires_time;
            proxy_cache_ctx
                .cache_file_node_expires
                .push_back(ProxyCacheFileNodeExpires::new(
                    md5,
                    version_expires,
                    proxy_cache_path,
                    uri,
                    cache_file_node.response_info.range.raw_content_length,
                    expires_time,
                ));

            let cache_file_node_version = cache_file_node.ctx_thread.get().cache_file_node_version;
            let cache_file_status = cache_file_node.ctx_thread.get().cache_file_status.clone();
            let http_cache_file_ctx_thread = HttpCacheFileContext {
                cache_file_node_manage,
                cache_file_node: Some(cache_file_node),
                cache_file_node_version,
                cache_file_status: Some(cache_file_status),
            };
            r.http_cache_file.ctx_thread.set(http_cache_file_ctx_thread);
        }
        Ok(())
    }

    pub fn read_cache_file_node_manage(
        proxy_cache: &ProxyCache,
        md5: &Bytes,
    ) -> (bool, ArcRwLockTokio<ProxyCacheFileNodeManage>) {
        let cache_file_node_manage = proxy_cache.cache_file_node_map.get().get(md5).cloned();
        if cache_file_node_manage.is_some() {
            return (false, cache_file_node_manage.unwrap());
        }

        let cache_file_node_map = &mut *proxy_cache.cache_file_node_map.get_mut();
        let cache_file_node_manage = cache_file_node_map.get_mut(md5).cloned();
        if cache_file_node_manage.is_none() {
            let cache_file_node_manage = ArcRwLockTokio::new(ProxyCacheFileNodeManage::new());
            cache_file_node_map.insert(md5.clone(), cache_file_node_manage.clone());
            (true, cache_file_node_manage)
        } else {
            (false, cache_file_node_manage.unwrap())
        }
    }

    pub async fn set_cache_file_node(
        &self,
        cache_file_node: Arc<ProxyCacheFileNode>,
    ) -> Result<bool> {
        let (_, cache_file_node_manage) = Self::read_cache_file_node_manage(
            self.proxy_cache.as_ref().unwrap(),
            &self.cache_file_info.md5,
        );
        let cache_file_node_manage = &mut *cache_file_node_manage.get_mut().await;

        if self.ctx_thread.get().cache_file_node_version
            != cache_file_node_manage.cache_file_node_version
        {
            return Ok(false);
        }

        cache_file_node_manage.cache_file_node_version += 1;
        self.ctx_thread.get_mut().cache_file_node_version += 1;
        cache_file_node.ctx_thread.get_mut().cache_file_node_version += 1;
        cache_file_node_manage.cache_file_node = Some(cache_file_node.clone());
        cache_file_node_manage.cache_file_node_pool.clear();
        cache_file_node_manage.is_upstream = false;

        let mut upstream_waits = VecDeque::with_capacity(10);
        swap(
            &mut upstream_waits,
            &mut cache_file_node_manage.upstream_waits,
        );
        for tx in upstream_waits {
            let _ = tx.send(());
        }

        let (proxy_cache_path_tmp, proxy_cache_path) = {
            let cache_file_info = &cache_file_node.cache_file_info;
            (
                cache_file_info.proxy_cache_path_tmp.clone(),
                cache_file_info.proxy_cache_path.clone(),
            )
        };

        let proxy_cache_path_ = proxy_cache_path.clone();

        let ret: Result<()> = tokio::task::spawn_blocking(move || {
            #[cfg(feature = "anyio-file")]
            let start_time = Instant::now();
            std::fs::rename(proxy_cache_path_tmp.as_str(), proxy_cache_path.as_str())?;
            #[cfg(feature = "anyio-file")]
            if start_time.elapsed().as_millis() > 100 {
                log::info!(
                    "std::fs::rename file time:{} => {:?}",
                    start_time.elapsed().as_millis(),
                    proxy_cache_path.as_str()
                );
            }
            Ok(())
        })
        .await?;
        ret?;

        let file_ext = cache_file_node.get_file_ext();
        let file_ext = Arc::new(FileExt {
            async_lock: file_ext.async_lock.clone(),
            file: file_ext.file.clone(),
            fix: file_ext.fix.clone(),
            file_path: proxy_cache_path_,
            file_len: file_ext.file_len,
        });
        cache_file_node.file_ext.set(file_ext);

        return Ok(true);
    }

    pub async fn cache_file_node_to_pool(
        &self,
        cache_file_node: Arc<ProxyCacheFileNode>,
    ) -> Result<()> {
        let (_, cache_file_node_manage) = Self::read_cache_file_node_manage(
            self.proxy_cache.as_ref().unwrap(),
            &self.cache_file_info.md5,
        );
        let cache_file_node_manage = &mut *cache_file_node_manage.get_mut().await;
        if cache_file_node_manage.cache_file_node.is_none() {
            return Ok(());
        }

        if self.ctx_thread.get().cache_file_node_version
            != cache_file_node_manage.cache_file_node_version
        {
            return Ok(());
        }

        cache_file_node_manage
            .cache_file_node_pool
            .push_back(cache_file_node);
        return Ok(());
    }

    pub async fn get_cache_file_node(
        &self,
    ) -> Result<(
        bool,
        Option<CacheFileStatus>,
        ArcRwLockTokio<ProxyCacheFileNodeManage>,
        Option<Arc<ProxyCacheFileNode>>,
        i64,
        Option<Arc<AtomicI64>>,
    )> {
        let (_, cache_file_node_manage) = Self::read_cache_file_node_manage(
            self.proxy_cache.as_ref().unwrap(),
            &self.cache_file_info.md5,
        );

        let mut upstream_version = -1;
        let mut is_open_file = true;
        let once_time = 1000 * 10;
        let mut is_ok = false;
        loop {
            let (_, cache_file_node) = Self::do_get_cache_file_node(
                &self.cache_file_info,
                &cache_file_node_manage,
                is_open_file,
            )
            .await?;
            is_open_file = false;
            if cache_file_node.is_some() {
                let cache_file_node = cache_file_node.unwrap();
                let cache_file_status = cache_file_node.ctx_thread.get().cache_file_status.clone();
                let is_ok = match &cache_file_status {
                    &CacheFileStatus::Exist => true,
                    &CacheFileStatus::Expire => {
                        //如何让外部指定要回源
                        let is_upstream = cache_file_node_manage.get().await.is_upstream;
                        let bitmap = cache_file_node.ctx_thread.get().bitmap.clone();
                        let is_full = bitmap.get().is_full();
                        if is_upstream && is_full {
                            true
                        } else {
                            is_ok
                        }
                    }
                };
                if is_ok {
                    let pool_cache_file_node = {
                        let cache_file_node_manage = &mut *cache_file_node_manage.get_mut().await;
                        cache_file_node_manage.cache_file_node_pool.pop_front()
                    };

                    let cache_file_node = if pool_cache_file_node.is_none() {
                        let file_ext = ProxyCacheFileNode::open_http_cache_node_file(
                            self.cache_file_info.proxy_cache_path.clone(),
                            self.cache_file_info.directio,
                        )
                        .await;
                        //如果文件不存在清空， 文件存在断开链接，  文件变化清空
                        if let Err(e) = file_ext {
                            if Path::new(self.cache_file_info.proxy_cache_path.as_str()).exists() {
                                return Err(anyhow!(
                                    "err:open file => path:{}, err:{}",
                                    self.cache_file_info.proxy_cache_path,
                                    e
                                ));
                            }
                            let (_, cache_file_node) = Self::do_get_cache_file_node(
                                &self.cache_file_info,
                                &cache_file_node_manage,
                                false,
                            )
                            .await?;
                            if cache_file_node.is_some() {
                                let cache_file_node_manage =
                                    &mut *cache_file_node_manage.get_mut().await;
                                cache_file_node_manage.cache_file_node_pool.clear();
                                cache_file_node_manage.cache_file_node = None;
                            }
                            continue;
                        }
                        let (file_ext, ..) = file_ext?;
                        let file_ext_ = cache_file_node.get_file_ext();

                        //可能外部改变了， 需要替换
                        if !file_ext_.is_uniq_eq(&file_ext.fix) {
                            let (_, cache_file_node) = Self::do_get_cache_file_node(
                                &self.cache_file_info,
                                &cache_file_node_manage,
                                true,
                            )
                            .await?;
                            if cache_file_node.is_none() {
                                continue;
                            }
                            let cache_file_node = cache_file_node.unwrap();
                            let file_ext_ = cache_file_node.get_file_ext();
                            if !file_ext_.is_uniq_eq(&file_ext.fix) {
                                let cache_file_node_manage =
                                    &mut *cache_file_node_manage.get_mut().await;
                                cache_file_node_manage.cache_file_node_pool.clear();
                                cache_file_node_manage.cache_file_node = None;
                            }
                            continue;
                        }

                        let cache_file_node = ProxyCacheFileNode::copy(
                            &cache_file_node,
                            file_ext,
                            self.cache_file_info.clone(),
                        )?;
                        Arc::new(cache_file_node)
                    } else {
                        pool_cache_file_node.unwrap()
                    };

                    let cache_file_status =
                        cache_file_node.ctx_thread.get().cache_file_status.clone();
                    let (is_upstream, upstream_count, upstream_count_drop) =
                        match &cache_file_status {
                            &CacheFileStatus::Exist => (false, 0, None),
                            &CacheFileStatus::Expire => {
                                let cache_file_node_manage =
                                    &mut *cache_file_node_manage.get_mut().await;
                                //如何让外部指定要回源
                                let bitmap = cache_file_node.ctx_thread.get().bitmap.clone();
                                let is_full = bitmap.get().is_full();
                                if cache_file_node_manage.is_upstream == true
                                    && cache_file_node_manage.upstream_time.elapsed().as_secs()
                                        < once_time
                                    && is_full
                                {
                                    (false, 0, None)
                                } else {
                                    cache_file_node_manage.is_upstream = true;
                                    cache_file_node_manage.upstream_version += 1;
                                    let upstream_count = cache_file_node_manage
                                        .upstream_count
                                        .fetch_add(1, Ordering::Relaxed)
                                        + 1;
                                    cache_file_node_manage.upstream_time = Instant::now();
                                    (
                                        true,
                                        upstream_count,
                                        Some(cache_file_node_manage.upstream_count.clone()),
                                    )
                                }
                            }
                        };

                    return Ok((
                        is_upstream,
                        Some(cache_file_status),
                        cache_file_node_manage,
                        Some(cache_file_node),
                        upstream_count,
                        upstream_count_drop,
                    ));
                }
            } else {
                if is_ok {
                    let cache_file_node_manage_ = &mut *cache_file_node_manage.get_mut().await;
                    cache_file_node_manage_.is_upstream = true;
                    cache_file_node_manage_.upstream_version += 1;
                    let upstream_count = cache_file_node_manage_
                        .upstream_count
                        .fetch_add(1, Ordering::Relaxed)
                        + 1;

                    return Ok((
                        true,
                        None,
                        cache_file_node_manage.clone(),
                        None,
                        upstream_count,
                        Some(cache_file_node_manage_.upstream_count.clone()),
                    ));
                }
            }

            let rx = {
                let cache_file_node_manage = &mut *cache_file_node_manage.get_mut().await;
                if !cache_file_node_manage.is_upstream
                    || (upstream_version >= 0
                        && upstream_version == cache_file_node_manage.upstream_version)
                {
                    is_ok = true;
                    continue;
                }
                upstream_version = cache_file_node_manage.upstream_version;
                let (tx, rx) = tokio::sync::oneshot::channel();
                cache_file_node_manage.upstream_waits.push_back(tx);
                rx
            };

            tokio::select! {
                biased;
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(once_time)) => {
                }
                _ =  rx => {
                }
                else => {
                    return Err(anyhow!("err:select"));
                }
            }
        }
    }

    pub async fn do_get_cache_file_node(
        cache_file_info: &Arc<ProxyCacheFileInfo>,
        cache_file_node_manage: &ArcRwLockTokio<ProxyCacheFileNodeManage>,
        is_open_file: bool,
    ) -> Result<(bool, Option<Arc<ProxyCacheFileNode>>)> {
        let cache_file_node = cache_file_node_manage.get().await.cache_file_node.clone();
        if cache_file_node.is_some() {
            let cache_file_node = cache_file_node.unwrap();
            cache_file_node.update_file_node_status();
            return Ok((false, Some(cache_file_node)));
        }

        let cache_file_node_manage = &mut *cache_file_node_manage.get_mut().await;
        let cache_file_node = cache_file_node_manage.cache_file_node.clone();
        if cache_file_node.is_some() {
            let cache_file_node = cache_file_node.unwrap();
            cache_file_node.update_file_node_status();
            return Ok((false, Some(cache_file_node)));
        }
        if !is_open_file {
            return Ok((false, None));
        }

        let cache_file_node = ProxyCacheFileNode::from_file(
            cache_file_info.clone(),
            cache_file_node_manage.cache_file_node_version,
        )
        .await?;
        if cache_file_node.is_none() {
            return Ok((false, None));
        }
        let cache_file_node = cache_file_node.unwrap();
        let cache_file_node = Arc::new(cache_file_node);
        cache_file_node_manage.cache_file_node = Some(cache_file_node.clone());
        Ok((true, Some(cache_file_node)))
    }
}
