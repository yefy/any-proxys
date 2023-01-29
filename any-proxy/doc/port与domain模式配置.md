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
下面例子都是在配置any-proxy/anyproxy/examples/conf/port_18080.conf里面
# 代理回源nginx
port_11131_to_tcp18080.conf  
回源到nginx无需配置is_proxy_protocol_hello = true

测试：
curl http://www.example.cn:11131 -v

# 边缘代理tcp加速
port_11132_to_tcp11131.conf

测试：
curl http://www.example.cn:11132 -v

# 边缘代理quic加速
port_11133_to_quic11131.conf

测试：
curl http://www.example.cn:11133 -v

# 边缘代理tunnel_tcp加速
port_11134_to_tunnel_tcp11131.conf

测试：
curl http://www.example.cn:11134 -v

# 边缘代理tunnel_quic加速
port_11135_to_tunnel_quic11131.conf

测试：
curl http://www.example.cn:11135 -v

# 边缘代理tunnel2_tcp加速
port_11136_to_tunnel2_tcp11131.conf

测试：
curl http://www.example.cn:11136 -v

# 边缘代理tunnel2_quic加速
port_11137_to_tunnel2_quic11131.conf

测试：
curl http://www.example.cn:11137 -v

# domain代理配置
domain_1444.conf  
