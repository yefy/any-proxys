#[cfg(feature = "anyproxy-ebpf")]
use libbpf_cargo::SkeletonBuilder;
#[cfg(feature = "anyproxy-ebpf")]
use std::fs::create_dir_all;
#[cfg(feature = "anyproxy-ebpf")]
use std::path::Path;
#[cfg(feature = "anyproxy-ebpf")]
const SRC: &str = "./src/ebpf/bpf/any_ebpf.bpf.c";

fn main() {
    #[cfg(feature = "anyproxy-ebpf")]
    {
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
}
