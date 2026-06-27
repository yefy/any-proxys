use any_proxy::anyproxy::run;

// /// unix内存管理器
// #[cfg(unix)]
// #[global_allocator]

// #[cfg(unix)]
// static GLOBAL: jemallocator::Jemalloc = jemallocator::Jemalloc;

/// unix内存管理器
#[cfg(unix)]
#[global_allocator]
#[cfg(unix)]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

/// window内存管理器
#[cfg(windows)]
#[cfg(all(target_arch = "x86_64", target_os = "windows", target_env = "msvc"))]
#[global_allocator]
#[cfg(windows)]
#[cfg(all(target_arch = "x86_64", target_os = "windows", target_env = "msvc"))]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    run::<()>(1, None)?;
    Ok(())
}
