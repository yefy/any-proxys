#最简单ebpf代理配置
```
编译开启ebof
cargo build --release --features "anyproxy-openssl, anyproxy-ebpf" --no-default-features

需要根据流大小、网络情况来设置tcp缓冲区，必须保证写的速度要大于读的速度，不然会导致数据错误
配置文件参考：
any-proxys/any-proxy/examples/anyproxy/examples/conf/more_conf/anyproxy_simple_ebpf.conf
```