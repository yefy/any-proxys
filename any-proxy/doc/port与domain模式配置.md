#区别
domain模式是ssl代理，会根据sni获取域名，并根据域名获取到指定配置  
port模式是端口代理，根据端口获取配置

# 配置文件路径
any-proxy/anyproxy/examples/conf  

# nginx源站配置
[nginx 源站 支持 http https](https://github.com/yefy/any-proxy/wiki/nginx-%E6%BA%90%E7%AB%99-%E6%94%AF%E6%8C%81-http-https)

# vim /etc/hosts
host 127.0.0.1 www.upstream.cn  
host 127.0.0.1 www.example.cn  

# port 代理配置
参考any-proxy/anyproxy/examples/conf/anyproxy_port_simple.conf

# domain代理配置
参考any-proxy/anyproxy/examples/conf/anyproxy_domain_simple.conf
