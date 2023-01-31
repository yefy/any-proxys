use std::net::{SocketAddr, ToSocketAddrs};
extern crate rand;
use anyhow::anyhow;
use anyhow::Result;
use rand::Rng;
use std::fs;
use std::path::Path;

pub fn str_addrs(addr: &str) -> Result<Vec<String>> {
    let mut datas = Vec::with_capacity(20);
    let addrs = addr.split(" ").collect::<Vec<_>>();
    for addr in addrs {
        let addr = addr.trim();
        if addr.len() <= 0 {
            continue;
        }
        let values = addr.split(":").collect::<Vec<_>>();
        if values.len() != 2 {
            return Err(anyhow!("err:addr => addr:{}", addr));
        }
        let ip = values[0].trim();
        let port = values[1].trim();
        if port.find("~").is_some() {
            let find = port.find("[");
            if find.is_none() || (find.is_some() && find.unwrap() != 0) {
                return Err(anyhow!("err:addr => addr:{}", addr));
            }

            let find = port.find("]");
            if find.is_none() || (find.is_some() && find.unwrap() + 1 != port.len()) {
                return Err(anyhow!("err:addr => addr:{}", addr));
            }

            let port = &port[1..port.len() - 1];
            let ports = port.split("~").collect::<Vec<_>>();
            let port_start = ports[0]
                .trim()
                .parse::<usize>()
                .map_err(|e| anyhow!("err:addr => addr:{}, e:{}", addr, e))?;
            let port_end = ports[1]
                .trim()
                .parse::<usize>()
                .map_err(|e| anyhow!("err:addr => addr:{}, e:{}", addr, e))?;
            if port_end < port_start {
                return Err(anyhow!("err:addr => addr:{}", addr));
            }
            let port_end = port_end + 1;
            for v in port_start..port_end {
                let addr = format!("{}:{}", ip, v);
                datas.push(addr)
            }
        } else {
            datas.push(addr.to_string())
        }
    }

    if datas.len() <= 0 {
        return Err(anyhow!("err:addr => addr:{}", addr));
    }
    Ok(datas)
}

pub fn addrs(str_addrs: &Vec<String>) -> Result<Vec<SocketAddr>> {
    let mut addrs = Vec::with_capacity(10);
    for addr in str_addrs.iter() {
        let addr = addr
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "err:empty address"))?;
        addrs.push(addr);
    }
    Ok(addrs)
}

pub fn addr(str_addr: &str) -> Result<SocketAddr> {
    let addr = str_addr
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "err:empty address"))?;
    Ok(addr)
}

pub async fn lookup_host(timeout: tokio::time::Duration, address: &str) -> Result<SocketAddr> {
    match tokio::time::timeout(timeout, tokio::net::lookup_host(address)).await {
        Ok(addrs) => match addrs {
            Ok(addrs) => {
                let addrs = addrs.into_iter().collect::<Vec<_>>();
                let index: usize = rand::thread_rng().gen();
                let index = index % addrs.len();
                return Ok(addrs[index]);
            }
            Err(e) => Err(anyhow!(
                "err:address timeout => address:{}, e:{}",
                address,
                e
            )),
        },
        Err(_) => Err(anyhow!("err:address timeout => address:{}", address)),
    }
}

pub async fn lookup_hosts(
    timeout: tokio::time::Duration,
    address: &str,
) -> Result<Vec<SocketAddr>> {
    match tokio::time::timeout(timeout, tokio::net::lookup_host(address)).await {
        Ok(addrs) => match addrs {
            Ok(addrs) => {
                let addrs = addrs.into_iter().collect::<Vec<_>>();
                return Ok(addrs);
            }
            Err(e) => Err(anyhow!(
                "err:address timeout => address:{}, e:{}",
                address,
                e
            )),
        },
        Err(_) => Err(anyhow!("err:address timeout => address:{}", address)),
    }
}

pub fn get_ports(ports: &str) -> Result<Vec<u16>> {
    let mut v = vec![];
    let ports = ports.trim();
    let addrs = ports.split(",").collect::<Vec<_>>();
    for addr in addrs.iter() {
        let addr = addr.trim();
        if addr.len() <= 0 {
            continue;
        }
        let addrs = addr.split("-").collect::<Vec<_>>();
        if addrs.len() == 1 {
            let port = addrs[0].parse::<u16>();
            if port.is_err() {
                continue;
            }
            v.push(port.unwrap());
        } else if addrs.len() == 2 {
            let port1 = addrs[0].parse::<u16>();
            if port1.is_err() {
                continue;
            }
            let port1 = port1.unwrap();

            let port2 = addrs[0].parse::<u16>();
            if port2.is_err() {
                continue;
            }
            let port2 = port2.unwrap();

            if port2 < port1 {
                continue;
            }

            for p in port1..port2 {
                v.push(p);
            }
        } else {
            continue;
        }
    }
    Ok(v)
}

#[cfg(unix)]
pub fn memlock_rlimit(curr: u64, max: u64) -> Result<()> {
    let rlimit = libc::rlimit {
        rlim_cur: curr,
        rlim_max: max,
    };

    if unsafe { libc::setrlimit(libc::RLIMIT_MEMLOCK, &rlimit) } != 0 {
        return Err(anyhow!("Failed to increase rlimit:{:?}", rlimit));
    }
    Ok(())
}

pub fn extract(file_name: &str, target: &str) -> anyhow::Result<()> {
    println!("file_name:{}", file_name);
    let file_name = std::path::Path::new(file_name);
    let zipfile = std::fs::File::open(file_name)
        .map_err(|e| anyhow::anyhow!("err:open file => file_name:{:?}, e:{}", file_name, e))?;
    let mut zip = zip::ZipArchive::new(zipfile).map_err(|e| {
        anyhow::anyhow!(
            "err:zip::ZipArchive::new => file_name:{:?}, e:{}",
            file_name,
            e
        )
    })?;
    let target = std::path::Path::new(target);

    if !target.exists() {
        fs::create_dir_all(target)
            .map_err(|e| anyhow::anyhow!("err:create_dir_all => target:{:?}, e:{}", target, e))?;
    }

    for i in 0..zip.len() {
        let mut file = zip
            .by_index(i)
            .map_err(|e| anyhow::anyhow!("err:by_index => i:{}, e:{}", i, e))?;
        println!("zip file name: {}", file.name());
        if file.is_dir() {
            println!("file utf8 path {:?}", file.name_raw());
            let target = target.join(Path::new(&file.name().replace("\\", "")));
            println!("create_dir_all {:?}", target);
            fs::create_dir_all(&target).map_err(|e| {
                anyhow::anyhow!("err:create_dir_all => target:{:?}, e:{}", target, e)
            })?;
        } else {
            let file_path = target.join(Path::new(file.name()));
            if file_path.exists() {
                std::fs::remove_file(&file_path).map_err(|e| {
                    anyhow::anyhow!("err::remove_file => file_path:{:?}, e:{}", file_path, e)
                })?;
            }
            println!("create file_path {:?}", file_path);
            let mut target_file = std::fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open(&file_path)
                .map_err(|e| anyhow::anyhow!("err::open => file_path:{:?}, e:{}", file_path, e))?;
            std::io::copy(&mut file, &mut target_file)
                .map_err(|e| anyhow::anyhow!("err:copy => e:{}", e))?;
            // Get and Set permissions
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;

                if let Some(mode) = file.unix_mode() {
                    fs::set_permissions(&file_path, fs::Permissions::from_mode(mode))
                        .map_err(|e| anyhow::anyhow!("err:set_permissions => e:{}", e))?;
                }
            }
        }
    }
    Ok(())
}
