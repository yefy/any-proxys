use any_base::executor_local_spawn;
use any_base::executor_local_spawn::ExecutorLocalSpawn;
use any_proxy::anyproxy::anyproxy::Anyproxy;
use any_proxy::anyproxy::anyproxy::ArgsConfig;
use any_proxy::util::default_config;
use anyhow::anyhow;
use anyhow::Result;
use lazy_static::lazy_static;
use std::sync::Mutex;

/// unix内存管理器
#[cfg(unix)]
extern crate jemallocator;
#[cfg(unix)]
#[global_allocator]
#[cfg(unix)]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

/// window内存管理器
#[cfg(windows)]
#[cfg(all(
    target_arch = "x86_64",
    target_os = "windows",
    target_env = "msvc",
    target_vendor = "pc"
))]
use mimalloc::MiMalloc;

#[cfg(windows)]
#[cfg(all(
    target_arch = "x86_64",
    target_os = "windows",
    target_env = "msvc",
    target_vendor = "pc"
))]
#[global_allocator]
#[cfg(windows)]
#[cfg(all(
    target_arch = "x86_64",
    target_os = "windows",
    target_env = "msvc",
    target_vendor = "pc"
))]
static GLOBAL: MiMalloc = MiMalloc;

lazy_static! {
    static ref THREAD_PANIC_MUTEX: Mutex<()> = Mutex::new(());
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_IGN);
    }

    std::panic::set_hook(Box::new(thread_panic));
    if let Err(e) = log4rs::init_file(
        default_config::ANYPROXY_LOG_FULL_PATH.as_str(),
        Default::default(),
    ) {
        eprintln!("err:log4rs::init_file => e:{}", e);
        return Err(anyhow!("err:log4rs::init_fil"))?;
    }
    if let Err(e) = do_main() {
        log::error!("err:main => err:{}", e);
    }
    Ok(())
}

fn do_main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "anyproxy-rustls")]
    log::info!("anyproxy-rustls");

    #[cfg(feature = "anyproxy-openssl")]
    log::info!("anyproxy-openssl");

    let arg_config = ArgsConfig::load_from_args();
    log::info!("arg_config:{:?}", arg_config);
    if Anyproxy::parse_args(&arg_config)? {
        return Ok(());
    }

    if let Some(cpu_core_ids) = core_affinity::get_core_ids() {
        log::info!("cpu_core_id num:{:?}", cpu_core_ids.len());
        for cpu_core_id in cpu_core_ids.iter() {
            log::debug!("cpu_core_id:{:?}", cpu_core_id);
        }
    }

    executor_local_spawn::_block_on(move |executor_local_spawn| async move {
        async_main(executor_local_spawn).await
    })
    .map_err(|e| anyhow!("err:anyproxy block_on => e:{}", e))?;
    Ok(())
}

async fn async_main(executor_local_spawn: ExecutorLocalSpawn) -> Result<()> {
    let mut anyproxy = Anyproxy::new(executor_local_spawn)?;
    anyproxy.start().await
}

/// 捕获异常信号
fn thread_panic(panic_info: &std::panic::PanicInfo) {
    let _ = THREAD_PANIC_MUTEX.lock();

    let curr_thread = std::thread::current();
    let curr_thread_name = curr_thread.name().unwrap_or("thread_panic");

    let payload = match panic_info.payload().downcast_ref::<&'static str>() {
        Some(payload) => *payload,
        None => match panic_info.payload().downcast_ref::<String>() {
            Some(payload) => &payload[..],
            None => "payload",
        },
    };

    let location = if let Some(location) = panic_info.location() {
        location.to_string()
    } else {
        "location".to_string()
    };

    log::error!(
        "thread_panic payload:{} curr_thread_name:{} location:{} backtrace:{:?}",
        payload,
        curr_thread_name,
        location,
        backtrace::Backtrace::new()
    );

    std::process::exit(1);
}
