use std::net::{SocketAddr, ToSocketAddrs};
extern crate rand;
use anyhow::anyhow;
use anyhow::Result;
use rand::Rng;

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
                let mut rng = rand::thread_rng();
                let index: u8 = rng.gen();
                let index = index as usize % addrs.len();
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
