// SPDX-License-Identifier: GPL-2.0 OR BSD-3-Clause
#include "vmlinux.h"
#include <bpf/bpf_helpers.h>
#include <bpf/bpf_core_read.h>
#include <bpf/bpf_tracing.h>
#include <bpf/bpf_endian.h>

/********BPF_MAP_TYPE_SOCKHASH*******/
struct sock_key {
    __u32 remote_ip4;
    __u32 local_ip4;
    __u32 remote_port;
    __u32 local_port;
    __u32  family;
    __u32 pad0;
} __attribute__((packed));

struct {
    __uint(type, BPF_MAP_TYPE_SOCKHASH);
    __uint(max_entries, 10240);
    __uint(key_size, sizeof(struct sock_key));
    __uint(value_size, sizeof(int));
} sock_hash SEC(".maps");

static __always_inline
void extract_key4_from_ops(struct bpf_sock_ops *ops, struct sock_key *key)
{
    // keep ip and port in network byte order
    // local_port is in host byte order, and
    // remote_port is in network byte order
    key->remote_ip4 = ops->remote_ip4;
    key->remote_port = bpf_ntohl(ops->remote_port);
    key->local_ip4 = ops->local_ip4;
    key->local_port = ops->local_port;
    key->family = ops->family;
    key->pad0 = 0;

}


static __always_inline
void bpf_sock_ops_ipv4(struct bpf_sock_ops *skops)
{
    struct sock_key key = {};
    int ret;

    extract_key4_from_ops(skops, &key);

    bpf_printk("bpf_sockops remote_ip4:remote_port = %d.%d\n", (u8)key.remote_ip4, (u8)(key.remote_ip4 >> 8));
    bpf_printk("bpf_sockops remote_ip4:remote_port = %d.%d\n", (u8)(key.remote_ip4 >> 16), key.remote_ip4 >> 24);
    bpf_printk("bpf_sockops remote_ip4:remote_port = %d\n", key.remote_port);

    bpf_printk("bpf_sockops local_ip4:local_port = %d.%d\n", (u8)key.local_ip4, (u8)(key.local_ip4 >> 8));
    bpf_printk("bpf_sockops local_ip4:local_port = %d.%d\n", (u8)(key.local_ip4 >> 16), key.local_ip4 >> 24);
    bpf_printk("bpf_sockops local_ip4:local_port = %d\n", key.local_port);

    ret = bpf_sock_hash_update(skops, &sock_hash, &key, BPF_NOEXIST);

    if (ret != 0) {
        bpf_printk("bpf_sockops bpf_sock_hash_update() failed. %d\n", -ret);
        return;
    }
    bpf_printk("bpf_sockops op: %d, port %d --> %d\n", skops->op,
               key.local_port, key.remote_port);
}

SEC("sockops")
int bpf_sockops(struct bpf_sock_ops *skops)
{
    bpf_printk("bpf_sockops\n");
    switch (skops->op) {
        case BPF_SOCK_OPS_PASSIVE_ESTABLISHED_CB:
        case BPF_SOCK_OPS_ACTIVE_ESTABLISHED_CB:
            if (skops->family == 2) { //AF_INET
                bpf_sock_ops_ipv4(skops);
            }
            break;
        default:
            break;
    }
    return 0;
}

static __always_inline
void extract_key4_from_msg(struct sk_msg_md *msg, struct sock_key *key)
{
    key->remote_ip4 = msg->local_ip4;
    key->remote_port = msg->local_port;
    key->local_ip4 = msg->remote_ip4;
    key->local_port = bpf_ntohl(msg->remote_port);
    key->family = msg->family;
    key->pad0 = 0;
}

SEC("sk_msg")
int bpf_sk_msg_verdict(struct sk_msg_md *msg)
{
    bpf_printk("bpf_sk_msg_verdict\n");

    int ret = 0;
    struct sock_key key = {};
    extract_key4_from_msg(msg, &key);

    bpf_printk("bpf_sk_msg_verdict remote_ip4:remote_port = %d.%d\n", (u8)key.remote_ip4, (u8)(key.remote_ip4 >> 8));
    bpf_printk("bpf_sk_msg_verdict remote_ip4:remote_port = %d.%d\n", (u8)(key.remote_ip4 >> 16), key.remote_ip4 >> 24);
    bpf_printk("bpf_sk_msg_verdict remote_ip4:remote_port = %d\n", key.remote_port);

    bpf_printk("bpf_sk_msg_verdict local_ip4:local_port = %d.%d\n", (u8)key.local_ip4, (u8)(key.local_ip4 >> 8));
    bpf_printk("bpf_sk_msg_verdict local_ip4:local_port = %d.%d\n", (u8)(key.local_ip4 >> 16), key.local_ip4 >> 24);
    bpf_printk("bpf_sk_msg_verdict local_ip4:local_port = %d\n", key.local_port);

    if (msg->family != 2) //AF_INET
        return SK_PASS;
    if (msg->remote_ip4 != msg->local_ip4)
        return SK_PASS;

    ret = bpf_msg_redirect_hash(msg, &sock_hash, &key, BPF_F_INGRESS);

    bpf_printk("bpf_sk_msg_verdict try redirect port %d --> %d\n", msg->local_port,
                     bpf_ntohl(msg->remote_port));
    if (ret != SK_PASS)
        bpf_printk("bpf_sk_msg_verdict redirect port %d --> %d failed\n", msg->local_port,
                         bpf_ntohl(msg->remote_port));
    return ret;
}








/********BPF_MAP_TYPE_SOCKMAP*******/
struct {
    __uint(type, BPF_MAP_TYPE_SOCKMAP);
    __uint(max_entries, 10240);
    __uint(key_size, sizeof(int));
    __uint(value_size, sizeof(int));
} sock_map SEC(".maps");

struct sock_map_key {
    __u32 remote_ip4;
    __u32 local_ip4;
    __u32 remote_port;
    __u32 local_port;
    __u32  family;
    __u32 pad0;
} __attribute__((packed));

struct {
    __uint(type, BPF_MAP_TYPE_HASH);
    __uint(max_entries, 10240);
    __type(key, struct sock_map_key);
    __type(value, int);
} sock_index_map SEC(".maps");


SEC("sk_skb/stream_parser")
int bpf_sk_skb__stream_parser(struct __sk_buff *skb)
{
    bpf_printk("bpf_sk_skb__stream_parser skb->len:%u, %p\n", skb->len, skb);
    return skb->len;
}

SEC("sk_skb/stream_verdict")
int bpf_sk_skb__stream_verdict(struct __sk_buff *skb)
{
    bpf_printk("=====================bpf_sk_skb__stream_verdict, %p\n", skb);
    bpf_printk("bpf_sk_skb__stream_verdict remote_ip4 = %d.%d\n", (u8)skb->remote_ip4, (u8)(skb->remote_ip4 >> 8));
    bpf_printk("bpf_sk_skb__stream_verdict remote_ip4 = %d.%d\n", (u8)(skb->remote_ip4 >> 16), skb->remote_ip4 >> 24);
    bpf_printk("bpf_sk_skb__stream_verdict remote_ip4 = %u\n", skb->remote_ip4);
    bpf_printk("bpf_sk_skb__stream_verdict remote_port = %d\n", bpf_ntohl(skb->remote_port));

    bpf_printk("bpf_sk_skb__stream_verdict local_ip4 = %d.%d\n", (u8)skb->local_ip4, (u8)(skb->local_ip4 >> 8));
    bpf_printk("bpf_sk_skb__stream_verdict local_ip4 = %d.%d\n", (u8)(skb->local_ip4 >> 16), skb->local_ip4 >> 24);
    bpf_printk("bpf_sk_skb__stream_verdict local_ip4 = %u\n", skb->local_ip4);
    bpf_printk("bpf_sk_skb__stream_verdict local_port = %d\n", skb->local_port);


    struct sock_map_key map_key = {};
    map_key.remote_ip4 = skb->remote_ip4;
    map_key.local_ip4 = skb->local_ip4;
    map_key.remote_port = bpf_ntohl(skb->remote_port);
    map_key.local_port = skb->local_port;
    map_key.family = 2;
    map_key.pad0 = 0;

    int *fd = bpf_map_lookup_elem(&sock_index_map, &map_key);
    if (fd == NULL) {
        bpf_printk("bpf_sk_skb__stream_verdict fd == NULL, %p\n", skb);
        return 0;
    }

    bpf_printk("bpf_sk_skb__stream_verdict *fd = %d, %p\n", *fd, skb);
    return bpf_sk_redirect_map(skb, &sock_map, *fd, 0);
}

char LICENSE[] SEC("license") = "Dual BSD/GPL";