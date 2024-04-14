#![cfg(feature = "anyproxy-ebpf")]

extern crate libc;
use any_base::thread_spawn::AsyncThreadContext;
use any_base::thread_spawn::ThreadSpawn;
use anyhow::anyhow;
use anyhow::Result;
use libc::c_uint;
use libc::c_ushort;
use std::net::SocketAddr;

#[path = "bpf/.output/any_ebpf.skel.rs"]
pub mod any_ebpf;
use any_base::executor_local_spawn::_block_on;
use any_ebpf::*;

#[repr(C)]
pub struct sock_map_key {
    pub remote_ip4: c_uint,
    pub local_ip4: c_uint,
    pub remote_port: c_ushort,
    pub local_port: c_ushort,
    pub family: c_uint,
}

#[derive(Clone, Debug)]
pub struct SockMapData {
    client_fd: i32,
    client_peer_addr: SocketAddr,
    client_local_addr: SocketAddr,
    ups_fd: i32,
    ups_peer_addr: SocketAddr,
    ups_local_addr: SocketAddr,
}

#[derive(Clone, Debug)]
pub struct ReuseportData {
    dcid: u64,
    fd: i32,
}

#[derive(Clone, Debug)]
pub enum AnyEbofData {
    AddSockMapData(SockMapData),
    DelSockMapData(SockMapData),
    AddReuseportData(ReuseportData),
}

#[derive(Clone, Debug)]
pub struct AnyEbpfTxData {
    data: AnyEbofData,
    tx: async_channel::Sender<()>,
}

#[derive(Clone, Debug)]
pub struct AnyEbpfTx {
    tx: async_channel::Sender<AnyEbpfTxData>,
}

impl AnyEbpfTx {
    pub fn sock_map_data(
        client_fd: i32,
        client_peer_addr: SocketAddr,
        client_local_addr: SocketAddr,
        ups_fd: i32,
        ups_peer_addr: SocketAddr,
        ups_local_addr: SocketAddr,
    ) -> SockMapData {
        let sock_map_data = SockMapData {
            client_fd,
            client_peer_addr,
            client_local_addr,
            ups_fd,
            ups_peer_addr,
            ups_local_addr,
        };
        sock_map_data
    }
    pub async fn add_sock_map_data(
        &self,
        client_fd: i32,
        client_peer_addr: SocketAddr,
        client_local_addr: SocketAddr,
        ups_fd: i32,
        ups_peer_addr: SocketAddr,
        ups_local_addr: SocketAddr,
    ) -> Result<async_channel::Receiver<()>> {
        let sock_map_data = Self::sock_map_data(
            client_fd,
            client_peer_addr,
            client_local_addr,
            ups_fd,
            ups_peer_addr,
            ups_local_addr,
        );

        self.add_sock_map_data_ext(sock_map_data).await
    }

    pub async fn add_sock_map_data_ext(
        &self,
        sock_map_data: SockMapData,
    ) -> Result<async_channel::Receiver<()>> {
        let (tx, rx) = async_channel::bounded(10);
        self.tx
            .send(AnyEbpfTxData {
                data: AnyEbofData::AddSockMapData(sock_map_data),
                tx,
            })
            .await?;
        Ok(rx)
    }

    pub async fn del_sock_map_data(
        &self,
        client_fd: i32,
        client_peer_addr: SocketAddr,
        client_local_addr: SocketAddr,
        ups_fd: i32,
        ups_peer_addr: SocketAddr,
        ups_local_addr: SocketAddr,
    ) -> Result<async_channel::Receiver<()>> {
        let sock_map_data = Self::sock_map_data(
            client_fd,
            client_peer_addr,
            client_local_addr,
            ups_fd,
            ups_peer_addr,
            ups_local_addr,
        );

        self.del_sock_map_data_ext(sock_map_data).await
    }

    pub async fn del_sock_map_data_ext(
        &self,
        sock_map_data: SockMapData,
    ) -> Result<async_channel::Receiver<()>> {
        let (tx, rx) = async_channel::bounded(10);
        self.tx
            .send(AnyEbpfTxData {
                data: AnyEbofData::DelSockMapData(sock_map_data),
                tx,
            })
            .await?;
        Ok(rx)
    }

    pub async fn add_reuseport_data(
        &self,
        dcid: u64,
        fd: i32,
    ) -> Result<async_channel::Receiver<()>> {
        let reuseport_data = ReuseportData { dcid, fd };

        let (tx, rx) = async_channel::bounded(10);
        self.tx
            .send(AnyEbpfTxData {
                data: AnyEbofData::AddReuseportData(reuseport_data),
                tx,
            })
            .await?;
        Ok(rx)
    }
}

unsafe fn any_as_u8_slice<T: Sized>(p: &T) -> &[u8] {
    ::std::slice::from_raw_parts((p as *const T) as *const u8, ::std::mem::size_of::<T>())
}

pub struct EbpfGroup {
    thread_spawn: ThreadSpawn,
}

impl EbpfGroup {
    pub fn new(cpu_affinity: bool, group_version: i32) -> EbpfGroup {
        let thread_spawn = ThreadSpawn::new(cpu_affinity, group_version);
        EbpfGroup { thread_spawn }
    }

    pub async fn stop(&mut self) {
        self.thread_spawn.stop(true, 30).await
    }

    pub async fn start(&mut self, debug_is_open_ebpf_log: bool) -> Result<AnyEbpfTx> {
        let (data_tx, data_rx) = async_channel::bounded(200);

        let mut thread_spawn_wait_run = self.thread_spawn.thread_spawn_wait_run();
        thread_spawn_wait_run._start(move |async_context| {
            _block_on(1, 1, move |_| async move {
                AnyEbpf::create(async_context, data_rx, debug_is_open_ebpf_log)
                    .await
                    .map_err(|e| anyhow!("err:create_sockhash => e:{}", e))?;
                Ok(())
            })?;
            Ok(())
        });

        thread_spawn_wait_run.wait_run().await?;

        Ok(AnyEbpfTx { tx: data_tx })
    }
}

pub struct AnyEbpf<'a> {
    skel: AnyEbpfSkel<'a>,
}

impl AnyEbpf<'_> {
    pub fn create_ebpf(debug_is_open_ebpf_log: bool) -> Result<AnyEbpf<'static>> {
        let mut skel_builder = AnyEbpfSkelBuilder::default();
        skel_builder.obj_builder.debug(debug_is_open_ebpf_log);
        let open_skel = skel_builder.open()?;
        let mut skel = open_skel.load()?;

        //bpf_sk_msg_verdict
        AnyEbpf::msg_verdict(&mut skel)?;
        //bpf_sk_skb__stream_verdict
        AnyEbpf::stream_verdict(&mut skel)?;
        //bpf_sk_reuseport
        AnyEbpf::reuseport(&mut skel)?;

        Ok(AnyEbpf { skel })
    }

    pub async fn create(
        async_context: AsyncThreadContext,
        data_rx: async_channel::Receiver<AnyEbpfTxData>,
        debug_is_open_ebpf_log: bool,
    ) -> Result<()> {
        let mut any_ebpf = Self::create_ebpf(debug_is_open_ebpf_log)?;

        log::info!("ebpf running");

        async_context.complete();
        let mut shutdown_thread_rx = async_context.shutdown_thread_tx.subscribe();
        loop {
            let ret: Result<bool> = async {
                let msg: Result<Option<AnyEbpfTxData>> = async {
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
                match &msg.data {
                    AnyEbofData::AddSockMapData(socket_data) => {
                        if let Err(e) = AnyEbpf::do_add_sock_map(&mut any_ebpf.skel, socket_data) {
                            log::error!("err:add_sockhash => e:{}", e);
                        }
                    }
                    AnyEbofData::DelSockMapData(socket_data) => {
                        if let Err(e) = AnyEbpf::do_del_sock_map(&mut any_ebpf.skel, socket_data) {
                            log::error!("err:del_sockhash => e:{}", e);
                        }
                    }
                    AnyEbofData::AddReuseportData(reuseport_data) => {
                        if let Err(e) =
                            AnyEbpf::do_add_reuseport(&mut any_ebpf.skel, reuseport_data)
                        {
                            log::error!("err:add_reuseport => e:{}", e);
                        }
                    }
                }
                let _ = msg.tx.send(()).await;

                Ok(false)
            }
            .await;
            let is_exit = match ret {
                Ok(is_exit) => is_exit,
                Err(e) => {
                    log::error!("err:start_cmd => e:{}", e);
                    true
                }
            };
            if is_exit {
                break;
            }
        }
        Ok(())
    }

    pub fn msg_verdict(skel: &mut AnyEbpfSkel) -> Result<()> {
        if !cfg!(feature = "anyproxy-msg-verdict") {
            return Ok(());
        }
        log::info!("msg_verdict");
        //cgroup_fd
        let f = std::fs::OpenOptions::new()
            //.custom_flags(libc::O_DIRECTORY)
            //.create(true)
            .read(true)
            .write(false)
            .open("/sys/fs/cgroup/")
            .map_err(|e| anyhow::anyhow!("open e:{}", e))?;
        use std::os::unix::io::AsRawFd;
        let cgroup_fd = f.as_raw_fd();
        log::info!("cgroup_fd:{}", cgroup_fd);

        //bpf_sk_msg_verdict
        let progs = skel.progs();
        let bpf_sk_msg_verdict = progs.bpf_sk_msg_verdict();
        log::info!(
            "bpf_sk_msg_verdict prog_type:{}",
            bpf_sk_msg_verdict.prog_type()
        );
        log::info!(
            "bpf_sk_msg_verdict attach_type:{}",
            bpf_sk_msg_verdict.attach_type()
        );
        log::info!("bpf_sk_msg_verdict name:{:?}", bpf_sk_msg_verdict.name());
        log::info!(
            "bpf_sk_msg_verdict section:{:?}",
            bpf_sk_msg_verdict.section()
        );
        let sock_hash_fd = skel.maps_mut().sk_sockhash().fd();
        let _bpf_sk_msg_verdict = skel
            .progs_mut()
            .bpf_sk_msg_verdict()
            .attach_sockmap(sock_hash_fd)
            .map_err(|e| anyhow::anyhow!("err: bpf_sk_msg_verdict => e:{}", e))?;

        //bpf_sockops
        let progs = skel.progs();
        let bpf_sockops = progs.bpf_sockops();
        log::info!("bpf_sockops prog_type:{}", bpf_sockops.prog_type());
        log::info!("bpf_sockops attach_type:{}", bpf_sockops.attach_type());
        log::info!("bpf_sockops name:{:?}", bpf_sockops.name());
        log::info!("bpf_sockops section:{:?}", bpf_sockops.section());
        let _bpf_sockops = skel
            .progs_mut()
            .bpf_sockops()
            .attach_cgroup(cgroup_fd)
            .map_err(|e| anyhow::anyhow!("err: bpf_sockops => e:{}", e))?;

        Ok(())
    }

    pub fn stream_verdict(skel: &mut AnyEbpfSkel) -> Result<()> {
        log::info!("stream_verdict");
        //bpf_sk_skb__stream_parser
        let sk_sockmap_fd = skel.maps().sk_sockmap().fd();
        let progs = skel.progs();
        let bpf_sk_skb_stream_parser = progs.bpf_sk_skb__stream_parser();
        log::info!(
            "bpf_sk_skb_stream_parser prog_type:{}",
            bpf_sk_skb_stream_parser.prog_type()
        );
        log::info!(
            "bpf_sk_skb_stream_parser attach_type:{}",
            bpf_sk_skb_stream_parser.attach_type()
        );
        log::info!(
            "bpf_sk_skb_stream_parser name:{:?}",
            bpf_sk_skb_stream_parser.name()
        );
        log::info!(
            "bpf_sk_skb_stream_parser section:{:?}",
            bpf_sk_skb_stream_parser.section()
        );
        let _bpf_sk_skb_stream_parser = skel
            .progs_mut()
            .bpf_sk_skb__stream_parser()
            .attach_sockmap(sk_sockmap_fd)
            .map_err(|e| anyhow::anyhow!("err: bpf_sk_skb__stream_parser => e:{}", e))?;

        //bpf_sk_skb__stream_verdict
        let progs = skel.progs();
        let bpf_sk_skb_stream_verdict = progs.bpf_sk_skb__stream_verdict();
        log::info!(
            "bpf_sk_skb_stream_verdict prog_type:{}",
            bpf_sk_skb_stream_verdict.prog_type()
        );
        log::info!(
            "bpf_sk_skb_stream_verdict attach_type:{}",
            bpf_sk_skb_stream_verdict.attach_type()
        );
        log::info!(
            "bpf_sk_skb_stream_verdict name:{:?}",
            bpf_sk_skb_stream_verdict.name()
        );
        log::info!(
            "bpf_sk_skb_stream_verdict section:{:?}",
            bpf_sk_skb_stream_verdict.section()
        );
        let _bpf_sk_skb_stream_verdict = skel
            .progs_mut()
            .bpf_sk_skb__stream_verdict()
            .attach_sockmap(sk_sockmap_fd)
            .map_err(|e| anyhow::anyhow!("err: bpf_sk_skb__stream_verdict => e:{}", e))?;
        Ok(())
    }

    pub fn reuseport(skel: &mut AnyEbpfSkel) -> Result<()> {
        if !cfg!(feature = "anyproxy-reuseport") {
            return Ok(());
        }
        log::info!("reuseport");
        //bpf_sk_reuseport
        let progs = skel.progs();
        let bpf_sk_reuseport = progs.bpf_sk_reuseport();
        log::info!(
            "bpf_sk_reuseport prog_type:{}",
            bpf_sk_reuseport.prog_type()
        );
        log::info!(
            "bpf_sk_reuseport attach_type:{}",
            bpf_sk_reuseport.attach_type()
        );
        log::info!("bpf_sk_reuseport name:{:?}", bpf_sk_reuseport.name());
        log::info!("bpf_sk_reuseport section:{:?}", bpf_sk_reuseport.section());

        let quic_sockhash_fd = skel.maps().quic_sockhash().fd();
        let _bpf_sk_reuseport = skel
            .progs_mut()
            .bpf_sk_reuseport()
            .attach_sockmap(quic_sockhash_fd)
            .map_err(|e| anyhow::anyhow!("err: bpf_sk_reuseport => e:{}", e))?;
        Ok(())
    }

    pub fn add_sock_map(&mut self, socket_data: &SockMapData) -> anyhow::Result<()> {
        Self::do_add_sock_map(&mut self.skel, socket_data)
    }

    pub fn do_add_sock_map(
        skel: &mut AnyEbpfSkel<'_>,
        socket_data: &SockMapData,
    ) -> anyhow::Result<()> {
        let mut maps_mut = skel.maps_mut();

        log::trace!(target: "main", "socket_data:{:?}", socket_data);

        if socket_data.ups_fd > 0 {
            let ip = &socket_data.client_peer_addr.ip().to_string();
            let ips = ip.split(".").collect::<Vec<_>>();
            let peer_addr_ip = ips[3].parse::<u32>().unwrap() << 24
                | ips[2].parse::<u32>().unwrap() << 16
                | ips[1].parse::<u32>().unwrap() << 8
                | ips[0].parse::<u32>().unwrap();
            // log::info!("=====================add_sockhash");
            // log::info!("remote_ip4:{:?}", ip);
            // log::info!("remote_ip4:{:?}", peer_addr_ip);
            // log::info!("remote_port:{:?}", socket_data.client_peer_addr.port());

            let ip = &socket_data.client_local_addr.ip().to_string();
            let ips = ip.split(".").collect::<Vec<_>>();
            let local_addr_ip = ips[3].parse::<u32>().unwrap() << 24
                | ips[2].parse::<u32>().unwrap() << 16
                | ips[1].parse::<u32>().unwrap() << 8
                | ips[0].parse::<u32>().unwrap();
            // log::info!("local_ip4:{:?}", ip);
            // log::info!("local_ip4:{:?}", local_addr_ip);
            // log::info!("local_port:{:?}", socket_data.client_local_addr.port());
            // log::info!("fd:{:?}", socket_data.ups_fd);

            let key = sock_map_key {
                remote_ip4: peer_addr_ip,
                local_ip4: local_addr_ip,
                remote_port: socket_data.client_peer_addr.port() as u16,
                local_port: socket_data.client_local_addr.port() as u16,
                family: 2,
            };
            log::trace!(target: "main",
                "ups_fd:{}, remote_ip4:{}, local_ip4:{}, remote_port:{}, local_port:{}",
                socket_data.ups_fd,
                key.remote_ip4,
                key.local_ip4,
                key.remote_port,
                key.local_port
            );
            let key_bytes: &[u8] = unsafe { any_as_u8_slice(&key) };

            log::trace!(target: "main", "ups_fd:{}, key_bytes:{:?}", socket_data.ups_fd, key_bytes);
            maps_mut
                .sk_index_hash()
                .update(
                    key_bytes,
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

            // log::info!("=====================add_sockhash");
            // log::info!("remote_ip4:{:?}", ip);
            // log::info!("remote_ip4:{:?}", peer_addr_ip);
            // log::info!("remote_port:{:?}", socket_data.ups_peer_addr.port());

            let ip = &socket_data.ups_local_addr.ip().to_string();
            let ips = ip.split(".").collect::<Vec<_>>();
            let local_addr_ip = ips[3].parse::<u32>().unwrap() << 24
                | ips[2].parse::<u32>().unwrap() << 16
                | ips[1].parse::<u32>().unwrap() << 8
                | ips[0].parse::<u32>().unwrap();
            // log::info!("local_ip4:{:?}", ip);
            // log::info!("local_ip4:{:?}", local_addr_ip);
            // log::info!("local_port:{:?}", socket_data.ups_local_addr.port());
            // log::info!("fd:{:?}", socket_data.client_fd);

            let key = sock_map_key {
                remote_ip4: peer_addr_ip,
                local_ip4: local_addr_ip,
                remote_port: socket_data.ups_peer_addr.port() as u16,
                local_port: socket_data.ups_local_addr.port() as u16,
                family: 2,
            };
            log::trace!(target: "main",
                "client_fd:{}, remote_ip4:{}, local_ip4:{}, remote_port:{}, local_port:{}",
                socket_data.client_fd,
                key.remote_ip4,
                key.local_ip4,
                key.remote_port,
                key.local_port
            );
            let key_bytes: &[u8] = unsafe { any_as_u8_slice(&key) };

            log::trace!(target: "main",
                "client_fd:{}, key_bytes:{:?}",
                socket_data.client_fd,
                key_bytes
            );
            maps_mut
                .sk_index_hash()
                .update(
                    key_bytes,
                    &socket_data.client_fd.to_le_bytes(),
                    libbpf_rs::MapFlags::ANY,
                )
                .map_err(|e| anyhow::anyhow!("update e:{}", e))?;
        }

        if socket_data.client_fd > 0 {
            maps_mut
                .sk_sockmap()
                .update(
                    &socket_data.client_fd.to_le_bytes(),
                    &socket_data.client_fd.to_le_bytes(),
                    libbpf_rs::MapFlags::ANY,
                )
                .map_err(|e| anyhow::anyhow!("update e:{}", e))?;
        }

        if socket_data.ups_fd > 0 {
            maps_mut
                .sk_sockmap()
                .update(
                    &socket_data.ups_fd.to_le_bytes(),
                    &socket_data.ups_fd.to_le_bytes(),
                    libbpf_rs::MapFlags::ANY,
                )
                .map_err(|e| anyhow::anyhow!("update e:{}", e))?;
        }

        log::trace!(target: "main", "ebpf  success");

        Ok(())
    }

    pub fn del_sock_map(&mut self, socket_data: &SockMapData) -> anyhow::Result<()> {
        Self::do_del_sock_map(&mut self.skel, socket_data)
    }

    pub fn do_del_sock_map(
        skel: &mut AnyEbpfSkel<'_>,
        socket_data: &SockMapData,
    ) -> anyhow::Result<()> {
        let mut maps_mut = skel.maps_mut();

        log::trace!(target: "main", "del socket_data:{:?}", socket_data);

        // if socket_data.ups_fd > 0 {
        //     let _ = maps_mut
        //         .sk_sockmap()
        //         .delete(&socket_data.ups_fd.to_le_bytes())
        //         .map_err(|e| anyhow::anyhow!("update e:{}", e));
        // }
        //
        // if socket_data.client_fd > 0 {
        //     let _ = maps_mut
        //         .sk_sockmap()
        //         .delete(&socket_data.client_fd.to_le_bytes())
        //         .map_err(|e| anyhow::anyhow!("update e:{}", e));
        // }

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
                remote_port: socket_data.client_peer_addr.port() as u16,
                local_port: socket_data.client_local_addr.port() as u16,
                family: 2,
            };
            log::trace!(target: "main",
                "ups_fd:{}, remote_ip4:{}, local_ip4:{}, remote_port:{}, local_port:{}",
                socket_data.ups_fd,
                key.remote_ip4,
                key.local_ip4,
                key.remote_port,
                key.local_port
            );
            let key_bytes: &[u8] = unsafe { any_as_u8_slice(&key) };

            log::trace!(target: "main", "ups_fd:{}, key_bytes:{:?}", socket_data.ups_fd, key_bytes);
            maps_mut
                .sk_index_hash()
                .delete(key_bytes)
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
                remote_port: socket_data.ups_peer_addr.port() as u16,
                local_port: socket_data.ups_local_addr.port() as u16,
                family: 2,
            };
            log::trace!(target: "main",
                "client_fd:{}, remote_ip4:{}, local_ip4:{}, remote_port:{}, local_port:{}",
                socket_data.client_fd,
                key.remote_ip4,
                key.local_ip4,
                key.remote_port,
                key.local_port
            );
            let key_bytes: &[u8] = unsafe { any_as_u8_slice(&key) };

            log::trace!(target: "main",
                "client_fd:{}, key_bytes:{:?}",
                socket_data.client_fd,
                key_bytes
            );
            maps_mut
                .sk_index_hash()
                .delete(key_bytes)
                .map_err(|e| anyhow::anyhow!("update e:{}", e))?;
        }

        log::trace!(target: "main", "ebpf  success");

        Ok(())
    }

    pub fn add_reuseport(&mut self, reuseport_data: &ReuseportData) -> anyhow::Result<()> {
        Self::do_add_reuseport(&mut self.skel, reuseport_data)
    }

    pub fn do_add_reuseport(
        skel: &mut AnyEbpfSkel<'_>,
        reuseport_data: &ReuseportData,
    ) -> anyhow::Result<()> {
        if !cfg!(feature = "anyproxy-reuseport") {
            return Ok(());
        }
        log::trace!(target: "main", "reuseport_data:{:?}", reuseport_data);
        let mut maps_mut = skel.maps_mut();
        maps_mut
            .quic_sockhash()
            .update(
                &reuseport_data.dcid.to_le_bytes(),
                &reuseport_data.fd.to_le_bytes(),
                libbpf_rs::MapFlags::ANY,
            )
            .map_err(|e| anyhow::anyhow!("update e:{}", e))?;
        Ok(())
    }
}
