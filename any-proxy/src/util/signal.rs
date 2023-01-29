pub async fn quit() -> bool {
    #[cfg(unix)]
    {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::quit()) {
            Err(_) => return false,
            Ok(mut sig) => {
                let _ = sig.recv().await;
                return true;
            }
        }
    }
    #[cfg(windows)]
    {
        tokio::time::sleep(std::time::Duration::from_secs(1000000)).await;
        return false;
    }
}

pub async fn stop() -> bool {
    #[cfg(unix)]
    {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Err(_) => return false,
            Ok(mut sig) => {
                let _ = sig.recv().await;
                return true;
            }
        }
    }
    #[cfg(windows)]
    {
        tokio::time::sleep(std::time::Duration::from_secs(1000000)).await;
        return false;
    }
}

pub async fn hup() -> bool {
    #[cfg(unix)]
    {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup()) {
            Err(_) => return false,
            Ok(mut sig) => {
                let _ = sig.recv().await;
                return true;
            }
        }
    }
    #[cfg(windows)]
    {
        tokio::time::sleep(std::time::Duration::from_secs(1000000)).await;
        return false;
    }
}

pub async fn user1() -> bool {
    #[cfg(unix)]
    {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::user_defined1()) {
            Err(_) => return false,
            Ok(mut sig) => {
                let _ = sig.recv().await;
                return true;
            }
        }
    }
    #[cfg(windows)]
    {
        tokio::time::sleep(std::time::Duration::from_secs(1000000)).await;
        return false;
    }
}

pub async fn user2() -> bool {
    #[cfg(unix)]
    {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::user_defined2()) {
            Err(_) => return false,
            Ok(mut sig) => {
                let _ = sig.recv().await;
                return true;
            }
        }
    }
    #[cfg(windows)]
    {
        tokio::time::sleep(std::time::Duration::from_secs(1000000)).await;
        return false;
    }
}
