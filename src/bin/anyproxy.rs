use any_proxy::util::default_config;
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
use mimalloc::MiMalloc;
#[cfg(windows)]
#[global_allocator]
#[cfg(windows)]
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
        return Err(anyhow::anyhow!("err:log4rs::init_fil"))?;
    }

    #[cfg(feature = "anyproxy-rustls")]
    log::info!("anyproxy-rustls");

    #[cfg(feature = "anyproxy-openssl")]
    log::info!("anyproxy-openssl");

    if let Some(cpu_core_ids) = core_affinity::get_core_ids() {
        for cpu_core_id in cpu_core_ids.iter() {
            log::info!("cpu_core_id:{:?}", cpu_core_id);
        }
    }

    let executor = async_executors::TokioCtBuilder::new().build().unwrap();
    let ret = executor
        .clone()
        .block_on(async { async_main(executor).await });
    if let Err(e) = ret {
        log::error!("err:async_main => e:{}", e);
        return Err(anyhow::anyhow!("err:block_on"))?;
    }

    Ok(())
}

async fn async_main(executor: async_executors::TokioCt) -> anyhow::Result<()> {
    return Ok(());
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
