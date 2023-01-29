// SPDX-License-Identifier: GPL-2.0 OR BSD-3-Clause
#include "vmlinux.h"
#include <bpf/bpf_helpers.h>
#include <bpf/bpf_core_read.h>
#include <bpf/bpf_tracing.h>
#include <bpf/bpf_endian.h>


#define MAP_MAX_ENTRIES 10240

/********sockops,sk_msg*******/
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
    __uint(max_entries, MAP_MAX_ENTRIES);
    __uint(key_size, sizeof(struct sock_key));
    __uint(value_size, sizeof(int));
} sk_sockhash SEC(".maps");

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

    ret = bpf_sock_hash_update(skops, &sk_sockhash, &key, BPF_NOEXIST);

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

    ret = bpf_msg_redirect_hash(msg, &sk_sockhash, &key, BPF_F_INGRESS);

    bpf_printk("bpf_sk_msg_verdict try redirect port %d --> %d\n", msg->local_port,
                     bpf_ntohl(msg->remote_port));
    if (ret != SK_PASS)
        bpf_printk("bpf_sk_msg_verdict redirect port %d --> %d failed\n", msg->local_port,
                         bpf_ntohl(msg->remote_port));
    return ret;
}








/********stream_parser,stream_verdict*******/
struct {
    __uint(type, BPF_MAP_TYPE_SOCKMAP);
    __uint(max_entries, MAP_MAX_ENTRIES);
    __uint(key_size, sizeof(int));
    __uint(value_size, sizeof(int));
} sk_sockmap SEC(".maps");

struct sock_map_key {
    __u32 remote_ip4;
    __u32 local_ip4;
    __u16 remote_port;
    __u16 local_port;
    __u32  family;
} __attribute__((packed));

struct {
    __uint(type, BPF_MAP_TYPE_HASH);
    __uint(max_entries, MAP_MAX_ENTRIES);
    __type(key, struct sock_map_key);
    __type(value, int);
} sk_index_hash SEC(".maps");


SEC("sk_skb/stream_parser")
int bpf_sk_skb__stream_parser(struct __sk_buff *skb)
{
    //bpf_printk("bpf_sk_skb__stream_parser skb->len:%u, %p\n", skb->len, skb);
    return skb->len;
}

SEC("sk_skb/stream_verdict")
int bpf_sk_skb__stream_verdict(struct __sk_buff *skb)
{
    /*
    bpf_printk("=====================bpf_sk_skb__stream_verdict, %p\n", skb);
    bpf_printk("bpf_sk_skb__stream_verdict remote_ip4 = %d.%d\n", (u8)skb->remote_ip4, (u8)(skb->remote_ip4 >> 8));
    bpf_printk("bpf_sk_skb__stream_verdict remote_ip4 = %d.%d\n", (u8)(skb->remote_ip4 >> 16), skb->remote_ip4 >> 24);
    bpf_printk("bpf_sk_skb__stream_verdict remote_ip4 = %u\n", skb->remote_ip4);
    bpf_printk("bpf_sk_skb__stream_verdict remote_port = %d\n", bpf_ntohl(skb->remote_port));

    bpf_printk("bpf_sk_skb__stream_verdict local_ip4 = %d.%d\n", (u8)skb->local_ip4, (u8)(skb->local_ip4 >> 8));
    bpf_printk("bpf_sk_skb__stream_verdict local_ip4 = %d.%d\n", (u8)(skb->local_ip4 >> 16), skb->local_ip4 >> 24);
    bpf_printk("bpf_sk_skb__stream_verdict local_ip4 = %u\n", skb->local_ip4);
    bpf_printk("bpf_sk_skb__stream_verdict local_port = %d\n", skb->local_port);
     */


    struct sock_map_key map_key = {};
    map_key.remote_ip4 = skb->remote_ip4;
    map_key.local_ip4 = skb->local_ip4;
    map_key.remote_port = (__u16)bpf_ntohl(skb->remote_port);
    map_key.local_port = (__u16)skb->local_port;
    map_key.family = 2;

    int *fd = bpf_map_lookup_elem(&sk_index_hash, &map_key);
    if (fd == NULL) {
        bpf_printk("bpf_sk_skb__stream_verdict fd == NULL, %p\n", skb);
        return 0;
    }

    //bpf_printk("bpf_sk_skb__stream_verdict *fd = %d, %p\n", *fd, skb);
    return bpf_sk_redirect_map(skb, &sk_sockmap, *fd, 0);
}



/********sk_reuseport*******/
#define QUIC_PKT_LONG        0x80  /* header form */
#define QUIC_SERVER_CID_LEN  20
#define ENOENT 2

#define advance_data(nbytes)                                                  \
    offset += nbytes;                                                         \
    if (start + offset > end) {                                               \
        bpf_printk("cannot read %ld bytes at offset %ld", nbytes, offset);      \
        goto failed;                                                          \
    }                                                                         \
    data = start + offset - 1;


#define quic_parse_uint64(p)                                              \
    (((__u64)(p)[0] << 56) |                                                  \
     ((__u64)(p)[1] << 48) |                                                  \
     ((__u64)(p)[2] << 40) |                                                  \
     ((__u64)(p)[3] << 32) |                                                  \
     ((__u64)(p)[4] << 24) |                                                  \
     ((__u64)(p)[5] << 16) |                                                  \
     ((__u64)(p)[6] << 8)  |                                                  \
     ((__u64)(p)[7]))

//struct {
//    __uint(type, BPF_MAP_TYPE_REUSEPORT_SOCKARRAY);
//    __uint(max_entries, 1);
//    __type(key, u64);
//    __type(value, int);
//} quic_sockhash SEC(".maps");

//struct {
//    __uint(type, BPF_MAP_TYPE_ARRAY);
//    __uint(max_entries, 1);
//    __type(key, u64);
//    __type(value, int);
//} quic_sockhash SEC(".maps");

//struct {
//    __uint(type, BPF_MAP_TYPE_HASH);
//    __uint(max_entries, MAP_MAX_ENTRIES);
//    __type(key, u64);
//    __type(value, int);
//} quic_sockhash SEC(".maps");

//struct {
//    __uint(type, BPF_MAP_TYPE_SOCKMAP);
//    __uint(max_entries, MAP_MAX_ENTRIES);
//    __uint(key_size, sizeof(u64));
//    __uint(value_size, sizeof(int));
//} quic_sockhash SEC(".maps");


struct {
    __uint(type, BPF_MAP_TYPE_SOCKHASH);
    __uint(max_entries, MAP_MAX_ENTRIES);
    __uint(key_size, sizeof(u64));
    __uint(value_size, sizeof(int));
} quic_sockhash SEC(".maps");


SEC("sk_reuseport")
int bpf_sk_reuseport(struct sk_reuseport_md *ctx)
{
    int             rc;
    u64           key;
    size_t          len, offset;
    unsigned char  *start, *end, *data, *dcid;

    start = ctx->data;
    end = (unsigned char *) ctx->data_end;
    offset = 0;

    advance_data(sizeof(struct udphdr)); /* data at UDP header */
    advance_data(1); /* data at QUIC flags */

    if (data[0] & QUIC_PKT_LONG) {

        advance_data(4); /* data at QUIC version */
        advance_data(1); /* data at DCID len */

        len = data[0];   /* read DCID length */

        if (len < 8) {
            /* it's useless to search for key in such short DCID */
            return SK_PASS;
        }

    } else {
        len = QUIC_SERVER_CID_LEN;
    }

    dcid = &data[1];
    advance_data(len); /* we expect the packet to have full DCID */

    /* make verifier happy */
    if (dcid + sizeof(u64) > end) {
        goto failed;
    }

    key = quic_parse_uint64(dcid);

    rc = bpf_sk_select_reuseport(ctx, &quic_sockhash, &key, 0);
    switch (rc) {
        case 0:
            bpf_printk("quic socket selected by key 0x%llx", key);
            return SK_PASS;

            /* kernel returns positive error numbers, errno.h defines positive */
        case -ENOENT:
            bpf_printk("quic default route for key 0x%llx", key);
            /* let the default reuseport logic decide which socket to choose */
            return SK_PASS;

        default:
            bpf_printk("quic bpf_sk_select_reuseport err: %d key 0x%llx",
                     rc, key);
            goto failed;
    }

    failed:
    /*
     * SK_DROP will generate ICMP, but we may want to process "invalid" packet
     * in userspace quic to investigate further and finally react properly
     * (maybe ignore, maybe send something in response or close connection)
     */
    return SK_PASS;
}



char LICENSE[] SEC("license") = "Dual BSD/GPL";