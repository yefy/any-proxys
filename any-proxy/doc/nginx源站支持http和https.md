# 编译nginx
cd $HOME  
wget http://nginx.org/download/nginx-1.18.0.tar.gz  
tar -xvf nginx-1.18.0.tar.gz  
cd nginx-1.18.0/  
./configure  --prefix=./nginx --with-http_ssl_module --with-file-aio --with-http_v2_module  
make  
make install

# 运行nginx
cp -r $HOME/any-proxys/any-proxy/examples/anyproxy/nginx/conf/* ./nginx/conf/  
./nginx/sbin/nginx

# 添加host
vim /etc/hosts

127.0.0.1 www.example.cn  
127.0.0.1 www.upstream.cn

# 测试
http:  
curl http://www.example.cn:19090 -k -v  
https:  
curl https://www.example.cn:19091 -k -v  
