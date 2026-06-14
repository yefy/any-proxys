#![allow(dead_code)]

use std::cell::UnsafeCell;
use std::ptr;
use std::sync::atomic::{fence, AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

/// 比较函数取引用，避免调用方为每次 cmp 做额外 clone
type QueueNodeCmp<T> = fn(a: &QueueNode<T>, b: &QueueNode<T>) -> std::cmp::Ordering;

// ─── 设计说明 ──────────────────────────────────────────────────────────────────
//
// 侵入式双向循环链表，O(1) insert / remove。
//
// 所有权模型：
//   - 用户持有 QueueNode<T> 句柄，每个句柄使 ref_cnt +1
//   - 节点入队时，队列额外持有一份所有权（ref_cnt +1）
//   - 节点出队时，队列释放那份所有权（ref_cnt -1）
//   - ref_cnt 降为 0 时，堆内存释放
//
// 线程安全：
//   - Queue / QueueNode 本身不加任何内部锁
//   - 所有 insert / remove / traverse / sort 必须由调用方在外部加锁保护
//   - 推荐：Arc<Mutex<Queue<T>>>
//
// ─────────────────────────────────────────────────────────────────────────────

// ─── 内部节点 ─────────────────────────────────────────────────────────────────

pub(crate) struct QueueNodeInner<T> {
    prev: *mut QueueNodeInner<T>,
    next: *mut QueueNodeInner<T>,
    /// 用 UnsafeCell 包裹：`data()` 返回 `&T`，`data_mut()` 返回 `&mut T`，
    /// 若不使用 UnsafeCell，在 Rust 内存模型（Stacked Borrows）下同时存在
    /// `&T` 与 `&mut T` 指向同一位置是 UB；UnsafeCell 是唯一合规的内部可变性机制。
    data: UnsafeCell<Option<T>>,
    /// 用户句柄数 + 队列持有份额（入队时 +1，出队时 -1）
    ref_cnt: AtomicUsize,
    /// 当前是否在某个队列中。
    /// 必须是 AtomicBool：即使所有结构性操作都在外部锁内，
    /// is_detached() 可以在不持有锁的情况下被任意线程调用，
    /// 普通 bool 的跨线程读写是 data race / UB。
    in_queue: AtomicBool,
    /// 所属队列的长度计数器（入队时设置，出队时 take）
    len: Option<Arc<AtomicUsize>>,
}

unsafe impl<T: Send> Send for QueueNodeInner<T> {}
// Sync 需要 T: Sync：&QueueNodeInner<T> 可被多线程共享，
// data() 会返回 &T，若 T 非 Sync（如 Cell/RefCell）则 UB。
// 另需 T: Send：最后一个句柄可能在任意线程析构，相当于跨线程 move T。
unsafe impl<T: Send + Sync> Sync for QueueNodeInner<T> {}

// ─── 公开句柄 ─────────────────────────────────────────────────────────────────

pub struct QueueNode<T> {
    /// 裸指针，pub(crate) 供同 crate 内遍历时与 sentinel_ptr() 比较
    pub(crate) ptr: *mut QueueNodeInner<T>,
}

unsafe impl<T: Send> Send for QueueNode<T> {}
unsafe impl<T: Send + Sync> Sync for QueueNode<T> {}

impl<T> QueueNode<T> {
    pub fn new(data: T) -> Self {
        let raw = Box::into_raw(Box::new(QueueNodeInner {
            prev: ptr::null_mut(),
            next: ptr::null_mut(),
            data: UnsafeCell::new(Some(data)),
            ref_cnt: AtomicUsize::new(1),
            in_queue: AtomicBool::new(false),
            len: None,
        }));
        unsafe {
            (*raw).prev = raw;
            (*raw).next = raw;
        }
        QueueNode { ptr: raw }
    }

    /// 创建哨兵节点（无数据，永久标记为 in_queue）
    fn new_sentinel() -> Self {
        let raw = Box::into_raw(Box::new(QueueNodeInner {
            prev: ptr::null_mut(),
            next: ptr::null_mut(),
            data: UnsafeCell::new(None),
            ref_cnt: AtomicUsize::new(1),
            in_queue: AtomicBool::new(true),
            len: None,
        }));
        unsafe {
            (*raw).prev = raw;
            (*raw).next = raw;
        }
        QueueNode { ptr: raw }
    }

    /// 从裸指针创建句柄，ref_cnt +1
    #[inline]
    fn from_raw(ptr: *mut QueueNodeInner<T>) -> Self {
        unsafe { (*ptr).ref_cnt.fetch_add(1, Ordering::Relaxed) };
        QueueNode { ptr }
    }

    /// 节点当前是否不在任何队列中。
    ///
    /// 可以在不持有外部锁的情况下调用（Acquire 保证读到最新状态）。
    #[inline]
    pub fn is_detached(&self) -> bool {
        unsafe { !(*self.ptr).in_queue.load(Ordering::Acquire) }
    }

    /// 返回数据的共享引用。通过 `UnsafeCell::get()` 取裸指针，
    /// 合规地告知编译器此字段具有内部可变性，禁止 LLVM 对其做只读缓存假设。
    #[inline]
    pub fn data(&self) -> &T {
        unsafe {
            (*(*self.ptr).data.get())
                .as_ref()
                .expect("sentinel node has no data")
        }
    }

    /// # Safety
    ///
    /// 调用方必须保证：
    /// - 已持有队列的外部锁（或确认单线程独占）
    /// - 此节点当前不存在其他活跃的 `&T` 或 `&mut T`
    ///
    /// `&self` 接收者是故意的：`&mut self` 只能阻止对同一句柄的重复借用，
    /// 无法阻止通过 clone 句柄产生的别名——用 `unsafe` 将责任转交给调用方。
    ///
    /// 通过 `UnsafeCell` 取得 `*mut T`，符合 Rust 内存模型（Stacked Borrows），
    /// 不触发 `&T` 同时存在时的 UB。
    #[inline]
    pub unsafe fn data_mut(&self) -> &mut T {
        (*(*self.ptr).data.get())
            .as_mut()
            .expect("sentinel node has no data")
    }

    /// 返回链表中的下一个节点。
    ///
    /// **已摘除（detached）的节点**：`next()` 和 `prev()` 均返回节点自身，
    /// 在该状态下做遍历会形成死循环，调用方须先用 `is_detached()` 判断。
    #[inline]
    pub fn next(&self) -> QueueNode<T> {
        unsafe { Self::from_raw((*self.ptr).next) }
    }

    /// 返回链表中的前一个节点。见 [`next`](Self::next) 关于 detached 节点的说明。
    #[inline]
    pub fn prev(&self) -> QueueNode<T> {
        unsafe { Self::from_raw((*self.ptr).prev) }
    }

    /// 将本节点从队列中摘除。若已不在队列中则为空操作（幂等）。
    ///
    /// # 关于 `&self`
    ///
    /// `QueueNode<T>` 是引用计数共享句柄（类似 `Arc`），`&mut self` 并不能阻止
    /// 其他 clone 并发访问同一底层节点。此处统一使用 `&self`，
    /// 由调用方通过外部锁（如 `Mutex<Queue<T>>`）保证互斥。
    pub fn remove(&self) {
        unsafe {
            // 早返回检查：在外部锁保护下 Relaxed 已足够
            if !(*self.ptr).in_queue.load(Ordering::Relaxed) {
                return;
            }

            // 1. 修复双向链接
            let next = (*self.ptr).next;
            let prev = (*self.ptr).prev;
            (*next).prev = prev;
            (*prev).next = next;

            // 2. 节点恢复自环
            (*self.ptr).prev = self.ptr;
            (*self.ptr).next = self.ptr;

            // 3. Release store：步骤 1-2 的写操作对之后 Acquire load 可见
            //    任何 is_detached() 读到 false 时，同时也能看到自环指针
            (*self.ptr).in_queue.store(false, Ordering::Release);

            // 4. 释放 len 计数（内部状态，无需额外 ordering）
            if let Some(len) = (*self.ptr).len.take() {
                len.fetch_sub(1, Ordering::Relaxed);
            }

            // 5. 释放队列持有的那一份所有权
            if (*self.ptr).ref_cnt.fetch_sub(1, Ordering::Release) == 1 {
                fence(Ordering::Acquire);
                drop(Box::from_raw(self.ptr));
            }
        }
    }

    /// 在 self 之后插入 node。
    ///
    /// - self 必须在队列中（`!is_detached()`）
    /// - node 必须不在任何队列中（`is_detached()`）
    ///
    /// # 关于 `&self`
    ///
    /// 同 `remove`，调用方须持有外部锁保证互斥。
    pub fn insert_after(&self, node: &QueueNode<T>) {
        // 结构完整性检查：违反此约束会静默腐败两条链，使用 assert! 而非 debug_assert!
        // 确保 release 模式同样拦截
        assert!(!self.is_detached(), "insert_after: self is not in a queue");
        assert!(node.is_detached(), "insert_after: node is already in a queue");

        unsafe {
            // 预先 clone 所有可能 OOM-panic 的 Arc，放在任何状态变更之前。
            // 若此处 panic，ref_cnt 未被触碰，节点状态保持一致。
            let len = (*self.ptr)
                .len
                .clone()
                .expect("in-queue node must have a len counter");
            let node_len = len.clone();

            // 1. 队列获得 node 的所有权（从此处起不再 panic）
            (*node.ptr).ref_cnt.fetch_add(1, Ordering::Relaxed);
            (*node.ptr).len = Some(node_len);

            // 2. 链入：建立双向连接
            let self_next = (*self.ptr).next;
            (*node.ptr).prev = self.ptr;
            (*node.ptr).next = self_next;
            (*self_next).prev = node.ptr;
            (*self.ptr).next = node.ptr;

            // 3. Release store：步骤 1-2 的写操作对之后 Acquire load 可见
            //    任何 is_detached() 读到 true 时，同时也能看到正确的指针
            (*node.ptr).in_queue.store(true, Ordering::Release);

            // 4. 更新队列长度计数
            len.fetch_add(1, Ordering::Relaxed);
        }
    }
}

impl<T> Clone for QueueNode<T> {
    fn clone(&self) -> Self {
        Self::from_raw(self.ptr)
    }
}

impl<T> Drop for QueueNode<T> {
    fn drop(&mut self) {
        unsafe {
            if (*self.ptr).ref_cnt.fetch_sub(1, Ordering::Release) != 1 {
                return;
            }
            // 最后一个句柄：同步所有先前的写操作后再释放内存
            fence(Ordering::Acquire);
            drop(Box::from_raw(self.ptr));
        }
    }
}

impl<T> PartialEq for QueueNode<T> {
    fn eq(&self, other: &Self) -> bool {
        self.ptr == other.ptr
    }
}

impl<T> Eq for QueueNode<T> {}

// ─── 队列 ──────────────────────────────────────────────────────────────────────

pub struct Queue<T> {
    sentinel: QueueNode<T>,
    len: Arc<AtomicUsize>,
}

unsafe impl<T: Send> Send for Queue<T> {}
unsafe impl<T: Send + Sync> Sync for Queue<T> {}

impl<T> Queue<T> {
    pub fn new() -> Self {
        let len = Arc::new(AtomicUsize::new(0));
        let sentinel = QueueNode::new_sentinel();
        unsafe { (*sentinel.ptr).len = Some(len.clone()) };
        Queue { sentinel, len }
    }

    #[inline]
    pub fn empty(&self) -> bool {
        self.len.load(Ordering::Relaxed) == 0
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len.load(Ordering::Relaxed)
    }

    /// 返回哨兵节点的句柄，用于遍历时的终止判断
    #[inline]
    pub fn sentinel(&self) -> QueueNode<T> {
        self.sentinel.clone()
    }

    /// 返回哨兵节点的裸指针，供 `q.ptr == queue.sentinel_ptr()` 模式使用
    #[inline]
    pub(crate) fn sentinel_ptr(&self) -> *mut QueueNodeInner<T> {
        self.sentinel.ptr
    }

    /// 返回第一个数据节点。队列为空时返回 `None`。
    ///
    /// 遍历时推荐与 `sentinel()` 配合：
    /// ```ignore
    /// let sentinel = queue.sentinel();
    /// if let Some(mut cur) = queue.head() {
    ///     loop {
    ///         // 处理 cur
    ///         cur = cur.next();
    ///         if cur == sentinel { break; }
    ///     }
    /// }
    /// ```
    #[inline]
    pub fn head(&self) -> Option<QueueNode<T>> {
        if self.empty() {
            None
        } else {
            unsafe { Some(QueueNode::from_raw((*self.sentinel.ptr).next)) }
        }
    }

    /// 返回最后一个数据节点。队列为空时返回 `None`。
    #[inline]
    pub fn last(&self) -> Option<QueueNode<T>> {
        if self.empty() {
            None
        } else {
            unsafe { Some(QueueNode::from_raw((*self.sentinel.ptr).prev)) }
        }
    }

    /// `last()` 的别名
    #[inline]
    pub fn tail(&self) -> Option<QueueNode<T>> {
        self.last()
    }

    /// 内部用：跳过空队列检查，直接返回 sentinel.next（可能是 sentinel 自身）
    #[inline]
    fn head_raw(&self) -> QueueNode<T> {
        unsafe { QueueNode::from_raw((*self.sentinel.ptr).next) }
    }

    /// 内部用：跳过空队列检查，直接返回 sentinel.prev（可能是 sentinel 自身）
    #[inline]
    fn tail_raw(&self) -> QueueNode<T> {
        unsafe { QueueNode::from_raw((*self.sentinel.ptr).prev) }
    }

    /// 在头部插入（哨兵之后）
    pub fn insert_head(&mut self, node: &QueueNode<T>) {
        self.sentinel.insert_after(node);
    }

    /// 在尾部插入（哨兵之前）
    pub fn insert_tail(&mut self, node: &QueueNode<T>) {
        self.tail_raw().insert_after(node);
    }

    pub fn pop_head(&mut self) -> Option<QueueNode<T>> {
        let node = self.head()?;
        node.remove();
        Some(node)
    }

    pub fn pop_tail(&mut self) -> Option<QueueNode<T>> {
        let node = self.last()?;
        node.remove();
        Some(node)
    }

    /// `pop_tail()` 的别名
    pub fn pop_last(&mut self) -> Option<QueueNode<T>> {
        self.pop_tail()
    }

    /// 稳定插入排序，O(n²)。必须持有外部锁。
    pub fn sort(&mut self, cmp: QueueNodeCmp<T>) {
        if self.len() <= 1 {
            return;
        }
        let sentinel = self.sentinel();
        // len > 1，head_raw() 必定返回数据节点。
        // `.next()` 内部调用 `from_raw()` 使 q 节点 ref_cnt +1，
        // 故 q 节点 ref_cnt ≥ 2（队列持有 + q 句柄）。
        // 之后 `q.remove()` 只释放队列那一份（ref_cnt -1 → ≥1），
        // q 句柄仍然存活，不触发 free，不存在 Use-After-Free。
        let mut q = self.head_raw().next();
        loop {
            if q == sentinel {
                break;
            }
            let next = q.next();
            let mut prev = q.prev();
            q.remove();
            loop {
                if prev == sentinel {
                    break;
                }
                if cmp(&prev, &q) != std::cmp::Ordering::Greater {
                    break;
                }
                prev = prev.prev();
            }
            prev.insert_after(&q);
            q = next;
        }
    }

    /// 清空队列，释放所有节点的队列所有权。
    ///
    /// 实现分两步：
    /// 1. O(1)：将哨兵重置为空状态，len 归零
    /// 2. O(n)：单次前向遍历旧链，逐节点释放所有权
    ///
    /// Panic 安全性：
    ///   步骤 1 完成后哨兵已与旧链断开。若某节点的 `T::drop` panic，
    ///   未使用 catch_unwind 的实现会直接 unwind 出循环，丢失 `next` 局部变量，
    ///   导致剩余节点永久内存泄漏（虽然内存泄漏在 Rust 中是 safe，但对工业级
    ///   基础设施仍不可接受）。此处用 `catch_unwind` 隔离单节点的 drop panic，
    ///   将泄漏范围限制在那一个节点，后续节点的清理保证继续进行。
    pub fn clear(&mut self) {
        if self.empty() {
            return;
        }
        unsafe {
            let sentinel = self.sentinel.ptr;

            // 1. O(1) 断开整条链，哨兵恢复自环
            let old_head = (*sentinel).next;
            (*sentinel).next = sentinel;
            (*sentinel).prev = sentinel;
            self.len.store(0, Ordering::Relaxed);

            // 2. 前向遍历旧链，逐节点释放队列所有权。
            //    `next` 在任何修改/释放前保存（局部指针值拷贝），始终有效。
            let mut cur = old_head;
            while cur != sentinel {
                let next = (*cur).next;

                // 恢复自环：用户句柄仍存活时 detach 后 next()/prev() 返回自身
                (*cur).prev = cur;
                (*cur).next = cur;

                drop((*cur).len.take());

                // Release store：自环写操作对后续 is_detached() Acquire load 可见
                (*cur).in_queue.store(false, Ordering::Release);

                // 释放队列持有的一份所有权
                if (*cur).ref_cnt.fetch_sub(1, Ordering::Release) == 1 {
                    fence(Ordering::Acquire);
                    let node_box = Box::from_raw(cur);
                    // 隔离 T::drop 的 panic：
                    //   若 panic，当前节点的 Box 内存泄漏（Box 未被 dealloc），
                    //   但 panic 不向调用方传播，后续节点继续被正常处理。
                    //   AssertUnwindSafe：无共享引用跨越此边界，外部锁保证独占。
                    // let _ = std::panic::catch_unwind(
                    //     std::panic::AssertUnwindSafe(move || drop(node_box)),
                    // );

                    //若 T::drop panic，
                    //clear/drop 将提前终止，
                    //剩余节点可能泄漏。
                    //这与 Vec/HashMap 等标准库行为一致。
                    //panic 说明业务逻辑异常了，需要尽快通知上层, 不应该屏蔽panic
                    drop(node_box)
                }

                cur = next;
            }
        }
    }
}

impl<T> Default for Queue<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Drop for Queue<T> {
    fn drop(&mut self) {
        self.clear();
    }
}
