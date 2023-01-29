use super::peer_client::PeerClient;
use super::peer_stream_connect::PeerStreamConnect;
use super::stream::Stream;
use crate::peer_stream::PeerStreamKey;
use anyhow::anyhow;
use anyhow::Result;
use chrono::Local;
use lazy_static::lazy_static;
use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;
use std::sync::Mutex;

lazy_static! {
    static ref CLIENT_ID: AtomicU32 = AtomicU32::new(1);
}

#[derive(Clone)]
pub struct AsyncClient {
    peer_stream_key_map: Arc<Mutex<HashMap<String, VecDeque<(Arc<PeerStreamKey>, i64)>>>>,
}

impl AsyncClient {
    pub fn new() -> AsyncClient {
        AsyncClient {
            peer_stream_key_map: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn add_peer_stream_key(
        &self,
        key: String,
        peer_stream_key: Arc<PeerStreamKey>,
        min_stream_cache_size: usize,
    ) {
        let curr_time = Local::now().timestamp();
        let mut peer_stream_key_map = self.peer_stream_key_map.lock().unwrap();
        let values = peer_stream_key_map.get_mut(&key);
        if values.is_none() {
            let mut values = VecDeque::with_capacity(30);
            values.push_back((peer_stream_key, curr_time));
            peer_stream_key_map.insert(key, values);
            return;
        }
        let values = values.unwrap();
        values.push_back((peer_stream_key, curr_time));

        if values.len() > min_stream_cache_size {
            for _ in 0..5 {
                if values.front().is_some() {
                    let (value, time) = values.front().unwrap();
                    if curr_time - time > 10 || value.to_peer_stream_tx.is_closed() {
                        let (key, _) = values.pop_front().unwrap();
                        key.to_peer_stream_tx.close();
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
        }
    }

    pub fn get_peer_stream_key(
        &self,
        key: &str,
        min_stream_cache_size: usize,
    ) -> Option<Arc<PeerStreamKey>> {
        let curr_time = Local::now().timestamp();
        let mut peer_stream_key_map = self.peer_stream_key_map.lock().unwrap();
        let values = peer_stream_key_map.get_mut(key);
        if values.is_none() {
            return None;
        }
        let values = values.unwrap();
        loop {
            let value = values.pop_front();
            if value.is_none() {
                return None;
            }
            let (peer_stream_key, _) = value.unwrap();
            if peer_stream_key.to_peer_stream_tx.is_closed() {
                peer_stream_key.to_peer_stream_tx.close();
                continue;
            }

            if values.len() > min_stream_cache_size {
                for _ in 0..5 {
                    if values.front().is_some() {
                        let (value, time) = values.front().unwrap();
                        if curr_time - time > 10 || value.to_peer_stream_tx.is_closed() {
                            let (key, _) = values.pop_front().unwrap();
                            key.to_peer_stream_tx.close();
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }
            return Some(peer_stream_key);
        }
    }
}

#[derive(Clone)]
pub struct Client {
    async_client: Arc<AsyncClient>,
    pid: i32,
}

impl Client {
    pub fn new() -> Client {
        let pid = unsafe { libc::getpid() };
        Client {
            async_client: Arc::new(AsyncClient::new()),
            pid,
        }
    }

    pub async fn connect(
        &self,
        peer_stream_connect: Arc<Box<dyn PeerStreamConnect>>,
        peer_stream_size: Option<Arc<AtomicUsize>>,
    ) -> Result<(Stream, SocketAddr, SocketAddr)> {
        self.do_connect(peer_stream_connect, peer_stream_size)
            .await
            .map_err(|e| {
                log::error!("{}", e);
                e
            })
    }

    pub async fn do_connect(
        &self,
        peer_stream_connect: Arc<Box<dyn PeerStreamConnect>>,
        peer_stream_size: Option<Arc<AtomicUsize>>,
    ) -> Result<(Stream, SocketAddr, SocketAddr)> {
        let client_id = CLIENT_ID.fetch_add(1, Ordering::Relaxed);
        let max_stream_size = peer_stream_connect.max_stream_size();
        let min_stream_cache_size = peer_stream_connect.min_stream_cache_size();
        let channel_size = peer_stream_connect.channel_size();
        let (_, stream, local_addr, remote_addr) = PeerClient::create_stream_and_peer_client(
            true,
            max_stream_size,
            Some((self.pid, client_id)),
            Some(self.async_client.clone()),
            Some(peer_stream_connect),
            None,
            min_stream_cache_size,
            peer_stream_size,
            None,
            None,
            channel_size,
        )
        .await
        .map_err(|e| anyhow!("err:create_client => e:{}", e))?;

        Ok((stream, local_addr, remote_addr))
    }
}
