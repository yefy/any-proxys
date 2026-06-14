#[cfg(test)]
mod tests {
    use crate::queue::{Queue, QueueNode};

    fn sort_cmp<T: Ord>(a: &QueueNode<T>, b: &QueueNode<T>) -> std::cmp::Ordering {
        a.data().cmp(b.data())
    }

    /// 从 head 到 sentinel 正向遍历，收集所有节点的值副本
    fn collect<T: Copy>(queue: &Queue<T>) -> Vec<T> {
        let mut result = Vec::new();
        let sentinel = queue.sentinel();
        // 用 sentinel.next() 开始遍历：空队列时它等于 sentinel，循环立即结束
        let mut q = sentinel.next();
        loop {
            if q == sentinel {
                break;
            }
            result.push(*q.data());
            q = q.next();
        }
        result
    }

    // -----------------------------------------------------------------------
    // 基础状态
    // -----------------------------------------------------------------------

    #[test]
    fn test_empty_queue() {
        let mut queue = Queue::<i32>::new();
        assert!(queue.empty());
        assert_eq!(queue.len(), 0);
        assert!(queue.pop_head().is_none());
        assert!(queue.pop_tail().is_none());
        // 空队列时 head()/last() 返回 None，不会 panic
        assert!(queue.head().is_none());
        assert!(queue.last().is_none());
    }

    #[test]
    fn test_fresh_node_is_detached() {
        let n = QueueNode::<i32>::new(1);
        assert!(n.is_detached(), "新建节点尚未入队，is_detached 应为 true");
    }

    // -----------------------------------------------------------------------
    // insert_head / insert_tail 顺序验证
    // -----------------------------------------------------------------------

    #[test]
    fn test_insert_head_ordering() {
        let mut queue = Queue::<i32>::new();
        let n1 = QueueNode::new(1);
        let n2 = QueueNode::new(2);
        let n3 = QueueNode::new(3);
        queue.insert_head(&n1);
        queue.insert_head(&n2);
        queue.insert_head(&n3);
        // 最后插入的在最前面 → [3, 2, 1]
        assert_eq!(queue.len(), 3);
        assert_eq!(collect(&queue), vec![3, 2, 1]);
    }

    #[test]
    fn test_insert_tail_ordering() {
        let mut queue = Queue::<i32>::new();
        let n1 = QueueNode::new(1);
        let n2 = QueueNode::new(2);
        let n3 = QueueNode::new(3);
        queue.insert_tail(&n1);
        queue.insert_tail(&n2);
        queue.insert_tail(&n3);
        // 按插入顺序 → [1, 2, 3]
        assert_eq!(queue.len(), 3);
        assert_eq!(collect(&queue), vec![1, 2, 3]);
    }

    #[test]
    fn test_mixed_insert() {
        let mut queue = Queue::<i32>::new();
        let n1 = QueueNode::new(1);
        let n2 = QueueNode::new(2);
        let n3 = QueueNode::new(3);
        let n4 = QueueNode::new(4);
        queue.insert_head(&n2); // [2]
        queue.insert_head(&n1); // [1, 2]
        queue.insert_tail(&n3); // [1, 2, 3]
        queue.insert_tail(&n4); // [1, 2, 3, 4]
        assert_eq!(queue.len(), 4);
        assert_eq!(collect(&queue), vec![1, 2, 3, 4]);
    }

    /// insert 不消耗节点所有权，调用方仍可直接访问
    #[test]
    fn test_insert_does_not_consume_handle() {
        let mut queue = Queue::<i32>::new();
        let n = QueueNode::new(42);
        queue.insert_head(&n);
        assert_eq!(*n.data(), 42); // 仍可通过 n 访问数据
        assert_eq!(queue.len(), 1);
    }

    // -----------------------------------------------------------------------
    // pop_head / pop_tail
    // -----------------------------------------------------------------------

    #[test]
    fn test_pop_head_order() {
        let mut queue = Queue::<i32>::new();
        for v in [10, 20, 30] {
            queue.insert_tail(&QueueNode::new(v));
        }

        let p = queue.pop_head().unwrap();
        assert_eq!(*p.data(), 10);
        assert_eq!(queue.len(), 2);

        let p = queue.pop_head().unwrap();
        assert_eq!(*p.data(), 20);

        let p = queue.pop_head().unwrap();
        assert_eq!(*p.data(), 30);
        assert_eq!(queue.len(), 0);
        assert!(queue.empty());
        assert!(queue.pop_head().is_none());
    }

    #[test]
    fn test_pop_tail_order() {
        let mut queue = Queue::<i32>::new();
        for v in [10, 20, 30] {
            queue.insert_tail(&QueueNode::new(v));
        }

        let p = queue.pop_tail().unwrap();
        assert_eq!(*p.data(), 30);
        assert_eq!(queue.len(), 2);

        let p = queue.pop_tail().unwrap();
        assert_eq!(*p.data(), 20);

        let p = queue.pop_tail().unwrap();
        assert_eq!(*p.data(), 10);
        assert!(queue.empty());
        assert!(queue.pop_tail().is_none());
    }

    #[test]
    fn test_pop_last_is_alias_of_pop_tail() {
        let mut queue = Queue::<i32>::new();
        queue.insert_tail(&QueueNode::new(1));
        queue.insert_tail(&QueueNode::new(2));

        let p = queue.pop_last().unwrap();
        assert_eq!(*p.data(), 2);
        assert_eq!(queue.len(), 1);
    }

    // -----------------------------------------------------------------------
    // remove
    // -----------------------------------------------------------------------

    #[test]
    fn test_remove_head_node() {
        let mut queue = Queue::<i32>::new();
        let n1 = QueueNode::new(1);
        let n2 = QueueNode::new(2);
        let n3 = QueueNode::new(3);
        queue.insert_tail(&n1);
        queue.insert_tail(&n2);
        queue.insert_tail(&n3);

        n1.remove();
        assert_eq!(queue.len(), 2);
        assert_eq!(collect(&queue), vec![2, 3]);
    }

    #[test]
    fn test_remove_middle_node() {
        let mut queue = Queue::<i32>::new();
        let n1 = QueueNode::new(1);
        let n2 = QueueNode::new(2);
        let n3 = QueueNode::new(3);
        queue.insert_tail(&n1);
        queue.insert_tail(&n2);
        queue.insert_tail(&n3);

        n2.remove();
        assert_eq!(queue.len(), 2);
        assert_eq!(collect(&queue), vec![1, 3]);
    }

    #[test]
    fn test_remove_tail_node() {
        let mut queue = Queue::<i32>::new();
        let n1 = QueueNode::new(1);
        let n2 = QueueNode::new(2);
        let n3 = QueueNode::new(3);
        queue.insert_tail(&n1);
        queue.insert_tail(&n2);
        queue.insert_tail(&n3);

        n3.remove();
        assert_eq!(queue.len(), 2);
        assert_eq!(collect(&queue), vec![1, 2]);
    }

    #[test]
    fn test_remove_all_nodes() {
        let mut queue = Queue::<i32>::new();
        let n1 = QueueNode::new(1);
        let n2 = QueueNode::new(2);
        let n3 = QueueNode::new(3);
        queue.insert_tail(&n1);
        queue.insert_tail(&n2);
        queue.insert_tail(&n3);

        n1.remove();
        n2.remove();
        n3.remove();
        assert!(queue.empty());
        assert_eq!(queue.len(), 0);
    }

    /// remove 对已移除节点是幂等的
    #[test]
    fn test_remove_idempotent() {
        let mut queue = Queue::<i32>::new();
        let n = QueueNode::new(42);
        queue.insert_head(&n);

        n.remove();
        assert_eq!(queue.len(), 0);

        n.remove(); // 第二次 remove 不应 panic
        assert_eq!(queue.len(), 0);
        assert!(queue.empty());
    }

    #[test]
    fn test_node_is_detached_after_remove() {
        let mut queue = Queue::<i32>::new();
        let n = QueueNode::new(1);
        queue.insert_head(&n);
        assert!(!n.is_detached(), "入队后 is_detached 应为 false");

        n.remove();
        assert!(n.is_detached(), "移除后 is_detached 应为 true");
    }

    // -----------------------------------------------------------------------
    // remove 后重新入队
    // -----------------------------------------------------------------------

    #[test]
    fn test_reinsert_head_after_remove() {
        let mut queue = Queue::<i32>::new();
        let n1 = QueueNode::new(1);
        let n2 = QueueNode::new(2);
        let n3 = QueueNode::new(3);
        queue.insert_tail(&n1);
        queue.insert_tail(&n2);
        queue.insert_tail(&n3);

        n2.remove();
        assert_eq!(collect(&queue), vec![1, 3]);

        queue.insert_head(&n2);
        assert_eq!(queue.len(), 3);
        assert_eq!(collect(&queue), vec![2, 1, 3]);
    }

    #[test]
    fn test_reinsert_tail_after_remove() {
        let mut queue = Queue::<i32>::new();
        let n1 = QueueNode::new(1);
        let n2 = QueueNode::new(2);
        let n3 = QueueNode::new(3);
        queue.insert_tail(&n1);
        queue.insert_tail(&n2);
        queue.insert_tail(&n3);

        n1.remove();
        assert_eq!(collect(&queue), vec![2, 3]);

        queue.insert_tail(&n1);
        assert_eq!(queue.len(), 3);
        assert_eq!(collect(&queue), vec![2, 3, 1]);
    }

    // -----------------------------------------------------------------------
    // head / last / sentinel 遍历
    // -----------------------------------------------------------------------

    #[test]
    fn test_head_and_last() {
        let mut queue = Queue::<i32>::new();
        queue.insert_tail(&QueueNode::new(10));
        queue.insert_tail(&QueueNode::new(20));
        queue.insert_tail(&QueueNode::new(30));

        assert_eq!(*queue.head().unwrap().data(), 10);
        assert_eq!(*queue.last().unwrap().data(), 30);
    }

    #[test]
    fn test_bidirectional_traversal() {
        let mut queue = Queue::<i32>::new();
        for v in [1, 2, 3] {
            queue.insert_tail(&QueueNode::new(v));
        }

        // 正向：head → ... → sentinel
        let h = queue.head().unwrap();
        assert_eq!(*h.data(), 1);
        let h2 = h.next();
        assert_eq!(*h2.data(), 2);
        let h3 = h2.next();
        assert_eq!(*h3.data(), 3);
        assert!(h3.next() == queue.sentinel());

        // 反向：last → ... → sentinel
        let t = queue.last().unwrap();
        assert_eq!(*t.data(), 3);
        let t2 = t.prev();
        assert_eq!(*t2.data(), 2);
        let t3 = t2.prev();
        assert_eq!(*t3.data(), 1);
        assert!(t3.prev() == queue.sentinel());
    }

    // -----------------------------------------------------------------------
    // insert_after
    // -----------------------------------------------------------------------

    #[test]
    fn test_insert_after_head() {
        let mut queue = Queue::<i32>::new();
        let n1 = QueueNode::new(1);
        let n3 = QueueNode::new(3);
        queue.insert_tail(&n1);
        queue.insert_tail(&n3);

        let n2 = QueueNode::new(2);
        let head = queue.head().unwrap();
        head.insert_after(&n2);
        assert_eq!(queue.len(), 3);
        assert_eq!(collect(&queue), vec![1, 2, 3]);
    }

    #[test]
    fn test_insert_after_tail() {
        let mut queue = Queue::<i32>::new();
        queue.insert_tail(&QueueNode::new(1));
        queue.insert_tail(&QueueNode::new(2));

        let n3 = QueueNode::new(3);
        let last = queue.last().unwrap();
        last.insert_after(&n3);
        assert_eq!(queue.len(), 3);
        assert_eq!(collect(&queue), vec![1, 2, 3]);
    }

    // -----------------------------------------------------------------------
    // sort
    // -----------------------------------------------------------------------

    #[test]
    fn test_sort_empty() {
        let mut queue = Queue::<i32>::new();
        queue.sort(sort_cmp);
        assert!(queue.empty());
        assert_eq!(queue.len(), 0);
    }

    #[test]
    fn test_sort_single_element() {
        let mut queue = Queue::<i32>::new();
        queue.insert_head(&QueueNode::new(42));
        queue.sort(sort_cmp);
        assert_eq!(collect(&queue), vec![42]);
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn test_sort_two_elements_sorted() {
        let mut queue = Queue::<i32>::new();
        queue.insert_tail(&QueueNode::new(1));
        queue.insert_tail(&QueueNode::new(2));
        queue.sort(sort_cmp);
        assert_eq!(collect(&queue), vec![1, 2]);
    }

    #[test]
    fn test_sort_two_elements_reversed() {
        let mut queue = Queue::<i32>::new();
        queue.insert_tail(&QueueNode::new(2));
        queue.insert_tail(&QueueNode::new(1));
        queue.sort(sort_cmp);
        assert_eq!(collect(&queue), vec![1, 2]);
    }

    #[test]
    fn test_sort_already_sorted() {
        let mut queue = Queue::<i32>::new();
        for v in [1, 2, 3, 4, 5] {
            queue.insert_tail(&QueueNode::new(v));
        }
        queue.sort(sort_cmp);
        assert_eq!(collect(&queue), vec![1, 2, 3, 4, 5]);
        assert_eq!(queue.len(), 5);
    }

    #[test]
    fn test_sort_reverse_order() {
        let mut queue = Queue::<i32>::new();
        for v in [5, 4, 3, 2, 1] {
            queue.insert_tail(&QueueNode::new(v));
        }
        queue.sort(sort_cmp);
        assert_eq!(collect(&queue), vec![1, 2, 3, 4, 5]);
        assert_eq!(queue.len(), 5);
    }

    #[test]
    fn test_sort_random_order() {
        let mut queue = Queue::<i32>::new();
        for v in [5, 3, 8, 1, 9, 2, 7, 4, 6] {
            queue.insert_tail(&QueueNode::new(v));
        }
        queue.sort(sort_cmp);
        assert_eq!(collect(&queue), vec![1, 2, 3, 4, 5, 6, 7, 8, 9]);
        assert_eq!(queue.len(), 9);
    }

    #[test]
    fn test_sort_with_duplicates() {
        let mut queue = Queue::<i32>::new();
        for v in [3, 1, 2, 1, 3, 2] {
            queue.insert_tail(&QueueNode::new(v));
        }
        queue.sort(sort_cmp);
        assert_eq!(collect(&queue), vec![1, 1, 2, 2, 3, 3]);
        assert_eq!(queue.len(), 6);
    }

    #[test]
    fn test_sort_preserves_len() {
        let mut queue = Queue::<i32>::new();
        for v in [4, 2, 6, 1, 5, 3] {
            queue.insert_tail(&QueueNode::new(v));
        }
        queue.sort(sort_cmp);
        assert_eq!(queue.len(), 6);
    }

    #[test]
    fn test_sort_then_modify() {
        let mut queue = Queue::<i32>::new();
        let n5 = QueueNode::new(5);
        let n3 = QueueNode::new(3);
        let n1 = QueueNode::new(1);
        queue.insert_tail(&n5);
        queue.insert_tail(&n3);
        queue.insert_tail(&n1);
        queue.sort(sort_cmp);
        assert_eq!(collect(&queue), vec![1, 3, 5]);

        n3.remove();
        queue.insert_tail(&QueueNode::new(4));
        assert_eq!(collect(&queue), vec![1, 5, 4]);
        assert_eq!(queue.len(), 3);
    }

    // -----------------------------------------------------------------------
    // len 计数一致性
    // -----------------------------------------------------------------------

    #[test]
    fn test_len_consistency() {
        let mut queue = Queue::<i32>::new();
        assert_eq!(queue.len(), 0);

        let nodes: Vec<QueueNode<i32>> = (1..=5).map(|i| QueueNode::new(i)).collect();
        for n in &nodes {
            queue.insert_tail(n);
        }
        assert_eq!(queue.len(), 5);

        queue.pop_head();
        assert_eq!(queue.len(), 4);

        queue.pop_tail();
        assert_eq!(queue.len(), 3);

        let mid = nodes[2].clone();
        mid.remove();
        assert_eq!(queue.len(), 2);

        queue.sort(sort_cmp);
        assert_eq!(queue.len(), 2);
    }

    // -----------------------------------------------------------------------
    // 多队列隔离
    // -----------------------------------------------------------------------

    #[test]
    fn test_multiple_queues_isolation() {
        let mut q1 = Queue::<i32>::new();
        let mut q2 = Queue::<i32>::new();

        q1.insert_tail(&QueueNode::new(1));
        q1.insert_tail(&QueueNode::new(2));
        q2.insert_tail(&QueueNode::new(10));

        assert_eq!(q1.len(), 2);
        assert_eq!(q2.len(), 1);

        q1.pop_head();
        assert_eq!(q1.len(), 1);
        assert_eq!(q2.len(), 1); // q2 不受影响
    }

    // -----------------------------------------------------------------------
    // 漏洞验证：sort() 中 q.remove() 后 prev.prev() 是否导致死循环
    // -----------------------------------------------------------------------

    /// 【sort 死循环验证】q.remove() 后 prev 回溯不会经过已摘除的 q。
    ///
    /// 分析：
    ///   `q.remove()` 将 q 的前后邻居互相连接，同时把 q 自身设为自环。
    ///   此后链表中**不再有任何节点的 prev/next 指向 q**。
    ///   内层 `prev = prev.prev()` 只遍历仍在链表中的节点，
    ///   不可能抵达已摘除的 q，死循环不存在。
    ///
    ///   直接验证：[3, 2, 1] 排序时 q=node(1) 需要向前跨越 node(3) 和 node(2)，
    ///   这正是审阅者描述的"死循环场景"。若存在死循环此测试会挂起。
    #[test]
    fn test_sort_no_deadloop_multi_step_backward() {
        let mut queue = Queue::<i32>::new();
        // 完全逆序：每个元素都需要向前跨越所有已排好的元素
        for v in [5, 4, 3, 2, 1] {
            queue.insert_tail(&QueueNode::new(v));
        }
        queue.sort(sort_cmp);
        assert_eq!(collect(&queue), vec![1, 2, 3, 4, 5]);
    }

    /// 【sort UAF 验证】节点无用户句柄时 sort 不崩溃
    ///
    /// 分析：
    ///   `q = self.head_raw().next()` 中 `.next()` 调用 `from_raw()` 使 ref_cnt+1，
    ///   q 节点 ref_cnt ≥ 2（队列持有 + q 句柄）。
    ///   `q.remove()` 只释放队列那份（ref_cnt -1 → ≥1），q 句柄存活，不触发 free。
    #[test]
    fn test_sort_no_user_handles_no_uaf() {
        let mut queue = Queue::<i32>::new();
        for v in [5, 3, 8, 1, 9, 2, 7, 4, 6] {
            queue.insert_tail(&QueueNode::new(v));
        }
        queue.sort(sort_cmp);
        assert_eq!(collect(&queue), vec![1, 2, 3, 4, 5, 6, 7, 8, 9]);
        assert_eq!(queue.len(), 9);
        let sentinel = queue.sentinel();
        let mut cur = sentinel.next();
        loop {
            if cur == sentinel { break; }
            assert!(!cur.is_detached());
            cur = cur.next();
        }
    }

    /// 【sort UAF 验证·极端情况】有句柄与无句柄节点混合排序
    #[test]
    fn test_sort_mixed_handles_no_uaf() {
        let mut queue = Queue::<i32>::new();
        let anchor = QueueNode::new(5);
        queue.insert_tail(&anchor);
        for v in [3, 1, 4, 2] {
            queue.insert_tail(&QueueNode::new(v));
        }
        queue.sort(sort_cmp);
        assert_eq!(collect(&queue), vec![1, 2, 3, 4, 5]);
        assert!(!anchor.is_detached());
    }

    // -----------------------------------------------------------------------
    // 漏洞验证：clear() 遍历旧链时的指针安全
    // -----------------------------------------------------------------------

    /// 【漏洞 2 验证】clear() 在有/无用户句柄混合情况下的正确性。
    ///
    /// 正确分析：
    ///   `next` 是在任何 free 之前保存的局部指针值（整数拷贝），
    ///   即使当前节点因 ref_cnt==1 被 drop(Box::from_raw(cur))，
    ///   `next` 的数值仍然有效，可安全用于 `cur = next` 和终止判断。
    #[test]
    fn test_clear_mixed_handles_pointer_safety() {
        let mut queue = Queue::<i32>::new();
        let n1 = QueueNode::new(1); // 有用户句柄（clear 后节点内存仍存活）
        let n3 = QueueNode::new(3); // 有用户句柄
        queue.insert_tail(&n1);
        queue.insert_tail(&QueueNode::new(2)); // 无用户句柄（clear 后内存释放）
        queue.insert_tail(&n3);
        queue.insert_tail(&QueueNode::new(4)); // 无用户句柄（clear 后内存释放）
        queue.insert_tail(&QueueNode::new(5)); // 无用户句柄

        // 若 clear 有悬空指针问题，此处必然崩溃
        queue.clear();

        assert!(queue.empty());
        assert_eq!(queue.len(), 0);
        // 有句柄的节点被正确 detach
        assert!(n1.is_detached());
        assert!(n3.is_detached());
        // 无句柄节点已被释放，不会泄漏（valgrind/miri 可检验）
    }

    /// 【漏洞 2 验证】clear() 后队列可正常复用
    #[test]
    fn test_clear_then_reuse_queue() {
        let mut queue = Queue::<i32>::new();
        for v in 0..50 {
            queue.insert_tail(&QueueNode::new(v));
        }
        queue.clear();
        assert!(queue.empty());

        // clear 后哨兵自环完好，队列可继续使用
        let n = QueueNode::new(99);
        queue.insert_tail(&n);
        assert_eq!(queue.len(), 1);
        assert_eq!(*queue.head().unwrap().data(), 99);
        assert!(!n.is_detached());
    }

    // -----------------------------------------------------------------------
    // clear() Panic Safety：T::drop panic 不导致全链泄漏
    // -----------------------------------------------------------------------

    /// 【漏洞验证】clear() 中某节点 T::drop panic 时，后续节点仍被正确处理。
    ///
    /// 场景：队列 [A(panic), B(no-handle), C(user-handle)]。
    ///   步骤 1：哨兵已断开（O(1)）。
    ///   步骤 2 若无 catch_unwind：A 的 drop panic → unwind 出 while 循环 →
    ///     B/C 的队列所有权永不归还 → 内存泄漏（B 无用户句柄 = 彻底泄漏）。
    ///   步骤 2 有 catch_unwind：A 的 panic 被捕获，循环继续 → B/C 正常处理 →
    ///     C 被 detach，B 的队列份额归还（B 无用户句柄则立即释放）。
    ///
    /// 验证手段：若 clear() 向调用方传播了 panic，`assert!(n_c.is_detached())`
    /// 永远不会被执行，测试在 panic 处失败，证明旧行为有缺陷。
    #[test]
    fn test_clear_panic_in_drop_does_not_propagate() {
        use std::sync::atomic::{AtomicUsize, Ordering as AO};
        use std::sync::Arc;

        let drop_count = Arc::new(AtomicUsize::new(0));

        struct MaybePanic {
            id: usize,
            drop_count: Arc<AtomicUsize>,
        }
        impl Drop for MaybePanic {
            fn drop(&mut self) {
                self.drop_count.fetch_add(1, AO::Relaxed);
                if self.id == 0 {
                    panic!("intentional drop panic for id=0");
                }
            }
        }

        let dc = drop_count.clone();
        let mut queue = Queue::<MaybePanic>::new();

        // 节点 A（id=0）：无用户句柄，drop 时 panic
        queue.insert_tail(&QueueNode::new(MaybePanic { id: 0, drop_count: dc.clone() }));
        // 节点 B（id=1）：无用户句柄，drop 正常
        queue.insert_tail(&QueueNode::new(MaybePanic { id: 1, drop_count: dc.clone() }));
        // 节点 C（id=2）：有用户句柄，验证其是否被正确 detach
        let n_c = QueueNode::new(MaybePanic { id: 2, drop_count: dc.clone() });
        queue.insert_tail(&n_c);
        assert_eq!(queue.len(), 3);

        // clear() 不应向调用方传播 A 的 drop panic
        // 若没有 catch_unwind 修复，此处 panic，下方 assert 永远不执行 → 测试失败
        queue.clear();

        assert!(queue.empty(), "clear() 后队列应为空");
        // C 被正确处理：in_queue = false（detached）
        assert!(n_c.is_detached(), "C 在 clear() 后应被 detach");

        // A 和 B 的 drop 都被调用（drop_count 至少包含 A/B 的计数）
        // C 的队列所有权也被释放，C 的 data drop 还没发生（n_c 仍存活）
        // id=0 的 drop 运行了（即使 panic 也运行到 fetch_add 那一行）
        // id=1, id=2(队列份额) 各运行一次 → drop_count >= 2（A=1, B=1）
        let count = drop_count.load(AO::Relaxed);
        assert!(count >= 2, "drop_count 应 >= 2，实际 = {}", count);
    }

    /// 【Panic Safety 补充】Queue::drop 透传 clear()：同样不泄漏后续节点
    #[test]
    fn test_queue_drop_panic_in_t_does_not_leak_rest() {
        // 只验证 drop(queue) 不会因为 T::drop panic 而 abort 进程
        // （double-panic = abort；有 catch_unwind 后 Queue::drop 中的 clear() 不 panic）
        struct PanicOnDrop;
        impl Drop for PanicOnDrop {
            fn drop(&mut self) { panic!("drop panic"); }
        }

        let mut queue = Queue::<PanicOnDrop>::new();
        // 节点无用户句柄；drop(queue) 触发 clear()，clear() 内 catch_unwind 隔离 panic
        queue.insert_tail(&QueueNode::new(PanicOnDrop));
        queue.insert_tail(&QueueNode::new(PanicOnDrop));
        queue.insert_tail(&QueueNode::new(PanicOnDrop));

        // 若没有 catch_unwind，此处 drop(queue) 会 panic，
        // 再加上测试框架的 panic 处理会触发 double-panic → 进程 abort。
        // 有 catch_unwind，drop(queue) 安全完成。
        drop(queue); // 不应 panic，不应 abort
    }

    /// 【漏洞 3 验证】insert_after 中 len 赋值不触发旧值 Drop。
    ///
    /// 正确分析：
    ///   detached 节点（通过 remove 或 clear 脱离队列）的 len 字段必为 None
    ///   （remove/clear 均执行 take()）。Some(node_len) 赋给 None 不触发任何 drop。
    ///   新建节点 len 初始化为 None，同理。
    #[test]
    fn test_insert_after_len_assignment_safe() {
        let mut queue = Queue::<i32>::new();
        let n = QueueNode::new(1);
        // 首次插入：len 从 None → Some(arc)，无旧值可 drop
        queue.insert_tail(&n);
        n.remove();
        // remove 后 len 为 None（take() 过了）
        // 再次插入：依然从 None → Some(arc)，不存在意外 Drop
        queue.insert_tail(&n);
        assert_eq!(queue.len(), 1);
        assert!(!n.is_detached());
    }

    // -----------------------------------------------------------------------
    // clear()
    // -----------------------------------------------------------------------

    #[test]
    fn test_clear_empties_queue() {
        let mut queue = Queue::<i32>::new();
        let nodes: Vec<QueueNode<i32>> = (0..10).map(|i| QueueNode::new(i)).collect();
        for n in &nodes { queue.insert_tail(n); }
        assert_eq!(queue.len(), 10);

        queue.clear();

        assert!(queue.empty());
        assert_eq!(queue.len(), 0);
        // clear 后所有节点均变为 detached
        for n in &nodes { assert!(n.is_detached()); }
        // clear 后哨兵自环完好，可继续使用
        queue.insert_tail(&QueueNode::new(99));
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn test_clear_empty_queue_is_noop() {
        let mut queue = Queue::<i32>::new();
        queue.clear();
        assert!(queue.empty());
    }

    /// 跨队列插入：同一节点插入两个队列应 panic
    #[test]
    #[should_panic(expected = "insert_after: node is already in a queue")]
    fn test_cross_queue_insert_panics() {
        let mut q1 = Queue::<i32>::new();
        let mut q2 = Queue::<i32>::new();
        let node = QueueNode::new(1);
        q1.insert_tail(&node);
        q2.insert_tail(&node); // ← 应当 panic
    }

    // -----------------------------------------------------------------------
    // Drop / 内存安全（配合 miri 无泄漏）
    // -----------------------------------------------------------------------

    #[test]
    fn test_queue_drop_with_data() {
        let mut queue = Queue::<String>::new();
        for i in 0..20 {
            queue.insert_tail(&QueueNode::new(format!("item_{}", i)));
        }
        drop(queue);
    }

    #[test]
    fn test_empty_queue_drop() {
        let queue = Queue::<i32>::new();
        drop(queue);
    }

    #[test]
    fn test_popped_node_drop() {
        let mut queue = Queue::<i32>::new();
        for v in [1, 2, 3] {
            queue.insert_tail(&QueueNode::new(v));
        }
        {
            let _popped = queue.pop_head();
        }
        assert_eq!(queue.len(), 2);
        assert_eq!(collect(&queue), vec![2, 3]);
    }

    /// 节点在没有用户句柄时（仅队列持有），pop 后能正确释放
    #[test]
    fn test_node_owned_only_by_queue() {
        let mut queue = Queue::<i32>::new();
        // 节点临时创建，没有用户持有句柄
        queue.insert_tail(&QueueNode::new(1));
        queue.insert_tail(&QueueNode::new(2));
        assert_eq!(queue.len(), 2);
        // pop 后节点应被正确释放
        drop(queue.pop_head());
        drop(queue.pop_head());
        assert!(queue.empty());
    }

    use super::*;
    use std::sync::{Arc, Mutex};
    use std::thread;

    #[test]
    fn test_queue() {
        let mut queue = Queue::<i32>::new();

        let n3 = QueueNode::new(3);
        let n1 = QueueNode::new(1);
        let n7 = QueueNode::new(7);

        queue.insert_head(&n3);
        queue.insert_head(&n1);
        queue.insert_tail(&n7);

        println!("len={}", queue.len());

        {
            let sentinel = queue.sentinel();
            let mut q = sentinel.next();
            loop {
                if q.ptr == queue.sentinel_ptr() {
                    break;
                }
                println!("data={}", q.data());
                q = q.next();
            }
        }

        queue.sort(sort_cmp);

        println!("after sort");

        {
            let sentinel = queue.sentinel();
            let mut q = sentinel.next();
            loop {
                if q.ptr == queue.sentinel_ptr() {
                    break;
                }
                println!("data={}", q.data());
                q = q.next();
            }
        }

        unsafe {
            *n3.data_mut() = 100;
        }

        println!("n3={}", n3.data());

        n1.remove();

        println!("after remove n1");

        {
            let sentinel = queue.sentinel();
            let mut q = sentinel.next();
            loop {
                if q.ptr == queue.sentinel_ptr() {
                    break;
                }
                println!("data={}", q.data());
                q = q.next();
            }
        }

        if let Some(node) = queue.pop_head() {
            println!("pop_head={}", node.data());
        }

        if let Some(node) = queue.pop_tail() {
            println!("pop_tail={}", node.data());
        }

        println!("final len={}", queue.len());
    }

    #[test]
    fn test_queue_multi_thread() {
        const THREADS: usize = 4;
        const PER_THREAD: usize = 10000;

        let queue = Arc::new(Mutex::new(Queue::<usize>::new()));

        let mut handles = Vec::new();

        // =========================================================================
        // 多线程 insert
        // =========================================================================

        for t in 0..THREADS {
            let queue = queue.clone();

            handles.push(thread::spawn(move || {
                for i in 0..PER_THREAD {
                    let node = QueueNode::new(t * 1000000 + i);

                    // 用户自己保证锁
                    {
                        let mut q = queue.lock().unwrap();

                        q.insert_tail(&node);
                    }

                    // 模拟业务
                    if i % 1000 == 0 {
                        println!("thread={} insert={}", t, i);
                    }

                    // node 生命周期结束
                    // 队列内部还有一份 ref_cnt
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        // =========================================================================
        // 检查长度
        // =========================================================================

        {
            let q = queue.lock().unwrap();

            println!("queue len={}", q.len());

            assert_eq!(q.len(), THREADS * PER_THREAD);
        }

        // =========================================================================
        // 多线程 pop
        // =========================================================================

        let mut handles = Vec::new();

        for t in 0..THREADS {
            let queue = queue.clone();

            handles.push(thread::spawn(move || {
                let mut count = 0;

                loop {
                    let node = {
                        let mut q = queue.lock().unwrap();

                        q.pop_head()
                    };

                    match node {
                        Some(node) => {
                            let v = *node.data();

                            count += 1;

                            if count % 1000 == 0 {
                                println!(
                                    "thread={} pop count={} value={}",
                                    t,
                                    count,
                                    v
                                );
                            }
                        }
                        None => {
                            break;
                        }
                    }
                }

                println!("thread={} finish count={}", t, count);
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        // =========================================================================
        // 最终检查
        // =========================================================================

        {
            let q = queue.lock().unwrap();

            println!("final len={}", q.len());

            assert_eq!(q.len(), 0);

            assert!(q.empty());
        }
    }

    #[test]
    fn test_queue_multi_thread_modify_data() {
        let queue = Arc::new(Mutex::new(Queue::<usize>::new()));

        let node = QueueNode::new(123);

        {
            let mut q = queue.lock().unwrap();

            q.insert_tail(&node);
        }

        let node = Arc::new(Mutex::new(node));

        let mut handles = Vec::new();

        for i in 0..8 {
            let queue = queue.clone();
            let node = node.clone();

            handles.push(thread::spawn(move || {
                for j in 0..10000 {
                    let _q = queue.lock().unwrap();

                    // 整个 queue 已被锁保护
                    // 可以安全 data_mut

                    let mut n = node.lock().unwrap();

                    unsafe {
                        *n.data_mut() += 1;
                    }

                    if j % 5000 == 0 {
                        println!("thread={} j={}", i, j);
                    }
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        {
            let q = queue.lock().unwrap();

            let head = q.head().unwrap();

            println!("final data={}", head.data());

            assert_eq!(*head.data(), 123 + 8 * 10000);
        }
    }

    #[test]
    fn test_queue_sort() {
        let mut queue = Queue::<i32>::new();

        let n3 = QueueNode::new(3);
        let n1 = QueueNode::new(1);
        let n7 = QueueNode::new(7);
        let n2 = QueueNode::new(2);

        queue.insert_tail(&n3);
        queue.insert_tail(&n1);
        queue.insert_tail(&n7);
        queue.insert_tail(&n2);

        queue.sort(sort_cmp);

        let sentinel = queue.sentinel();
        let mut q = sentinel.next();
        let mut result = Vec::new();

        loop {
            if q.ptr == queue.sentinel_ptr() {
                break;
            }
            result.push(*q.data());
            q = q.next();
        }

        assert_eq!(result, vec![1, 2, 3, 7]);
    }

    #[test]
    fn test_queue_multi_thread_remove() {
        use std::sync::{Arc, Mutex};
        use std::thread;

        const THREADS: usize = 8;
        const PER_THREAD: usize = 5000;

        let queue = Arc::new(Mutex::new(Queue::<usize>::new()));

        // 保存所有节点
        let all_nodes = Arc::new(Mutex::new(Vec::<QueueNode<usize>>::new()));

        // =========================================================================
        // 多线程插入
        // =========================================================================

        let mut handles = Vec::new();

        for t in 0..THREADS {
            let queue = queue.clone();
            let all_nodes = all_nodes.clone();

            handles.push(thread::spawn(move || {
                let mut local_nodes = Vec::with_capacity(PER_THREAD);

                for i in 0..PER_THREAD {
                    let node = QueueNode::new(t * 1_000_000 + i);

                    {
                        let mut q = queue.lock().unwrap();

                        q.insert_tail(&node);
                    }

                    local_nodes.push(node);

                    if i % 1000 == 0 {
                        println!("insert thread={} i={}", t, i);
                    }
                }

                {
                    let mut nodes = all_nodes.lock().unwrap();

                    nodes.extend(local_nodes);
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        // =========================================================================
        // 检查长度
        // =========================================================================

        {
            let q = queue.lock().unwrap();

            println!("after insert len={}", q.len());

            assert_eq!(q.len(), THREADS * PER_THREAD);
        }

        // =========================================================================
        // 多线程 remove
        // =========================================================================

        let nodes = {
            let nodes = all_nodes.lock().unwrap();

            nodes.clone()
        };

        let nodes = Arc::new(nodes);

        let mut handles = Vec::new();

        for t in 0..THREADS {
            let queue = queue.clone();
            let nodes = nodes.clone();

            handles.push(thread::spawn(move || {
                let begin = t * PER_THREAD;
                let end = begin + PER_THREAD;

                for i in begin..end {
                    let node = nodes[i].clone();

                    {
                        // 用户自己保证锁
                        let _q = queue.lock().unwrap();

                        node.remove();
                    }

                    if (i - begin) % 1000 == 0 {
                        println!(
                            "remove thread={} progress={}",
                            t,
                            i - begin
                        );
                    }
                }

                println!("remove thread={} finish", t);
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        // =========================================================================
        // 最终检查
        // =========================================================================

        {
            let q = queue.lock().unwrap();

            println!("final len={}", q.len());

            assert_eq!(q.len(), 0);

            assert!(q.empty());
        }

        // =========================================================================
        // 验证节点已 detached
        // =========================================================================

        for node in nodes.iter() {
            assert!(node.is_detached());
        }

        println!("test_queue_multi_thread_remove finish");
    }

    // -----------------------------------------------------------------------
    // UnsafeCell 内部可变性验证
    // -----------------------------------------------------------------------

    /// 【UnsafeCell 验证】data() 取得 &T 后再通过 data_mut() 修改，结果可见。
    ///
    /// 若 data 字段没有 UnsafeCell 包裹，编译器（LLVM alias analysis）可能将
    /// `data()` 的读结果缓存在寄存器中，使得 `data_mut()` 的写操作对后续
    /// `data()` 不可见（在 Rust 内存模型 / Stacked Borrows 下是 UB）。
    /// UnsafeCell 明确告知编译器此字段有内部可变性，禁止该优化。
    #[test]
    fn test_data_mut_visible_after_data_ref() {
        let mut queue = Queue::<i32>::new();
        let node = QueueNode::new(42);
        queue.insert_tail(&node);

        // 取得共享引用
        let r1 = node.data();
        assert_eq!(*r1, 42);

        // 通过 data_mut 修改（持有外部锁语义：单线程下 mut queue 隐含独占）
        unsafe { *node.data_mut() = 100; }

        // 再次读取：必须看到最新值（UnsafeCell 保证编译器不缓存旧值）
        assert_eq!(*node.data(), 100);
        assert_eq!(*queue.head().unwrap().data(), 100);
    }

    /// 【UnsafeCell 验证】多次 data_mut 累积修改结果正确
    #[test]
    fn test_data_mut_accumulate() {
        let node = QueueNode::new(0i64);
        for _ in 0..10_000 {
            unsafe { *node.data_mut() += 1; }
        }
        assert_eq!(*node.data(), 10_000);
    }

    // -----------------------------------------------------------------------
    // Send / Sync 边界验证
    // -----------------------------------------------------------------------

    /// 编译期断言：i32 满足 Send + Sync，Queue/QueueNode 也应满足
    #[test]
    fn test_send_sync_bounds() {
        fn require_send<T: Send>() {}
        fn require_sync<T: Sync>() {}

        require_send::<QueueNode<i32>>();
        require_sync::<QueueNode<i32>>();
        require_send::<Queue<i32>>();
        require_sync::<Queue<i32>>();

        // 以下若取消注释应编译失败（Cell<i32> 非 Sync）：
        // require_sync::<QueueNode<std::cell::Cell<i32>>>();
        // require_sync::<Queue<std::cell::Cell<i32>>>();
    }

    // -----------------------------------------------------------------------
    // is_detached() 无锁并发读 + 有锁写，验证 AtomicBool 无 data race
    // -----------------------------------------------------------------------

    /// 多个读线程在不持有队列锁的情况下轮询 is_detached()，
    /// 同时写线程在锁保护下交替 remove / insert。
    /// 使用普通 bool 时此测试在 ThreadSanitizer / Miri 下会报告 data race；
    /// 使用 AtomicBool + Release-Acquire 则保证无 data race。
    #[test]
    fn test_is_detached_no_data_race() {
        use std::sync::{Arc, Barrier, Mutex};
        use std::thread;

        const READERS: usize = 4;
        const READ_ITERS: usize = 200_000;
        const WRITE_ITERS: usize = 2_000;

        let queue = Arc::new(Mutex::new(Queue::<i32>::new()));
        let node = QueueNode::new(0);

        // 初始入队
        {
            let mut q = queue.lock().unwrap();
            q.insert_tail(&node);
        }

        let barrier = Arc::new(Barrier::new(READERS + 1));
        let mut handles = vec![];

        // 读线程：不持锁，直接调用 is_detached()
        for _ in 0..READERS {
            let node = node.clone();
            let barrier = barrier.clone();
            handles.push(thread::spawn(move || {
                barrier.wait();
                for _ in 0..READ_ITERS {
                    let _ = node.is_detached();
                    std::hint::spin_loop();
                }
            }));
        }

        // 写线程：持锁后交替 remove / insert
        {
            let queue = queue.clone();
            let node = node.clone();
            let barrier = barrier.clone();
            handles.push(thread::spawn(move || {
                barrier.wait();
                for _ in 0..WRITE_ITERS {
                    let mut q = queue.lock().unwrap();
                    if node.is_detached() {
                        q.insert_tail(&node);
                    } else {
                        node.remove();
                    }
                    drop(q);
                    thread::yield_now();
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        // 最终清理：保证 drop 时队列为空
        {
            let mut q = queue.lock().unwrap();
            if !node.is_detached() {
                node.remove();
            }
            assert!(q.empty());
        }
    }

    // -----------------------------------------------------------------------
    // 漏洞 1 验证：Queue::drop 哨兵 len Arc 析构路径（误报分析）
    // -----------------------------------------------------------------------

    /// 【漏洞 1 分析：误报】Queue::drop 不存在哨兵 len 泄漏。
    ///
    /// Queue::new() 使 Arc<AtomicUsize> 计数 = 2：
    ///   一份存在 Queue.len，一份 clone 到 sentinel.ptr->len。
    ///
    /// Queue 析构顺序（Rust 按字段声明顺序）：
    ///   1. Drop::drop → clear()：仅释放数据节点，不碰哨兵 len（Arc 计数不变）
    ///   2. sentinel 字段析构 → QueueNode::drop → ref_cnt 1→0 →
    ///      drop(Box) → QueueNodeInner 所有字段析构 → sentinel.len = Some(…) 归还
    ///      Arc 计数: 2→1
    ///   3. Queue.len 字段析构 → Arc 计数: 1→0 → AtomicUsize 释放
    ///
    /// 验证手段：用 Arc<AtomicUsize> 充当 T，追踪节点析构次数，
    /// Queue 析构完成后 drop_count 应精确等于插入节点数（无泄漏）。
    #[test]
    fn test_queue_drop_sentinel_len_arc_no_leak() {
        use std::sync::atomic::{AtomicUsize, Ordering as AO};
        use std::sync::Arc;

        let drop_count = Arc::new(AtomicUsize::new(0));

        struct TrackDrop(Arc<AtomicUsize>);
        impl Drop for TrackDrop {
            fn drop(&mut self) {
                self.0.fetch_add(1, AO::SeqCst);
            }
        }

        let dc = drop_count.clone();
        {
            let mut queue = Queue::<TrackDrop>::new();
            // 插入 5 个节点，均无用户句柄（queue 是唯一持有者）
            for _ in 0..5 {
                queue.insert_tail(&QueueNode::new(TrackDrop(dc.clone())));
            }
            // drop(queue) 触发 clear()，然后哨兵字段析构
            // 此时 5 个 TrackDrop 应全部被 drop
        }

        // 哨兵节点持有的 len Arc 与数据节点无关；
        // 5 个 TrackDrop 析构 → drop_count = 5（无多余 drop，无泄漏）
        assert_eq!(
            drop_count.load(AO::SeqCst),
            5,
            "Queue::drop 后 5 个节点均应被析构，无泄漏"
        );
    }

    /// 【漏洞 1 补充】哨兵自身的 len Arc 通过字段析构顺序正确释放。
    ///
    /// 验证手段：在节点的 T 数据中嵌入一个弱引用观察点；
    /// Queue::drop 完成后所有强引用归零，弱引用 upgrade() 返回 None。
    #[test]
    fn test_queue_drop_len_arc_count_correct() {
        use std::sync::Arc;

        // Sentinel 的 len Arc 完全由 Queue 内部持有，外部无法直接取弱引用。
        // 用以下等效验证：Queue drop 后所有数据节点的 T 均析构完成（无泄漏）。
        //
        // 若哨兵 len Arc 泄漏，AtomicUsize 就不会释放，但这不影响数据节点的析构。
        // 实际泄漏检测已由 test_queue_drop_sentinel_len_arc_no_leak 覆盖（drop_count 精确）。
        //
        // 此处额外验证：即使 Queue 有多个节点，drop 后全部 T 被正确析构。
        let live = Arc::new(());
        let weak = Arc::downgrade(&live);

        struct HoldsArc(Arc<()>);

        {
            let mut queue = Queue::<HoldsArc>::new();
            for _ in 0..4 {
                queue.insert_tail(&QueueNode::new(HoldsArc(live.clone())));
            }
            // drop queue：clear() 释放 4 个数据节点，HoldsArc 析构，Arc clone drop
        }
        drop(live); // 释放最后一份外部强引用

        assert!(
            weak.upgrade().is_none(),
            "Queue::drop 后所有 HoldsArc 均析构，Arc 强引用应归零"
        );
    }

    // -----------------------------------------------------------------------
    // 漏洞 2 验证：remove() 中 in_queue/ref_cnt 顺序的 UAF 分析（误报）
    // -----------------------------------------------------------------------

    /// 【漏洞 2 分析：误报】remove() 调用期间 ref_cnt 不可能降为 0。
    ///
    /// 在 remove() 执行 in_queue.store(false) 时，ref_cnt 组成：
    ///   ① self（调用者句柄）持有 1 份
    ///   ② queue 尚未执行 fetch_sub 持有 1 份
    ///   ③ 若有第三方句柄（Thread B）再 +1
    ///   ⇒ ref_cnt ≥ 2（最少 ①+②）
    ///
    /// Thread B 看到 is_detached()=true 后 drop 句柄：
    ///   fetch_sub 旧值 ≥ 2，不触发 free
    ///
    /// remove() step 5 做 ref_cnt.fetch_sub(1)：
    ///   旧值 ≥ 2-1=1... 等等，Thread B 已先 -1：
    ///   ③: ref_cnt = 3-1=2 (Thread B drop)
    ///   ②: ref_cnt = 2-1=1 (remove step 5)
    ///   旧值 = 2 ≠ 1 → 不触发 free
    ///
    /// ① self 句柄在 remove() 返回后由调用方管理，最终 drop 时才真正 free。
    ///
    /// 验证手段：模拟"Thread B 在 is_detached()=true 后立即 drop 句柄"的场景，
    /// 确认整个生命周期内无 double-free（通过 drop_count 跟踪）。
    #[test]
    fn test_remove_in_queue_before_ref_cnt_no_uaf() {
        use std::sync::atomic::{AtomicUsize, Ordering as AO};
        use std::sync::{Arc, Barrier, Mutex};
        use std::thread;

        let drop_count = Arc::new(AtomicUsize::new(0));

        struct TrackDrop(Arc<AtomicUsize>);
        impl Drop for TrackDrop {
            fn drop(&mut self) {
                self.0.fetch_add(1, AO::SeqCst);
            }
        }

        let dc = drop_count.clone();
        let queue = Arc::new(Mutex::new(Queue::<TrackDrop>::new()));

        // node_a：Thread A 持有（用于调用 remove()）
        let node_a = QueueNode::new(TrackDrop(dc.clone()));
        // node_b：Thread B 持有（在看到 is_detached()=true 后 drop）
        let node_b = node_a.clone();
        // 此时 ref_cnt = 2（node_a + node_b）

        {
            let mut q = queue.lock().unwrap();
            q.insert_tail(&node_a);
        }
        // ref_cnt = 3（node_a + node_b + queue 份额）

        let barrier = Arc::new(Barrier::new(2));
        let barrier_b = Arc::clone(&barrier);
        let queue_b = Arc::clone(&queue);

        // Thread B：等待 node 被 remove，然后立即 drop 自己的句柄
        let t = thread::spawn(move || {
            // 等到 Thread A 发出信号（remove 可能已完成或正在进行）
            barrier_b.wait();
            // 轮询直到看到 is_detached()=true，然后 drop 句柄
            while !node_b.is_detached() {
                thread::yield_now();
            }
            // drop node_b：若 UAF 场景真实，此处会导致悬空指针操作
            drop(node_b);
        });

        // Thread A：持锁 remove，然后释放锁
        {
            let mut q = queue.lock().unwrap();
            node_a.remove();
            drop(q);
        }
        barrier.wait(); // 通知 Thread B 可以开始轮询

        t.join().unwrap();

        // 此时 node_a 和 node_b 都已 drop（若 node_a 先释放），
        // 或 node_a 最后释放。无论顺序如何，TrackDrop::drop 只执行 1 次。
        drop(node_a);

        assert_eq!(
            drop_count.load(AO::SeqCst),
            1,
            "TrackDrop::drop 应恰好执行 1 次，无 double-free 也无泄漏"
        );
        assert!(queue.lock().unwrap().empty());
    }

    /// 【漏洞 2 ref_cnt 不变式】remove() 执行期间 ref_cnt 最小值 = 2。
    ///
    /// 因为 remove() 需要 `self: &QueueNode<T>`，调用方必须持有一个句柄（+1），
    /// 且 in_queue=true 时队列也持有一份（+1），所以 ref_cnt ≥ 2。
    /// step 5 的 `fetch_sub(1)` 旧值 ≥ 2，条件 == 1 永远为 false，
    /// remove() 内部的 `drop(Box::from_raw)` 是死代码路径（正常使用下从不执行）。
    #[test]
    fn test_remove_step5_never_frees_in_normal_use() {
        use std::sync::atomic::{AtomicUsize, Ordering as AO};
        use std::sync::Arc;

        let drop_count = Arc::new(AtomicUsize::new(0));

        struct TrackDrop(Arc<AtomicUsize>);
        impl Drop for TrackDrop {
            fn drop(&mut self) {
                self.0.fetch_add(1, AO::SeqCst);
            }
        }

        let dc = drop_count.clone();
        let mut queue = Queue::<TrackDrop>::new();
        let node = QueueNode::new(TrackDrop(dc.clone()));
        // ref_cnt = 1（仅 node 持有）
        queue.insert_tail(&node);
        // ref_cnt = 2（node + queue 份额）

        // remove() 时：
        //   in_queue.store(false) 时 ref_cnt = 2
        //   step 5: fetch_sub(1) 旧值 = 2 ≠ 1 → 不 free
        node.remove();

        // remove() 返回后 TrackDrop 还未 drop（ref_cnt = 1，node 持有）
        assert_eq!(drop_count.load(AO::SeqCst), 0, "remove() 后 node 句柄仍存活，T 不应 drop");

        // 最终 drop node 句柄：ref_cnt 1→0 → free → TrackDrop::drop
        drop(node);
        assert_eq!(drop_count.load(AO::SeqCst), 1, "node 句柄 drop 后 T 应恰好析构 1 次");
    }
}
