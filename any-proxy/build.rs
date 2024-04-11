#[cfg(feature = "anyproxy-ebpf")]
use libbpf_cargo::SkeletonBuilder;
#[cfg(feature = "anyproxy-ebpf")]
use std::fs::create_dir_all;
#[cfg(feature = "anyproxy-ebpf")]
const SRC: &str = "./src/ebpf/bpf/any_ebpf.bpf.c";
use anyhow::Result;
use std::fs;
use std::path::Path;

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

fn main() -> Result<()> {
    println!("cargo:rerun-if-changed=build.rs");

    let mut config = vergen::Config::default();
    *config.git_mut().sha_kind_mut() = vergen::ShaKind::Short;
    *config.git_mut().commit_timestamp_kind_mut() = vergen::TimestampKind::DateOnly;
    let _ = vergen::vergen(config).map_err(|e| anyhow::anyhow!("err:ergen::vergen => e:{}", e));

    #[cfg(feature = "anyproxy-ebpf")]
    {
        let file_name = "./src/ebpf/bpf/vmlinux.h";
        let zip_file_name = "./src/ebpf/bpf/vmlinux.zip";
        let output = "./src/ebpf/bpf/";
        if !Path::new(&file_name).exists() {
            if let Err(e) = extract(zip_file_name, output) {
                println!("err:extract => e:{}", e);
                std::process::exit(1);
            }
        }

        // It's unfortunate we cannot use `OUT_DIR` to store the generated skeleton.
        // Reasons are because the generated skeleton contains compiler attributes
        // that cannot be `include!()`ed via macro. And we cannot use the `#[path = "..."]`
        // trick either because you cannot yet `concat!(env!("OUT_DIR"), "/skel.rs")` inside
        // the path attribute either (see https://github.com/rust-lang/rust/pull/83366).
        //
        // However, there is hope! When the above feature stabilizes we can clean this
        // all up.
        create_dir_all("./src/ebpf/bpf/.output").unwrap();
        let skel = Path::new("./src/ebpf/bpf/.output/any_ebpf.skel.rs");

        SkeletonBuilder::new()
            .source(SRC)
            .build_and_generate(skel)
            .expect("bpf compilation failed");
        println!("cargo:rerun-if-changed={}", SRC);
    }
    Ok(())
}
