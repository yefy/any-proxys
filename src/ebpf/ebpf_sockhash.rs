#![cfg(feature = "anyproxy-ebpf")]

use anyhow::anyhow;
use anyhow::{bail, Result};
extern crate libc;
use crate::Executors;
use awaitgroup::WaitGroup;
use libc::c_uint;
use std::thread;
use tokio::sync::broadcast;
use tokio::sync::mpsc;

#[path = "bpf/.output/ebpf_sockhash.skel.rs"]
pub mod ebpf_sockhash;
use ebpf_sockhash::*;
use std::net::SocketAddr;

#[repr(C)]
pub struct sock_map_key {
    pub remote_ip4: c_uint,
    pub local_ip4: c_uint,
    pub remote_port: c_uint,
    pub local_port: c_uint,
    pub family: c_uint,
    pub pad0: c_uint,
}

#[derive(Clone, Debug)]
pub struct AddSockHashSocketData {
    client_fd: i32,
    client_peer_addr: SocketAddr,
    client_local_addr: SocketAddr,
    ups_fd: i32,
    ups_peer_addr: SocketAddr,
    ups_local_addr: SocketAddr,
}

#[derive(Clone, Debug)]
pub struct AddSockHashData {
    socket_data: AddSockHashSocketData,
    tx: mpsc::Sender<()>,
}

#[derive(Clone, Debug)]
pub struct AddSockHash {
    tx: async_channel::Sender<AddSockHashData>,
}

impl AddSockHash {
    pub async fn add(
        &self,
        client_fd: i32,
        client_peer_addr: SocketAddr,
        client_local_addr: SocketAddr,
        ups_fd: i32,
        ups_peer_addr: SocketAddr,
        ups_local_addr: SocketAddr,
    ) -> Result<mpsc::Receiver<()>> {
        let socket_data = AddSockHashSocketData {
            client_fd,
            client_peer_addr,
            client_local_addr,
            ups_fd,
            ups_peer_addr,
            ups_local_addr,
        };

        let (tx, rx) = mpsc::channel(10);
        self.tx.send(AddSockHashData { socket_data, tx }).await?;
        Ok(rx)
    }
}

unsafe fn any_as_u8_slice<T: Sized>(p: &T) -> &[u8] {
    ::std::slice::from_raw_parts((p as *const T) as *const u8, ::std::mem::size_of::<T>())
}

fn bump_memlock_rlimit() -> Result<()> {
    let rlimit = libc::rlimit {
        rlim_cur: 128 << 20,
        rlim_max: 128 << 20,
    };

    if unsafe { libc::setrlimit(libc::RLIMIT_MEMLOCK, &rlimit) } != 0 {
        bail!("Failed to increase rlimit");
    }

    Ok(())
}

pub struct EbpfGroup {
    group_version: i32,
    shutdown_thread_tx: Option<broadcast::Sender<bool>>,
    thread_handles: Option<Vec<std::thread::JoinHandle<()>>>,
    thread_wait_group: Option<WaitGroup>,
}

impl EbpfGroup {
    pub fn new(group_version: i32) -> Result<EbpfGroup> {
        Ok(EbpfGroup {
            group_version,
            shutdown_thread_tx: None,
            thread_handles: None,
            thread_wait_group: None,
        })
    }

    pub fn group_version(&self) -> i32 {
        self.group_version
    }

    pub fn start(&mut self) -> Result<AddSockHash> {
        let group_version = self.group_version;

        let (data_tx, data_rx) = async_channel::bounded(200);
        let (shutdown_thread_tx, _) = broadcast::channel(100);
        let thread_wait_group = WaitGroup::new();
        log::info!("group_version:{}", group_version);
        // 创建线程
        let thread_handles = (0..1)
            .map(|index| {
                let shutdown_thread_tx = shutdown_thread_tx.clone();
                let data_rx = data_rx.clone();
                let worker_thread = thread_wait_group.worker().add();
                thread::spawn(move || {
                    let thread_id = thread::current().id();
                    scopeguard::defer! {
                        log::info!("stop thread group_version:{} index:{}, worker_threads_id:{:?}", group_version, index, thread_id);
                        worker_thread.done();
                    }
                    log::info!("start thread group_version:{} index:{}, worker_threads_id:{:?}", group_version, index, thread_id);

                    let executor = async_executors::TokioCtBuilder::new().build().unwrap();
                    let executors = Executors {
                        executor,
                        shutdown_thread_tx,
                        group_version,
                        thread_id,
                    };
                    executors.executor.clone().block_on(async {
                        let ret: Result<()> = async {
                            EbpfSockhash::create_sockhash(executors, data_rx).await.map_err(|e| anyhow!("err:create_sockhash => e:{}", e))?;
                            Ok(())
                        }.await;
                        ret.unwrap_or_else(|e| log::error!("err:AnyproxyWork => e:{}", e));
                    });
                })
            })
            .collect::<Vec<_>>();

        self.shutdown_thread_tx = Some(shutdown_thread_tx);
        self.thread_handles = Some(thread_handles);
        self.thread_wait_group = Some(thread_wait_group);

        Ok(AddSockHash { tx: data_tx })
    }
}

pub struct EbpfSockhash {}

impl EbpfSockhash {
    pub async fn create_sockhash(
        executors: Executors,
        data_rx: async_channel::Receiver<AddSockHashData>,
    ) -> Result<()> {
        bump_memlock_rlimit()?;
        let mut skel_builder = EbpfSockhashSkelBuilder::default();
        skel_builder.obj_builder.debug(true);

        let open_skel = skel_builder.open()?;
        let mut skel = open_skel.load()?;

        // let progs = skel.progs();
        // let bpf_sk_msg_verdict = progs.bpf_sk_msg_verdict();
        // println!(
        //     "bpf_sk_msg_verdict prog_type:{}",
        //     bpf_sk_msg_verdict.prog_type()
        // );
        // println!(
        //     "bpf_sk_msg_verdict attach_type:{}",
        //     bpf_sk_msg_verdict.attach_type()
        // );
        // println!("bpf_sk_msg_verdict name:{:?}", bpf_sk_msg_verdict.name());
        // println!(
        //     "bpf_sk_msg_verdict section:{:?}",
        //     bpf_sk_msg_verdict.section()
        // );

        // let sock_hash_fd = skel.maps_mut().sock_hash().fd();
        // let _bpf_sk_msg_verdict = skel
        //     .progs_mut()
        //     .bpf_sk_msg_verdict()
        //     .attach_sockmap(sock_hash_fd)?;
        //

        // let progs = skel.progs();
        // let bpf_sockops = progs.bpf_sockops();
        // println!("bpf_sockops prog_type:{}", bpf_sockops.prog_type());
        // println!("bpf_sockops attach_type:{}", bpf_sockops.attach_type());
        // println!("bpf_sockops name:{:?}", bpf_sockops.name());
        // println!("bpf_sockops section:{:?}", bpf_sockops.section());

        // let f = std::fs::OpenOptions::new()
        //     //.custom_flags(libc::O_DIRECTORY)
        //     //.create(true)
        //     .read(true)
        //     .write(false)
        //     .open("/sys/fs/cgroup/")
        //     .map_err(|e| anyhow::anyhow!("open e:{}", e))?;
        // let cgroup_fd = f.as_raw_fd();
        //
        // let _bpf_sockops = skel.progs_mut().bpf_sockops().attach_cgroup(cgroup_fd)?;

        let sock_map_fd = skel.maps().sock_map().fd();

        let progs = skel.progs();
        let bpf_sk_skb_stream_parser = progs.bpf_sk_skb__stream_parser();
        println!(
            "bpf_sk_skb_stream_parser prog_type:{}",
            bpf_sk_skb_stream_parser.prog_type()
        );
        println!(
            "bpf_sk_skb_stream_parser attach_type:{}",
            bpf_sk_skb_stream_parser.attach_type()
        );
        println!(
            "bpf_sk_skb_stream_parser name:{:?}",
            bpf_sk_skb_stream_parser.name()
        );
        println!(
            "bpf_sk_skb_stream_parser section:{:?}",
            bpf_sk_skb_stream_parser.section()
        );

        let _bpf_sk_skb_stream_parser = skel
            .progs_mut()
            .bpf_sk_skb__stream_parser()
            .attach_sockmap(sock_map_fd)?;

        let progs = skel.progs();
        let bpf_sk_skb_stream_verdict = progs.bpf_sk_skb__stream_verdict();
        println!(
            "bpf_sk_skb_stream_verdict prog_type:{}",
            bpf_sk_skb_stream_verdict.prog_type()
        );
        println!(
            "bpf_sk_skb_stream_verdict attach_type:{}",
            bpf_sk_skb_stream_verdict.attach_type()
        );
        println!(
            "bpf_sk_skb_stream_verdict name:{:?}",
            bpf_sk_skb_stream_verdict.name()
        );
        println!(
            "bpf_sk_skb_stream_verdict section:{:?}",
            bpf_sk_skb_stream_verdict.section()
        );

        let _bpf_sk_skb_stream_verdict = skel
            .progs_mut()
            .bpf_sk_skb__stream_verdict()
            .attach_sockmap(sock_map_fd)?;

        println!("ebpf-sockhash-rs running");

        let mut shutdown_thread_rx = executors.shutdown_thread_tx.subscribe();
        loop {
            let ret: Result<bool> = async {
                let msg: Result<Option<AddSockHashData>> = async {
                    tokio::select! {
                        biased;
                        _ = shutdown_thread_rx.recv() => {
                            Ok(None)
                        }
                        msg = data_rx.recv() => {
                            Ok(Some(msg?))
                        }
                        else => {
                            return Err(anyhow!("err:start_cmd"))?;
                        }
                    }
                }
                .await;
                let msg = msg?;
                if msg.is_none() {
                    return Ok(true);
                }
                let msg = msg.unwrap();
                EbpfSockhash::add_sockhash(&mut skel, &msg.socket_data)?;
                let _ = msg.tx.send(()).await;

                Ok(false)
            }
            .await;
            let is_exit = match ret {
                Ok(is_exit) => is_exit,
                Err(e) => {
                    log::error!("err:start_cmd => e:{}", e);
                    false
                }
            };
            if is_exit {
                break;
            }
        }
        Ok(())
    }

    pub fn add_sockhash(
        skel: &mut EbpfSockhashSkel<'_>,
        socket_data: &AddSockHashSocketData,
    ) -> anyhow::Result<()> {
        let mut maps_mut = skel.maps_mut();

        println!("socket_data:{:?}", socket_data);

        if socket_data.ups_fd > 0 {
            let ip = &socket_data.client_peer_addr.ip().to_string();
            let ips = ip.split(".").collect::<Vec<_>>();
            let peer_addr_ip = ips[3].parse::<u32>().unwrap() << 24
                | ips[2].parse::<u32>().unwrap() << 16
                | ips[1].parse::<u32>().unwrap() << 8
                | ips[0].parse::<u32>().unwrap();

            let ip = &socket_data.client_local_addr.ip().to_string();
            let ips = ip.split(".").collect::<Vec<_>>();
            let local_addr_ip = ips[3].parse::<u32>().unwrap() << 24
                | ips[2].parse::<u32>().unwrap() << 16
                | ips[1].parse::<u32>().unwrap() << 8
                | ips[0].parse::<u32>().unwrap();

            let key = sock_map_key {
                remote_ip4: peer_addr_ip,
                local_ip4: local_addr_ip,
                remote_port: socket_data.client_peer_addr.port() as u32,
                local_port: socket_data.client_local_addr.port() as u32,
                family: 2,
                pad0: 0,
            };
            println!(
                "client_stream remote_ip4:{}, local_ip4:{}, remote_port:{}, local_port:{}",
                key.remote_ip4, key.local_ip4, key.remote_port, key.local_port
            );
            let key_bytes: &[u8] = unsafe { any_as_u8_slice(&key) };

            maps_mut
                .sock_index_map()
                .update(
                    key_bytes,
                    &socket_data.ups_fd.to_le_bytes(),
                    libbpf_rs::MapFlags::ANY,
                )
                .map_err(|e| anyhow::anyhow!("update e:{}", e))?;

            maps_mut
                .sock_map()
                .update(
                    &socket_data.ups_fd.to_le_bytes(),
                    &socket_data.ups_fd.to_le_bytes(),
                    libbpf_rs::MapFlags::ANY,
                )
                .map_err(|e| anyhow::anyhow!("update e:{}", e))?;
        }

        if socket_data.client_fd > 0 {
            let ip = &socket_data.ups_peer_addr.ip().to_string();
            let ips = ip.split(".").collect::<Vec<_>>();
            let peer_addr_ip = ips[3].parse::<u32>().unwrap() << 24
                | ips[2].parse::<u32>().unwrap() << 16
                | ips[1].parse::<u32>().unwrap() << 8
                | ips[0].parse::<u32>().unwrap();

            let ip = &socket_data.ups_local_addr.ip().to_string();
            let ips = ip.split(".").collect::<Vec<_>>();
            let local_addr_ip = ips[3].parse::<u32>().unwrap() << 24
                | ips[2].parse::<u32>().unwrap() << 16
                | ips[1].parse::<u32>().unwrap() << 8
                | ips[0].parse::<u32>().unwrap();

            let key = sock_map_key {
                remote_ip4: peer_addr_ip,
                local_ip4: local_addr_ip,
                remote_port: socket_data.ups_peer_addr.port() as u32,
                local_port: socket_data.ups_local_addr.port() as u32,
                family: 2,
                pad0: 0,
            };
            println!(
                "upstream_stream remote_ip4:{}, local_ip4:{}, remote_port:{}, local_port:{}",
                key.remote_ip4, key.local_ip4, key.remote_port, key.local_port
            );
            let key_bytes: &[u8] = unsafe { any_as_u8_slice(&key) };

            maps_mut
                .sock_index_map()
                .update(
                    key_bytes,
                    &socket_data.client_fd.to_le_bytes(),
                    libbpf_rs::MapFlags::ANY,
                )
                .map_err(|e| anyhow::anyhow!("update e:{}", e))?;

            maps_mut
                .sock_map()
                .update(
                    &socket_data.client_fd.to_le_bytes(),
                    &socket_data.client_fd.to_le_bytes(),
                    libbpf_rs::MapFlags::ANY,
                )
                .map_err(|e| anyhow::anyhow!("update e:{}", e))?;
        }

        println!("ebpf  success");

        Ok(())
    }
}
