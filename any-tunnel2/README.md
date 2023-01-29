# any-tunnel2
1、any-tunnel2 是实验性质不稳定  
2、any-tunnel2 需要处理超时重传、ack、滑动窗口，性能不是很理想、可能方案不可行，可以使用any-tunnel的链接池来解决短链接加速问题
3、any-tunnel2 建立在tcp，quic，srt等可靠协议之上

# 原理
any-tunnel2 使用M:N模式，创建N个共享长连接建立一个隧道，可以在隧道上创建M个连接，创建连接非常快，不需要连接握手和断开握手

# 解决问题
解决高延迟带宽单连接带宽低问题、短连接加速频繁连接和断开握手问题，链接断开不影响使用，可以达到提速的效果

# example
cargo run --example server  
cargo run --example client  
