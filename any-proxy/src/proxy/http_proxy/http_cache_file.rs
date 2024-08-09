use crate::config::net_core_proxy::{ProxyCacheConf, ProxyHotFile, CACHE_FILE_SLISE};
use crate::proxy::http_proxy::http_cache_file_node::{ProxyCacheFileNode, CACHE_FILE_KEY};
use any_base::typ::ArcMutex;
use any_base::typ::{ArcRwLock, OptionExt};
use any_base::typ2;
use any_base::util::{bytes_index, bytes_split, bytes_split_once, ArcString};
use anyhow::anyhow;
use anyhow::Result;
use bytes::{Bytes, BytesMut};
use lazy_static::lazy_static;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
//use hyper::body::HttpBody;
use crate::proxy::http_proxy::bitmap::align_bitset_ok;
use crate::proxy::http_proxy::http_stream_request::{
    CacheFileStatus, HttpCacheFileRequest, HttpStreamRequest,
};
use crate::proxy::StreamConfigContext;
use any_base::file_ext::{unlink, FileExt};
use any_base::io::async_write_msg::MsgReadBufFile;
use any_base::module::module::Modules;
use http::HeaderValue;
use hyper::{Body, Response};
use std::fs;
use std::io::Seek;
use std::mem::swap;
use std::path::Path;
use std::time::UNIX_EPOCH;

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
    pub cache_file_node_version: u64,
    pub cache_file_node_head: Arc<ProxyCacheFileNodeHead>,
}

impl ProxyCacheFileNodeExpires {
    pub fn new(
        md5: Bytes,
        version_expires: u64,
        cache_file_node_version: u64,
        cache_file_node_head: Arc<ProxyCacheFileNodeHead>,
    ) -> Self {
        ProxyCacheFileNodeExpires {
            md5,
            version_expires,
            cache_file_node_version,
            cache_file_node_head,
        }
    }
}

pub struct ProxyCacheFileHotValue {
    pub md5: Bytes,
    pub hot_count: Arc<AtomicU64>,
    pub is_hot: Arc<AtomicBool>,
}

impl ProxyCacheFileHotValue {
    pub fn new(md5: Bytes, hot_count: Arc<AtomicU64>, is_hot: Arc<AtomicBool>) -> Self {
        ProxyCacheFileHotValue {
            md5,
            hot_count,
            is_hot,
        }
    }
}

pub struct ProxyCacheFileHot {
    pub values: VecDeque<ProxyCacheFileHotValue>,
    pub is_ok: Arc<AtomicBool>,
}

impl ProxyCacheFileHot {
    pub fn new() -> Self {
        ProxyCacheFileHot {
            values: VecDeque::with_capacity(1000),
            is_ok: Arc::new(AtomicBool::new(true)),
        }
    }
}

pub struct ProxyCacheFileNodeHead {
    pub md5: Bytes,
    pub md5_str: Bytes,
    pub proxy_cache_path: ArcString,
    pub proxy_cache_path_tmp: ArcString,
    pub uri: hyper::Uri,
    pub method: http::method::Method,
    pub raw_content_length: u64,
    pub trie_url: String,
    pub directio: u64,
    pub cache_file_slice: u64,
    //文件实际过期时间，会被更新
    pub expires_time: AtomicU64,
    pub cache_control_time: AtomicI64,
}

impl ProxyCacheFileNodeHead {
    pub fn new(
        md5: Bytes,
        md5_str: Bytes,
        proxy_cache_path: ArcString,
        proxy_cache_path_tmp: ArcString,
        uri: hyper::Uri,
        method: http::method::Method,
        raw_content_length: u64,
        expires_time: u64,
        trie_url: String,
        directio: u64,
        cache_file_slice: u64,
        cache_control_time: i64,
    ) -> Self {
        ProxyCacheFileNodeHead {
            md5,
            md5_str,
            proxy_cache_path,
            proxy_cache_path_tmp,
            uri,
            method,
            raw_content_length,
            expires_time: AtomicU64::new(expires_time),
            trie_url,
            directio,
            cache_file_slice,
            cache_control_time: AtomicI64::new(cache_control_time),
        }
    }

    pub fn is_expires(&self) -> bool {
        let curr_time = std::time::SystemTime::now();
        let curr_time = curr_time.duration_since(UNIX_EPOCH).unwrap().as_secs();
        if self.expires_time.load(Ordering::Relaxed) <= curr_time {
            return true;
        }
        return false;
    }

    pub fn expires_time(&self) -> u64 {
        self.expires_time.load(Ordering::Relaxed)
    }

    pub fn set_expires_time(&self, expires_time: u64) {
        self.expires_time.store(expires_time, Ordering::Relaxed);
    }

    pub fn cache_control_time(&self) -> i64 {
        self.cache_control_time.load(Ordering::Relaxed)
    }

    pub fn set_cache_control_time(&self, cache_control_time: i64) {
        self.cache_control_time
            .store(cache_control_time, Ordering::Relaxed);
    }
}

pub struct ProxyCacheFileInfo {
    //配置文件里面的
    pub directio: u64,
    pub cache_file_slice: u64,
    pub md5: Bytes,
    pub md5_str: Bytes,
    pub crc32: usize,
    pub proxy_cache_path: ArcString,
    pub proxy_cache_path_tmp: ArcString,
    pub trie_url: String,
}

pub struct ProxyCacheFileNodeHot {
    pub hot_minute_count: u64,
    pub hot_minute_time: Instant,
    pub hot_hour_count: u64,
    pub hot_hour_time: Instant,
    pub hot_history_count: u64,
}

pub struct ProxyCacheFileNodeData {
    pub count: AtomicU64,
    pub node: ArcMutex<Arc<ProxyCacheFileNode>>,
}

impl ProxyCacheFileNodeData {
    pub fn new(node: Option<Arc<ProxyCacheFileNode>>) -> Self {
        if node.is_some() {
            ProxyCacheFileNodeData {
                count: AtomicU64::new(0),
                node: ArcMutex::new(node.unwrap()),
            }
        } else {
            ProxyCacheFileNodeData {
                count: AtomicU64::new(0),
                node: ArcMutex::default(),
            }
        }
    }
}

pub struct ProxyCacheFileNodeManage {
    pub is_upstream: bool,
    pub upstream_version: i64,
    pub upstream_time: Instant,
    pub upstream_count: Arc<AtomicI64>,
    pub upstream_waits: VecDeque<tokio::sync::oneshot::Sender<()>>,
    pub cache_file_node_queue: VecDeque<Arc<ProxyCacheFileNodeData>>,
    pub cache_file_node: Option<Arc<ProxyCacheFileNode>>,
    pub cache_file_node_version: u64,
    pub version_expires: u64,
    pub is_last_upstream_cache: bool,
    pub cache_file_node_head: OptionExt<Arc<ProxyCacheFileNodeHead>>,
    pub hot_count: Arc<AtomicU64>,
    pub hot_is_ok: Arc<AtomicBool>,
    pub hot_is_hot: Arc<AtomicBool>,
    pub cache_file_node_queue_lru: ArcMutex<VecDeque<Arc<ProxyCacheFileNodeData>>>,
    pub proxy_cache_name: ArcString,
}

impl Drop for ProxyCacheFileNodeManage {
    fn drop(&mut self) {
        let mut upstream_waits = VecDeque::with_capacity(10);
        swap(&mut upstream_waits, &mut self.upstream_waits);
        for tx in upstream_waits {
            let _ = tx.send(());
        }
    }
}

impl ProxyCacheFileNodeManage {
    pub fn new(
        cache_file_node_queue: ArcMutex<VecDeque<Arc<ProxyCacheFileNodeData>>>,
        proxy_cache_name: ArcString,
    ) -> Self {
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
            cache_file_node: None,
            cache_file_node_version,
            upstream_waits: VecDeque::with_capacity(10),
            version_expires,
            is_last_upstream_cache: true,
            cache_file_node_head: None.into(),
            hot_count: Arc::new(AtomicU64::new(0)),
            hot_is_ok: Arc::new(AtomicBool::new(false)),
            hot_is_hot: Arc::new(AtomicBool::new(false)),
            cache_file_node_queue: VecDeque::new(),
            cache_file_node_queue_lru: cache_file_node_queue,
            proxy_cache_name,
        }
    }

    pub fn update_version_expires(&mut self) {
        self.version_expires += 1;
    }

    pub fn cache_file_node_queue_clear(&mut self) {
        self.cache_file_node_queue.clear();
    }
}

pub struct ProxyCacheContext {
    pub curr_size: i64,
    pub cache_file_node_expires: VecDeque<ProxyCacheFileNodeExpires>,
    pub hots: VecDeque<ProxyCacheFileHot>,
    pub hot_instant: Instant,
}

pub struct ProxyCache {
    pub ctx: ArcRwLock<ProxyCacheContext>,
    pub cache_file_node_map:
        ArcRwLock<HashMap<Bytes, typ2::ArcRwLockTokio<ProxyCacheFileNodeManage>>>,
    pub cache_conf: ProxyCacheConf,
    pub levels: Vec<usize>,
    pub name: ArcString,
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

        let mut hots = VecDeque::with_capacity(10);
        hots.push_back(ProxyCacheFileHot::new());
        hots.push_back(ProxyCacheFileHot::new());
        let name = ArcString::new(cache_conf.name.clone());
        Ok(ProxyCache {
            ctx: ArcRwLock::new(ProxyCacheContext {
                curr_size: 0,
                cache_file_node_expires: VecDeque::with_capacity(1000),
                hots,
                hot_instant: Instant::now(),
            }),
            cache_file_node_map: ArcRwLock::new(HashMap::new()),
            cache_conf,
            levels: level_vec,
            name,
        })
    }

    pub fn sort_hot(&self, _r: &Arc<HttpStreamRequest>, proxy_hot_file: &ProxyHotFile) {
        let ctx = &mut *self.ctx.get_mut();
        let curr = Instant::now();
        let elapsed = {
            let hot_instant = &ctx.hot_instant;
            let elapsed = curr - *hot_instant;
            elapsed.as_secs()
        };
        if elapsed < proxy_hot_file.hot_interval_time {
            return;
        }
        ctx.hot_instant = curr;
        for hot in &mut ctx.hots {
            hot.is_ok.store(false, Ordering::Relaxed);
        }
        ctx.hots.push_back(ProxyCacheFileHot::new());

        let hot = ctx.hots.pop_front().unwrap();
        let hot_top_count = proxy_hot_file.hot_top_count as usize;
        let hot_top_count = if hot.values.len() < hot_top_count {
            hot.values.len()
        } else {
            hot_top_count
        };
        for index in 0..hot_top_count {
            let v = &hot.values[index];
            v.is_hot.store(false, Ordering::Relaxed);
        }

        let hot = ctx.hots.front_mut().unwrap();
        hot.values.make_contiguous().sort_by(|a, b| {
            a.hot_count
                .load(Ordering::Relaxed)
                .cmp(&b.hot_count.load(Ordering::Relaxed))
        });
        let hot_top_count = proxy_hot_file.hot_top_count as usize;
        let hot_top_count = if hot.values.len() < hot_top_count {
            hot.values.len()
        } else {
            hot_top_count
        };
        for index in 0..hot_top_count {
            let v = &hot.values[index];
            v.is_hot.store(true, Ordering::Relaxed);
        }
    }

    pub async fn load(&self, net_core_proxy: &crate::config::net_core_proxy::Conf) -> Result<()> {
        let cache_file_node_queue = net_core_proxy.cache_file_node_queue.clone();
        let mut file_full_names = Vec::with_capacity(1000);
        let file_full_name = if &self.cache_conf.path[self.cache_conf.path.len() - 1..] == "/" {
            &self.cache_conf.path[0..self.cache_conf.path.len() - 1]
        } else {
            &self.cache_conf.path
        };
        log::debug!(target: "ext", "file_full_name: {:?}", file_full_name);
        Self::load_file_full_path(
            file_full_name,
            &mut file_full_names,
            &vec!["tmp", "temp"],
            &vec!["_tmp", "_temp"],
        )?;
        log::debug!(target: "ext", "file_full_names: {:?}", file_full_names);

        for file_full_name in file_full_names {
            let proxy_cache_path = ArcString::new(file_full_name);
            let (cache_file_node_head, ..) =
                Self::load_cache_file_head(proxy_cache_path.clone(), 0).await?;
            let cache_file_node_head = Arc::new(cache_file_node_head);
            let md5 = cache_file_node_head.md5.clone();
            let raw_content_length = cache_file_node_head.raw_content_length.clone();
            let trie_url = cache_file_node_head.trie_url.clone();

            let (_, cache_file_node_manage) = HttpCacheFile::read_cache_file_node_manage(
                self,
                &md5,
                cache_file_node_queue.clone(),
            );
            let version_expires = cache_file_node_manage
                .get(file!(), line!())
                .await
                .version_expires;
            let cache_file_node_version = cache_file_node_manage
                .get(file!(), line!())
                .await
                .cache_file_node_version;

            cache_file_node_manage
                .get_mut(file!(), line!())
                .await
                .cache_file_node_head = Some(cache_file_node_head.clone()).into();

            let proxy_cache_ctx = &mut *self.ctx.get_mut();
            proxy_cache_ctx.curr_size += raw_content_length as i64;
            proxy_cache_ctx
                .cache_file_node_expires
                .push_back(ProxyCacheFileNodeExpires::new(
                    md5.clone(),
                    version_expires,
                    cache_file_node_version,
                    cache_file_node_head,
                ));

            log::trace!(target: "ext",
                        "trie_url:{}, md5:{}",
                        trie_url, String::from_utf8_lossy(md5.as_ref()));
            net_core_proxy.trie_insert(trie_url, md5.clone())?;
            net_core_proxy.index_insert(md5, self.cache_conf.name.clone());
        }

        Ok(())
    }

    pub async fn load_cache_file_head(
        file_full_name: ArcString,
        _directio: u64,
    ) -> Result<(ProxyCacheFileNodeHead, FileExt, Bytes, Bytes, usize)> {
        let file_ext = ProxyCacheFileNode::open_file(file_full_name.clone(), 0).await?;
        let file_len = file_ext.file_len;

        let file_r = file_ext.file.clone();
        let ret: Result<BytesMut> = tokio::task::spawn_blocking(move || {
            use std::io::Read;
            #[cfg(feature = "anyio-file")]
            let start_time = Instant::now();
            let read_len = 1024 * 16;
            let read_len = if file_len < read_len {
                file_len
            } else {
                read_len
            };
            let mut buf = BytesMut::zeroed(read_len as usize);
            let file_r = &mut *file_r.get_mut();
            file_r.seek(std::io::SeekFrom::Start(0))?;
            file_r
                .read_exact(buf.as_mut())
                .map_err(|e| anyhow!("err:file.read => e:{}", e))?;

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
        let mut md5_str = Bytes::new();
        let mut client_method = Bytes::new();
        let mut expires_time = Bytes::new();
        let mut directio = Bytes::new();
        let mut trie_url = Bytes::new();
        let mut proxy_cache_path = Bytes::new();
        let mut proxy_cache_path_tmp = Bytes::new();
        let mut cache_control_time = Bytes::new();

        let pattern = "\r\n\r\n";
        let position = bytes_index(&buf, pattern.as_ref())
            .ok_or(anyhow!("read_head file_full_name:{}", file_full_name))?;
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

            if key == "client_uri" {
                client_uri = value.clone();
            } else if key == "raw_content_length" {
                raw_content_length = value.clone();
            } else if key == "md5" {
                md5 = value.clone();
            } else if key == "md5_str" {
                md5_str = value.clone();
            } else if key == "client_method" {
                client_method = value.clone();
            } else if key == "expires_time" {
                expires_time = value.clone();
            } else if v.as_ref() == &CACHE_FILE_KEY.as_bytes()[0..CACHE_FILE_KEY.len() - 2] {
                is_file_ok = true;
            } else if key == "directio" {
                directio = value.clone();
            } else if key == "trie_url" {
                trie_url = value.clone();
            } else if key == "proxy_cache_path" {
                proxy_cache_path = value.clone();
            } else if key == "proxy_cache_path_tmp" {
                proxy_cache_path_tmp = value.clone();
            } else if key == "cache_control_time" {
                cache_control_time = value.clone();
            }
        }

        if !is_file_ok {
            return Err(anyhow!("err:open file fail"));
        }

        if raw_content_length.is_empty() {
            return Err(anyhow!("raw_content_length.is_empty"));
        }
        let raw_content_length = String::from_utf8(raw_content_length.to_vec())?.parse::<u64>()?;

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

        if md5.is_empty() {
            return Err(anyhow!("md5.is_empty"));
        }

        if md5_str.is_empty() {
            return Err(anyhow!("md5.is_empty"));
        }

        if proxy_cache_path.is_empty() {
            return Err(anyhow!("proxy_cache_path.is_empty"));
        }
        let proxy_cache_path = ArcString::new(String::from_utf8(proxy_cache_path.to_vec())?);
        if proxy_cache_path_tmp.is_empty() {
            return Err(anyhow!("proxy_cache_path_tmp.is_empty"));
        }
        let proxy_cache_path_tmp =
            ArcString::new(String::from_utf8(proxy_cache_path_tmp.to_vec())?);
        if client_uri.is_empty() {
            return Err(anyhow!("client_uri.is_empty"));
        }
        let uri = hyper::Uri::try_from(client_uri.as_ref())?;
        if trie_url.is_empty() {
            return Err(anyhow!("trie_url.is_empty"));
        }
        let trie_url = String::from_utf8(trie_url.to_vec())?;

        if client_method.is_empty() {
            return Err(anyhow!("client_method.is_empty"));
        }
        let method = http::method::Method::from_bytes(client_method.as_ref())?;

        if directio.is_empty() {
            return Err(anyhow!("raw_content_length.is_empty"));
        }
        let directio = String::from_utf8(directio.to_vec())?.parse::<u64>()?;

        let cache_file_slice = CACHE_FILE_SLISE;

        #[cfg(unix)]
        if _directio > 0 && directio > 0 && file_ext.file_len >= directio {
            file_ext.directio_on()?;
        }

        let cache_file_node_head = ProxyCacheFileNodeHead::new(
            md5,
            md5_str,
            proxy_cache_path,
            proxy_cache_path_tmp,
            uri,
            method,
            raw_content_length,
            expires_time,
            trie_url,
            directio,
            cache_file_slice,
            cache_control_time,
        );
        Ok((
            cache_file_node_head,
            file_ext,
            http_head_start,
            buf,
            file_head_size,
        ))
    }

    pub fn load_file_full_path(
        file_full_path: &str,
        file_full_names: &mut Vec<String>,
        filter_dir_names: &Vec<&str>,
        filter_file_names: &Vec<&str>,
    ) -> Result<()> {
        let dir_path = Path::new(file_full_path);
        for entry in fs::read_dir(dir_path)? {
            let entry = entry?;
            let path = entry.path();
            // 判断是否是目录
            if path.is_dir() {
                let name = path.file_name().unwrap().to_str().unwrap();
                let mut is_continue = false;
                for filter_dir_name in filter_dir_names {
                    if name == *filter_dir_name {
                        is_continue = true;
                        break;
                    }
                }
                if is_continue {
                    continue;
                }
                let file_full_path = format!("{}/{}", file_full_path, name);
                log::debug!(target: "main", "{}", file_full_path);
                Self::load_file_full_path(
                    &file_full_path,
                    file_full_names,
                    filter_dir_names,
                    filter_file_names,
                )?;
            } else {
                let name = path.file_name().unwrap().to_str().unwrap();
                let mut is_continue = false;
                for filter_file_name in filter_file_names {
                    if name.len() > filter_file_name.len()
                        && &name[name.len() - filter_file_name.len()..] == *filter_file_name
                    {
                        is_continue = true;
                        break;
                    }
                }
                if is_continue {
                    let _ = unlink(name, None);
                    continue;
                }

                let file_full_name = format!("{}/{}", file_full_path, name);
                log::debug!(target: "main", "{}", file_full_name);
                file_full_names.push(file_full_name);
            }
        }
        Ok(())
    }
}

pub struct HttpCacheFileContext {
    pub cache_file_node_manage: typ2::ArcRwLockTokio<ProxyCacheFileNodeManage>,
    pub cache_file_node: Option<Arc<ProxyCacheFileNode>>,
    pub cache_file_node_data: Option<Arc<ProxyCacheFileNodeData>>,
    pub cache_file_node_version: u64,
    pub cache_file_status: Option<CacheFileStatus>,
}

impl HttpCacheFileContext {
    pub fn cache_file_node(&self) -> Option<Arc<ProxyCacheFileNode>> {
        if self.cache_file_node.is_none() {
            None
        } else {
            self.cache_file_node.clone()
        }
    }
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
        let cache_file_node = self.ctx_thread.get().cache_file_node().unwrap();
        let response_info = &cache_file_node.response_info;
        let mut response = Response::builder().body(Body::empty())?;
        *response.status_mut() = cache_file_node.fix.response.status().clone();
        *response.version_mut() = cache_file_node.fix.response.version().clone();
        *response.headers_mut() = cache_file_node.fix.response.headers().clone();
        if response_info.range.raw_content_length > 0 {
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
        }
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
        let cache_file_node = self.ctx_thread.get().cache_file_node().unwrap();
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
        let cache_file_node = self.ctx_thread.get().cache_file_node().unwrap();
        let response_info = &cache_file_node.response_info;
        {
            let bitmap = cache_file_node.ctx_thread.get().bitmap.clone();
            let is_full = bitmap.get().is_full();
            if !is_full {
                log::warn!("err:file_response_not_get !cache_file_node_ctx.bitmap.is_full():{}, proxy_cache_path:{}", bitmap.get().to_string(), self.cache_file_info.proxy_cache_path);
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
        ms: &Modules,
        scc: &Arc<StreamConfigContext>,
    ) -> Result<()> {
        use crate::config::net_core_proxy;
        //当前可能是main  server local， ProxyCache必须到main_conf中读取
        let net_core_proxy = net_core_proxy::main_conf(ms).await;

        let (_, cache_file_node_manage) = Self::read_cache_file_node_manage(
            proxy_cache,
            &cache_file_info.md5,
            net_core_proxy.cache_file_node_queue.clone(),
        );
        let is_new = {
            let cache_file_node_manage =
                &mut *cache_file_node_manage.get_mut(file!(), line!()).await;

            let net_curr_conf = scc.net_curr_conf();
            let net_core_proxy_conf = net_core_proxy::curr_conf(&net_curr_conf);

            if net_core_proxy_conf.proxy_hot_file.is_open
                && !cache_file_node_manage.hot_is_ok.load(Ordering::Relaxed)
            {
                let ctx = &mut *proxy_cache.ctx.get_mut();
                if !ctx.hots.is_empty() {
                    let hot_count = Arc::new(AtomicU64::new(0));
                    let hot_is_hot = cache_file_node_manage.hot_is_hot.clone();
                    let back = ctx.hots.back_mut().unwrap();
                    back.values.push_back(ProxyCacheFileHotValue::new(
                        cache_file_info.md5.clone(),
                        hot_count.clone(),
                        hot_is_hot.clone(),
                    ));

                    let hot_is_ok = back.is_ok.clone();

                    cache_file_node_manage.hot_count = hot_count;
                    cache_file_node_manage.hot_is_ok = hot_is_ok;
                }
            }

            cache_file_node_manage
                .hot_count
                .fetch_add(1, Ordering::Relaxed);

            cache_file_node_manage.update_version_expires();
            cache_file_node_manage.cache_file_node_head.is_none()
        };

        Self::do_get_cache_file_node(r, cache_file_info, &cache_file_node_manage, true).await?;

        let cache_file_node_manage = &mut *cache_file_node_manage.get_mut(file!(), line!()).await;
        if is_new && cache_file_node_manage.cache_file_node_head.is_some() {
            let cache_file_node_head = cache_file_node_manage.cache_file_node_head.as_ref();
            let version_expires = cache_file_node_manage.version_expires;
            let cache_file_node_version = cache_file_node_manage.cache_file_node_version;
            let md5 = cache_file_node_head.md5.clone();
            let trie_url = cache_file_node_head.trie_url.clone();
            let http_cache_file = r.ctx.get().http_cache_file.clone();
            let proxy_cache = http_cache_file.proxy_cache.as_ref().unwrap();
            let proxy_cache_ctx = &mut *proxy_cache.ctx.get_mut();
            proxy_cache_ctx.curr_size += cache_file_node_head.raw_content_length as i64;
            proxy_cache_ctx
                .cache_file_node_expires
                .push_back(ProxyCacheFileNodeExpires::new(
                    md5.clone(),
                    version_expires,
                    cache_file_node_version,
                    cache_file_node_head.clone(),
                ));
            net_core_proxy.trie_insert(trie_url, md5.clone())?;
            net_core_proxy.index_insert(md5, proxy_cache.cache_conf.name.clone());
        }
        Ok(())
    }

    pub fn read_cache_file_node_manage(
        proxy_cache: &ProxyCache,
        md5: &Bytes,
        cache_file_node_queue: ArcMutex<VecDeque<Arc<ProxyCacheFileNodeData>>>,
    ) -> (bool, typ2::ArcRwLockTokio<ProxyCacheFileNodeManage>) {
        let cache_file_node_manage = proxy_cache.cache_file_node_map.get().get(md5).cloned();
        if cache_file_node_manage.is_some() {
            return (false, cache_file_node_manage.unwrap());
        }

        let cache_file_node_map = &mut *proxy_cache.cache_file_node_map.get_mut();
        let cache_file_node_manage = cache_file_node_map.get_mut(md5).cloned();
        if cache_file_node_manage.is_none() {
            let cache_file_node_manage = typ2::ArcRwLockTokio::new(ProxyCacheFileNodeManage::new(
                cache_file_node_queue,
                proxy_cache.name.clone(),
            ));
            cache_file_node_map.insert(md5.clone(), cache_file_node_manage.clone());
            (true, cache_file_node_manage)
        } else {
            (false, cache_file_node_manage.unwrap())
        }
    }

    pub async fn is_hot(
        _r: &Arc<HttpStreamRequest>,
        proxy_cache: &ProxyCache,
        md5: &Bytes,
    ) -> bool {
        let cache_file_node_manage = proxy_cache.cache_file_node_map.get().get(md5).cloned();
        if cache_file_node_manage.is_none() {
            return false;
        }

        let cache_file_node_manage = cache_file_node_manage.unwrap();
        let cache_file_node_manage = &*cache_file_node_manage.get(file!(), line!()).await;
        cache_file_node_manage.hot_is_hot.load(Ordering::Relaxed)
    }

    pub async fn set_cache_file_node(
        &self,
        r: &Arc<HttpStreamRequest>,
        cache_file_node: Arc<ProxyCacheFileNode>,
        cache_file_node_manage: &mut ProxyCacheFileNodeManage,
    ) -> Result<bool> {
        if cache_file_node_manage.cache_file_node_head.is_some()
            && self.ctx_thread.get().cache_file_node_version
                != cache_file_node_manage.cache_file_node_version
        {
            return Ok(false);
        }

        let cache_file_node_old = cache_file_node_manage.cache_file_node.clone();
        cache_file_node_manage.cache_file_node_version += 1;
        self.ctx_thread.get_mut().cache_file_node_version += 1;
        cache_file_node.ctx_thread.get_mut().cache_file_node_version += 1;
        cache_file_node_manage.cache_file_node_head =
            Some(cache_file_node.cache_file_node_head.clone()).into();

        let file_ext = Arc::new(FileExt {
            async_lock: cache_file_node.file_ext.async_lock.clone(),
            file: ArcMutex::default(),
            fix: cache_file_node.file_ext.fix.clone(),
            file_path: cache_file_node.file_ext.file_path.clone(),
            file_len: cache_file_node.file_ext.file_len,
        });

        let buf_file = MsgReadBufFile {
            file_ext: file_ext.clone(),
            seek: cache_file_node.buf_file.seek,
            size: cache_file_node.buf_file.size,
        };

        let node = Arc::new(ProxyCacheFileNode {
            ctx_thread: cache_file_node.ctx_thread.clone(),
            fix: cache_file_node.fix.clone(),
            file_ext,
            buf_file,
            cache_file_info: cache_file_node.cache_file_info.clone(),
            response_info: cache_file_node.response_info.clone(),
            cache_file_node_head: cache_file_node.cache_file_node_head.clone(),
            proxy_cache_name: cache_file_node_manage.proxy_cache_name.clone(),
        });

        cache_file_node_manage.cache_file_node = Some(node);
        cache_file_node_manage.cache_file_node_queue_clear();
        if r.ctx.get().r_in.is_range {
            cache_file_node_manage.is_upstream = false;
            if r.ctx.get().is_upstream_add {
                r.ctx.get_mut().is_upstream_add = false;
                cache_file_node_manage
                    .upstream_count
                    .fetch_sub(1, Ordering::Relaxed);
            }

            let mut upstream_waits = VecDeque::with_capacity(10);
            swap(
                &mut upstream_waits,
                &mut cache_file_node_manage.upstream_waits,
            );
            for tx in upstream_waits {
                let _ = tx.send(());
            }
        }

        let (proxy_cache_path_tmp, proxy_cache_path) = {
            let cache_file_info = &cache_file_node.cache_file_info;
            (
                cache_file_info.proxy_cache_path_tmp.clone(),
                cache_file_info.proxy_cache_path.clone(),
            )
        };

        let proxy_cache_path_ = proxy_cache_path.clone();

        let scc = r.http_arg.stream_info.get().scc.clone();
        use crate::config::common_core;
        let common_core_conf = common_core::main_conf(&scc.ms).await;
        let tmpfile_id = common_core_conf.tmpfile_id.fetch_add(1, Ordering::Relaxed);
        let session_id = r.session_id;
        let ret: Result<()> = tokio::task::spawn_blocking(move || {
            #[cfg(feature = "anyio-file")]
            let start_time = Instant::now();

            if Path::new(proxy_cache_path.as_str()).exists() {
                let mut tmp = proxy_cache_path_tmp.to_string();
                tmp.push_str(&format!("_{}_tmp", tmpfile_id));

                log::info!(target: "ext", "r.session_id:{}, exists rename {},{}",
                           session_id, proxy_cache_path.as_str(), tmp.as_str());
                std::fs::rename(proxy_cache_path.as_str(), tmp.as_str())?;
                let _ = unlink(tmp.as_str(), None);
                if cache_file_node_old.is_some() {
                    let cache_file_node_old = cache_file_node_old.unwrap();
                    cache_file_node_old.file_ext.file_path.set(tmp.into());
                    cache_file_node_old.file_ext.unlink(None);
                }
            }
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

        cache_file_node.file_ext.file_path.set(proxy_cache_path_);
        return Ok(true);
    }

    pub async fn cache_file_node_to_pool(
        &self,
        r: &Arc<HttpStreamRequest>,
        cache_file_node: Arc<ProxyCacheFileNode>,
        cache_file_node_data: Arc<ProxyCacheFileNodeData>,
    ) -> Result<()> {
        let scc = r.http_arg.stream_info.get().scc.clone();
        use crate::config::net_core_proxy;
        let net_core_proxy_main_conf = net_core_proxy::main_conf(scc.ms()).await;

        let (_, cache_file_node_manage) = Self::read_cache_file_node_manage(
            self.proxy_cache.as_ref().unwrap(),
            &self.cache_file_info.md5,
            net_core_proxy_main_conf.cache_file_node_queue.clone(),
        );
        let cache_file_node_manage = &mut *cache_file_node_manage.get_mut(file!(), line!()).await;
        if cache_file_node_manage.cache_file_node.is_none() {
            return Ok(());
        }

        if self.ctx_thread.get().cache_file_node_version
            != cache_file_node_manage.cache_file_node_version
        {
            return Ok(());
        }

        cache_file_node_data.count.fetch_add(1, Ordering::Relaxed);
        if cache_file_node_data.node.get().is_none() {
            cache_file_node_data.node.set(cache_file_node);
            cache_file_node_manage
                .cache_file_node_queue_lru
                .get_mut()
                .push_back(cache_file_node_data.clone());
        }
        cache_file_node_manage
            .cache_file_node_queue
            .push_back(cache_file_node_data.clone());

        return Ok(());
    }

    pub async fn get_cache_file_node(
        &self,
        r: &Arc<HttpStreamRequest>,
    ) -> Result<(
        bool,
        Option<CacheFileStatus>,
        typ2::ArcRwLockTokio<ProxyCacheFileNodeManage>,
        Option<Arc<ProxyCacheFileNodeData>>,
        Option<Arc<ProxyCacheFileNode>>,
        i64,
        Option<Arc<AtomicI64>>,
    )> {
        let scc = r.http_arg.stream_info.get().scc.clone();
        use crate::config::net_core_proxy;
        let net_core_proxy_main_conf = net_core_proxy::main_conf(scc.ms()).await;

        let mut upstream_version = -1;
        let mut is_open_file = true;
        let once_time = 1000 * 10;
        let mut is_ok = false;
        let mut open_file_err_num = 0;
        let open_file_err_num_max = 10;
        let mut cache_file_err_num = 0;
        let cache_file_err_num_max = 10;
        let upstream_count_warn_max = 10;
        loop {
            let (_, cache_file_node_manage) = Self::read_cache_file_node_manage(
                self.proxy_cache.as_ref().unwrap(),
                &self.cache_file_info.md5,
                net_core_proxy_main_conf.cache_file_node_queue.clone(),
            );

            let (_, cache_file_node) = Self::do_get_cache_file_node(
                r,
                &self.cache_file_info,
                &cache_file_node_manage,
                is_open_file,
            )
            .await?;
            is_open_file = false;

            if cache_file_node.is_some() {
                let cache_file_node = cache_file_node.as_ref().unwrap();
                let bitmap = cache_file_node.ctx_thread.get().bitmap.clone();
                let is_full = bitmap.get().is_full();
                let cache_file_status = cache_file_node.ctx_thread.get().cache_file_status.clone();
                let _is_ok = match &cache_file_status {
                    &CacheFileStatus::Exist => {
                        let rctx = &*r.ctx.get();
                        if rctx.r_in.is_range {
                            true
                        } else {
                            if is_full {
                                true
                            } else {
                                is_ok
                            }
                        }
                    }
                    &CacheFileStatus::Expire => {
                        if is_full {
                            true
                        } else {
                            is_ok
                        }
                    }
                };
                if _is_ok {
                    let pool_cache_file_node_data = {
                        let cache_file_node_manage =
                            &mut *cache_file_node_manage.get_mut(file!(), line!()).await;
                        cache_file_node_manage.cache_file_node_queue.pop_front()
                    };
                    let (pool_cache_file_node, pool_cache_file_node_data) = {
                        if pool_cache_file_node_data.is_none() {
                            (None, None)
                        } else {
                            let pool_cache_file_node_data = pool_cache_file_node_data.unwrap();
                            let node = pool_cache_file_node_data.node.get();
                            if node.is_none() {
                                (None, None)
                            } else {
                                (Some(node.clone()), Some(pool_cache_file_node_data.clone()))
                            }
                        }
                    };

                    let (cache_file_node, cache_file_node_data) = if pool_cache_file_node.is_none()
                    {
                        let file_ext = {
                            let ret = ProxyCache::load_cache_file_head(
                                self.cache_file_info.proxy_cache_path.clone(),
                                self.cache_file_info.directio,
                            )
                            .await;
                            //如果文件不存在清空， 文件存在断开链接，  文件变化清空
                            if let Err(e) = ret {
                                if Path::new(self.cache_file_info.proxy_cache_path.as_str())
                                    .exists()
                                {
                                    open_file_err_num += 1;
                                    if open_file_err_num >= 2 {
                                        tokio::time::sleep(tokio::time::Duration::from_millis(500))
                                            .await;
                                    }
                                    if open_file_err_num > open_file_err_num_max {
                                        return Err(anyhow!(
                                        "err:open file => open_file_err_num:{}, path:{}, err:{}",
                                        open_file_err_num,
                                        self.cache_file_info.proxy_cache_path,
                                        e
                                    ));
                                    }
                                    log::warn!(
                                        "err:open file => open_file_err_num:{}, path:{}, err:{}",
                                        open_file_err_num,
                                        self.cache_file_info.proxy_cache_path,
                                        e
                                    );
                                    continue;
                                }
                                let (_, cache_file_node) = Self::do_get_cache_file_node(
                                    r,
                                    &self.cache_file_info,
                                    &cache_file_node_manage,
                                    false,
                                )
                                .await?;
                                if cache_file_node.is_some() {
                                    let cache_file_node_manage = &mut *cache_file_node_manage
                                        .get_mut(file!(), line!())
                                        .await;
                                    cache_file_node_manage.cache_file_node_queue_clear();
                                    cache_file_node_manage.cache_file_node = None;
                                }
                                continue;
                            }
                            let (_, file_ext, ..) = ret?;
                            file_ext
                        };

                        let file_ext_ = cache_file_node.get_file_ext();

                        //可能外部改变了， 需要替换
                        if !file_ext_.is_uniq_eq(&file_ext.fix) {
                            let (_, cache_file_node) = Self::do_get_cache_file_node(
                                r,
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
                                cache_file_err_num += 1;
                                if cache_file_err_num >= 2 {
                                    tokio::time::sleep(tokio::time::Duration::from_millis(500))
                                        .await;
                                }
                                log::warn!(
                                    "err:file uniq => cache_file_err_num:{}, path:{}, {}, {}",
                                    cache_file_err_num,
                                    self.cache_file_info.proxy_cache_path,
                                    file_ext_.fix.file_uniq.uniq,
                                    file_ext.fix.file_uniq.uniq,
                                );
                                if cache_file_err_num >= cache_file_err_num_max {
                                    cache_file_err_num = 0;
                                    log::warn!(
                                        "err:clear file uniq => cache_file_err_num:{}, path:{}, {}, {}",
                                        cache_file_err_num,
                                        self.cache_file_info.proxy_cache_path,
                                        file_ext_.fix.file_uniq.uniq,
                                        file_ext.fix.file_uniq.uniq,
                                    );

                                    let cache_file_node_manage = &mut *cache_file_node_manage
                                        .get_mut(file!(), line!())
                                        .await;
                                    cache_file_node_manage.cache_file_node_queue_clear();
                                    cache_file_node_manage.cache_file_node = None;
                                }
                            }
                            continue;
                        }

                        let cache_file_node = ProxyCacheFileNode::copy(
                            &cache_file_node,
                            file_ext,
                            self.cache_file_info.clone(),
                        )?;
                        let cache_file_node = Arc::new(cache_file_node);

                        let cache_file_node_data = Arc::new(ProxyCacheFileNodeData::new(None));
                        (cache_file_node, cache_file_node_data)
                    } else {
                        (
                            pool_cache_file_node.unwrap(),
                            pool_cache_file_node_data.unwrap(),
                        )
                    };

                    let cache_file_status =
                        cache_file_node.ctx_thread.get().cache_file_status.clone();
                    let (is_upstream, upstream_count, upstream_count_drop) =
                        match &cache_file_status {
                            &CacheFileStatus::Exist => {
                                let bitmap = cache_file_node.ctx_thread.get().bitmap.clone();
                                let is_full = bitmap.get().is_full();
                                if r.ctx.get().r_in.is_range {
                                    (false, 0, None)
                                } else {
                                    if is_full {
                                        (false, 0, None)
                                    } else {
                                        let cache_file_node_manage = &mut *cache_file_node_manage
                                            .get_mut(file!(), line!())
                                            .await;
                                        if cache_file_node_manage.is_upstream {
                                            is_ok = false;
                                            continue;
                                        }
                                        cache_file_node_manage.is_upstream = true;
                                        cache_file_node_manage.upstream_version += 1;
                                        let upstream_count = cache_file_node_manage
                                            .upstream_count
                                            .fetch_add(1, Ordering::Relaxed)
                                            + 1;
                                        cache_file_node_manage.upstream_time = Instant::now();
                                        log::trace!(target: "is_ups", "session_id:{}, upstream cache_file_node Exist", r.session_id);
                                        (
                                            true,
                                            upstream_count,
                                            Some(cache_file_node_manage.upstream_count.clone()),
                                        )
                                    }
                                }
                            }
                            &CacheFileStatus::Expire => {
                                let cache_file_node_manage =
                                    &mut *cache_file_node_manage.get_mut(file!(), line!()).await;
                                //如何让外部指定要回源
                                let bitmap = cache_file_node.ctx_thread.get().bitmap.clone();
                                let is_full = bitmap.get().is_full();

                                if cache_file_node_manage.is_upstream == true
                                    && cache_file_node_manage.upstream_time.elapsed().as_secs()
                                        < once_time
                                    && is_full
                                {
                                    log::trace!(target: "is_ups", "session_id:{}, local cache_file_node Expire", r.session_id);
                                    (false, 0, None)
                                } else {
                                    if cache_file_node_manage.is_upstream {
                                        is_ok = false;
                                        continue;
                                    }
                                    cache_file_node_manage.is_upstream = true;
                                    cache_file_node_manage.upstream_version += 1;
                                    let upstream_count = cache_file_node_manage
                                        .upstream_count
                                        .fetch_add(1, Ordering::Relaxed)
                                        + 1;
                                    cache_file_node_manage.upstream_time = Instant::now();
                                    log::trace!(target: "is_ups", "session_id:{}, upstream cache_file_node Expire", r.session_id);
                                    (
                                        true,
                                        upstream_count,
                                        Some(cache_file_node_manage.upstream_count.clone()),
                                    )
                                }
                            }
                        };

                    if upstream_count >= upstream_count_warn_max {
                        log::warn!(
                            "err:upstream_count => upstream_count:{}, path:{}",
                            upstream_count,
                            self.cache_file_info.proxy_cache_path,
                        );
                    }

                    return Ok((
                        is_upstream,
                        Some(cache_file_status),
                        cache_file_node_manage,
                        Some(cache_file_node_data),
                        Some(cache_file_node),
                        upstream_count,
                        upstream_count_drop,
                    ));
                }
            } else {
                if is_ok {
                    let cache_file_node_manage_ =
                        &mut *cache_file_node_manage.get_mut(file!(), line!()).await;
                    if cache_file_node_manage_.is_upstream {
                        is_ok = false;
                        continue;
                    }
                    cache_file_node_manage_.is_upstream = true;
                    cache_file_node_manage_.upstream_version += 1;
                    let upstream_count = cache_file_node_manage_
                        .upstream_count
                        .fetch_add(1, Ordering::Relaxed)
                        + 1;

                    if upstream_count >= upstream_count_warn_max {
                        log::warn!(
                            "err:upstream_count => upstream_count:{}, path:{}",
                            upstream_count,
                            self.cache_file_info.proxy_cache_path,
                        );
                    }
                    log::trace!(target: "is_ups", "session_id:{}, upstream cache_file_node nil", r.session_id);
                    return Ok((
                        true,
                        None,
                        cache_file_node_manage.clone(),
                        None,
                        None,
                        upstream_count,
                        Some(cache_file_node_manage_.upstream_count.clone()),
                    ));
                }
            }

            let rx = {
                let cache_file_node_manage =
                    &mut *cache_file_node_manage.get_mut(file!(), line!()).await;
                if !cache_file_node_manage.is_upstream
                    || (upstream_version >= 0
                        && upstream_version == cache_file_node_manage.upstream_version)
                {
                    cache_file_node_manage.is_upstream = false;
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
        _r: &Arc<HttpStreamRequest>,
        cache_file_info: &Arc<ProxyCacheFileInfo>,
        cache_file_node_manage: &typ2::ArcRwLockTokio<ProxyCacheFileNodeManage>,
        is_open_file: bool,
    ) -> Result<(bool, Option<Arc<ProxyCacheFileNode>>)> {
        let cache_file_node = {
            cache_file_node_manage
                .get(file!(), line!())
                .await
                .cache_file_node
                .clone()
        };
        if cache_file_node.is_some() {
            let cache_file_node = cache_file_node.unwrap();
            cache_file_node.update_file_node_status();
            return Ok((false, Some(cache_file_node)));
        }

        let cache_file_node_manage = &mut *cache_file_node_manage.get_mut(file!(), line!()).await;
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
            cache_file_node_manage.proxy_cache_name.clone(),
        )
        .await;
        if let Err(e) = &cache_file_node {
            log::warn!("err:open file => e:{}", e);
            return Ok((false, None));
        }
        let cache_file_node = cache_file_node.unwrap();
        if cache_file_node.is_none() {
            return Ok((false, None));
        }
        let mut cache_file_node = cache_file_node.unwrap();

        if cache_file_node_manage.cache_file_node_head.is_none() {
            cache_file_node_manage.cache_file_node_head =
                Some(cache_file_node.cache_file_node_head.clone()).into();
        } else {
            cache_file_node.cache_file_node_head =
                cache_file_node_manage.cache_file_node_head.clone().unwrap();
        }

        let cache_file_node = Arc::new(cache_file_node);

        let file_ext = Arc::new(FileExt {
            async_lock: cache_file_node.file_ext.async_lock.clone(),
            file: ArcMutex::default(),
            fix: cache_file_node.file_ext.fix.clone(),
            file_path: cache_file_node.file_ext.file_path.clone(),
            file_len: cache_file_node.file_ext.file_len,
        });

        let buf_file = MsgReadBufFile {
            file_ext: file_ext.clone(),
            seek: cache_file_node.buf_file.seek,
            size: cache_file_node.buf_file.size,
        };

        let node = Arc::new(ProxyCacheFileNode {
            ctx_thread: cache_file_node.ctx_thread.clone(),
            fix: cache_file_node.fix.clone(),
            file_ext,
            buf_file,
            cache_file_info: cache_file_node.cache_file_info.clone(),
            response_info: cache_file_node.response_info.clone(),
            cache_file_node_head: cache_file_node.cache_file_node_head.clone(),
            proxy_cache_name: cache_file_node_manage.proxy_cache_name.clone(),
        });
        cache_file_node_manage.cache_file_node = Some(node.clone());

        let data = Arc::new(ProxyCacheFileNodeData::new(Some(cache_file_node)));

        cache_file_node_manage
            .cache_file_node_queue_lru
            .get_mut()
            .push_back(data.clone());

        cache_file_node_manage.cache_file_node_queue.push_back(data);

        Ok((true, Some(node)))
    }
}
