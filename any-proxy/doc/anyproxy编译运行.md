# 编译运行anyproxy
cd $HOME  
git clone https://github.com/yefy/any-proxys.git  
cd any-proxys  
```
linux:需要安装: 
apt install gcc
apt install g++
apt install make
apt install pkg-config
apt install openssl
apt install libssl-dev
安装rust，如果安装失败，请看rust官方安装教程
安装rust: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
这里可能需要重启终端
rustup install 1.74.1
rustup toolchain list
rustup default 1.74.1-x86_64-unknown-linux-gnu

cargo install cargo-component

window:需要安装: 
安装openssl: 下载Win64OpenSSL-1_1_1k.exe安装
安装rust: 请看rust官方安装教程, 自己下载二进制安装
rustup install 1.74.1
rustup toolchain list
rustup default 1.74.1-x86_64-unknown-linux-gnu

cargo install cargo-component

如果本地没安装openssl建议使用rustls库(程序内嵌，无需安装): 
cargo build --release --bin anyproxy  --no-default-features --features "anyproxy-rustls"  
```
rust一定要用--release编译， release和debug性能几十倍的差异  
cargo build --release --bin anyproxy  
cp target/release/anyproxy ./any-example/anyproxy  
cd ./any-example/anyproxy  

```
./anyproxy
默认读取 ./conf/anyproxy.conf, 可以使用-c 指定配置文件  
./anyproxy -c conf/anyproxy_edge_to_proxy.conf  
```

```
如果是使用CLion打开项目，  
Working directory设置 ./any-example/anyproxy  
Command设置 run --release --no-default-features --features "anyproxy-rustls" -- -c conf/anyproxy_edge_to_proxy.conf    
```


# 设置测试域名
```
vim /etc/hosts
host 127.0.0.1 www.upstream.cn  
host 127.0.0.1 www.example.cn  
```

#开启源站
./anyproxy -c conf/anyproxy_origin.conf或是使用nginx做源站见下面链接  
[nginx 源站 支持 http https](https://github.com/yefy/any-proxys/blob/main/any-proxy/doc/nginx%E6%BA%90%E7%AB%99%E6%94%AF%E6%8C%81http%E5%92%8Chttps.md)  

#中转代理  
./anyproxy -c conf/anyproxy_proxy_to_origin.conf  
#边缘代理  
./anyproxy -c conf/anyproxy_edge_to_proxy.conf  

#测试  
anyproxy_edge_to_proxy.conf 文件有对应的curl测试  


# 目录使用
cert放置密钥  
conf放配置文件  
logs放access_log、anyproxy.log、anyproxy.pid  
tmp临时文件目录  
html静态文件目录  

