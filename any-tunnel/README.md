# any-tunnel
any-tunnel 建立在tcp，quic，srt等可靠协议之上

# 原理
1、没开启链接缓冲池  
any-tunnel 使用1:N模式，创建N个短连接建立一个隧道，可以在隧道上创建1个连接，第一个链接与tcp，quic，srt使用一样
（如果是tcp需要等待三次握手），其它链接都是异步开启不需要等待，结束链接释放  
2、开启链接缓冲池  
配置链接池大小，链接复用，无需等待三次握手和四次断开，但是会占用socket链接和端口  
3、某个链接断开流结束  

# 解决问题
解决高延迟带宽单连接带宽低问题、短连接加速频繁连接和断开握手问题，可以达到提速的效果

# example
cargo run --example server  
cargo run --example client  
