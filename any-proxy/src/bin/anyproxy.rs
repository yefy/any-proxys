use any_base::executor_local_spawn;
use any_base::executor_local_spawn::ExecutorLocalSpawn;
use any_proxy::anyproxy::anyproxy::Anyproxy;
use any_proxy::anyproxy::anyproxy::ArgsConfig;
use any_proxy::util::default_config;
use anyhow::anyhow;
use anyhow::Result;
use lazy_static::lazy_static;
use std::sync::Mutex;
extern crate page_size;
use any_proxy::anyproxy::anymodule;

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
use std::sync::atomic::Ordering;

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
        default_config::ANYPROXY_LOG_FULL_PATH.get().as_str(),
        Default::default(),
    ) {
        eprintln!("err:log4rs::init_file => e:{}", e);
        return Err(anyhow!("err:log4rs::init_file"))?;
    }
    if let Err(e) = do_main() {
        log::error!("err:main => err:{}", e);
    }
    Ok(())
}

fn do_main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(debug_assertions)]
    log::info!("DEBUG");
    #[cfg(not(debug_assertions))]
    log::info!("RELEASE");

    log::info!("BUILD_VERSION: {}", default_config::BUILD_VERSION.as_str());
    log::info!("HTTP_VERSION: {}", default_config::HTTP_VERSION.as_str());
    log::info!("pwd:{:?}", std::env::current_dir()?);
    let page_size = page_size::get();
    default_config::PAGE_SIZE.store(page_size, Ordering::Relaxed);
    log::info!("page_size:{:?}", page_size);

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
            log::debug!(target: "main", "cpu_core_id:{:?}", cpu_core_id);
        }
    }

    {
        log::info!(
            "config full path :{}",
            default_config::ANYPROXY_CONF_FULL_PATH.get().as_str()
        );
    }

    anymodule::add_modules()?;
    use any_base::module::module;
    module::parse_modules()?;

    executor_local_spawn::_block_on(
        1,
        512,
        move |executor| async move { async_main(executor).await },
    )
    .map_err(|e| anyhow!("err:anyproxy block_on => e:{}", e))?;
    Ok(())
}

async fn async_main(executor: ExecutorLocalSpawn) -> Result<()> {
    //console_subscriber::init();
    let mut anyproxy = Anyproxy::new(executor)?;
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

/*
use std::collections::HashMap;
use std::path::Path;
use anyhow::bail;
use std::fs;
use wit_bindgen_core::{Files, WorldGenerator};
use wit_parser::{Resolve, UnresolvedPackage};

fn build_wit_guest_code() {
    // loop wit directory, find .wit files , check same name as .rs file, if not, generate it
    //let wit_dir = Path::new("../wasm/http-filter-headers/wit");
    let wit_dir = Path::new(
        "C:/Users/yefy/Desktop/yefy/develop/git-project/any-proxys/wasm/http-filter-headers/wit",
    );
    let wit_files = wit_dir.read_dir().unwrap();
    for wit_file in wit_files {
        let wit_file_path = wit_file.unwrap().path();
        if !wit_file_path.is_file() {
            continue;
        }
        if wit_file_path.extension().unwrap() != "wit" {
            continue;
        }
        println!(
            "wit_file_path:{}",
            wit_file_path.as_os_str().to_str().unwrap()
        );
        let outputs = generate_world_guest(wit_file_path.to_str().unwrap(), None).unwrap();
        outputs.iter().for_each(|(path, content)| {
            let target_rs = wit_dir.join(path);
            std::fs::write(target_rs, content).unwrap();
        });
    }
}

/// parse wit file and return world id
pub fn generate_world_guest(s: &str, world: Option<String>) -> Result<HashMap<String, String>> {
    // parse exported world in wit file
    let path = Path::new(s);
    if !path.is_file() {
        panic!("wit file `{}` does not exist", path.display());
    }

    let mut resolve = Resolve::default();
    println!("path:{:?}", path);
    let package = UnresolvedPackage::parse_file(path).map_err(|e| anyhow!("e:{}", e))?;
    let pkg = resolve.push(package).map_err(|e| anyhow!("e:{}", e))?;
    let package = resolve.packages.get(pkg).unwrap();

    let world = match &world {
        Some(world) => {
            let mut parts = world.splitn(2, '.');
            let doc = parts.next().unwrap();
            let world = parts.next().unwrap();
            package
                .worlds
                .get(world)
                .ok_or_else(|| anyhow!("no world named `{name}` in document"))?
        }
        None => {
            let mut world = package.worlds.iter();
            let (_, world) = world
                .next()
                .ok_or_else(|| anyhow!("no documents found in package"))?;
            world
        }
    };

    //get guest genrator
    let mut generator = gen_guest_code_builder().map_err(|e| anyhow!("e:{}", e))?;

    // generate file
    let mut files = Files::default();
    generator
        .generate(&resolve, *world, &mut files)
        .map_err(|e| anyhow!("e:{}", e))?;

    let mut output_maps = HashMap::new();
    for (name, contents) in files.iter() {
        output_maps.insert(
            name.to_string(),
            String::from_utf8_lossy(contents).to_string(),
        );
    }
    Ok(output_maps)
}

fn gen_guest_code_builder() -> Result<Box<dyn WorldGenerator>> {
    let opts = wit_bindgen_rust::Opts {
        rustfmt: true,
        ..Default::default()
    };
    let builder = opts.build();
    Ok(builder)
}
 */
