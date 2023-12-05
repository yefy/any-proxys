# 编译运行anyproxy
cd $HOME  
git clone https://github.com/yefy/any-proxys.git  
cd any-proxys  
cargo build --release --bin anyproxy  
cp target/release/anyproxy ./any-proxy/examples/anyproxy/examples  
cd ./any-proxy/examples/anyproxy/examples  
//默认读取 ./conf/anyproxy.conf, 可以使用-c 指定配置文件  

(如果是使用CLion打开项目，Working directory 选择./any-proxy/examples/anyproxy/examples  )


# 配置文件路径
any-proxys/any-proxy/examples/anyproxy/examples/conf  

# vim /etc/hosts
host 127.0.0.1 www.upstream.cn  
host 127.0.0.1 www.example.cn  

开启源站  
./anyproxy -c conf/anyproxy_origin.conf或是使用nginx见下面链接
[nginx 源站 支持 http https](https://github.com/yefy/any-proxy/wiki/nginx-%E6%BA%90%E7%AB%99-%E6%94%AF%E6%8C%81-http-https)


中转代理  
./anyproxy -c conf/anyproxy_proxy_to_origin.conf  
边缘代理  
./anyproxy -c conf/anyproxy_edge_to_proxy.conf  

#测试  
anyproxy_edge_to_proxy.conf 文件由对应的curl测试  


# 目录使用
cert放置密钥  
conf放配置文件  
logs放access_log、anyproxy.log、anyproxy.pid  

