# 编译运行anyproxy
cd $HOME  
git clone https://github.com/yefy/any-proxy.git  
cd any-proxy  
cargo build --release --bin anyproxy  
cp target/release/anyproxy ./anyproxy/examples/  
cd ./anyproxy/examples/  
./anyproxy  

# 配置读取
cert放置密钥  
conf放配置文件  
logs放access_log、anyproxy.log、anyproxy.pid  

any-proxy\anyproxy\examples/conf/anyproxy.conf
any-proxy\anyproxy\examples/conf/port_18080.conf

