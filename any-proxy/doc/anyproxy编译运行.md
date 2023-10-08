# 编译运行anyproxy
cd $HOME  
git clone https://github.com/yefy/any-proxys.git  
cd any-proxys  
cargo build --release --bin anyproxy  
cp target/release/anyproxy ./any-proxy/anyproxy/examples/  
cd ./any-proxy/anyproxy/examples/  
//默认读取 ./conf/anyproxy.conf, 可以使用-c 指定配置文件  
./anyproxy  

# 目录使用
cert放置密钥  
conf放配置文件  
logs放access_log、anyproxy.log、anyproxy.pid

