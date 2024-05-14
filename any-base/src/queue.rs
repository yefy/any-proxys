#![allow(dead_code)]

use std::ptr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

type QueueNodeCmp<T> = fn(a: QueueNode<T>, b: QueueNode<T>) -> std::cmp::Ordering;

struct QueueNodePtr<T> {
    prev: *mut QueueNodePtr<T>,
    next: *mut QueueNodePtr<T>,
    data: Option<T>,
    ref_cnt: AtomicUsize,
    node: Option<QueueNode<T>>,
    len: Option<Arc<AtomicUsize>>,
}

unsafe impl<T: Send> Send for QueueNodePtr<T> {}
unsafe impl<T: Sync> Sync for QueueNodePtr<T> {}

impl<T> QueueNodePtr<T> {
    pub fn new(data: Option<T>) -> Self {
        QueueNodePtr {
            prev: ptr::null_mut(),
            next: ptr::null_mut(),
            data,
            ref_cnt: AtomicUsize::new(1),
            node: None,
            len: None,
        }
    }
}

pub struct QueueNode<T> {
    ptr: *mut QueueNodePtr<T>,
}

unsafe impl<T: Send> Send for QueueNode<T> {}
unsafe impl<T: Sync> Sync for QueueNode<T> {}

impl<T> QueueNode<T> {
    pub fn new(data: T) -> Self {
        Self::new_option(Some(data))
    }

    fn new_nil() -> Self {
        Self::new_option(None)
    }

    fn new_option(data: Option<T>) -> Self {
        let ptr = Box::new(QueueNodePtr::new(data));
        let ptr = Box::into_raw(ptr);
        let node = QueueNode { ptr };

        node.ptr_mut().prev = node.ptr();
        node.ptr_mut().next = node.ptr();
        node
    }

    pub fn empty(&self) -> bool {
        self.ptr_mut().prev == self.ptr
    }

    pub fn is_nil(&self) -> bool {
        self.empty() || self.ptr_mut().node.is_none() || self.ptr_mut().len.is_none()
    }

    fn ptr(&self) -> *mut QueueNodePtr<T> {
        self.ptr
    }

    fn ptr_mut(&self) -> &mut QueueNodePtr<T> {
        let ptr = self.ptr();
        unsafe { &mut *ptr }
    }

    pub fn data(&self) -> &T {
        self.ptr_mut().data.as_ref().unwrap()
    }
    pub fn data_mut(&mut self) -> &mut T {
        self.ptr_mut().data.as_mut().unwrap()
    }

    pub fn next(&self) -> QueueNode<T> {
        unsafe { &mut *self.ptr_mut().next }.node.clone().unwrap()
    }

    pub fn prev(&self) -> QueueNode<T> {
        unsafe { &mut *self.ptr_mut().prev }.node.clone().unwrap()
    }

    pub fn insert_after(&mut self, node: QueueNode<T>) {
        if self.is_nil() {
            panic!("self.is_nil()");
        }

        if !node.is_nil() {
            panic!("!node.is_nil()");
        }
        node.ptr_mut().node = Some(node.clone());
        node.ptr_mut().len = self.ptr_mut().len.clone();

        node.ptr_mut().next = self.ptr_mut().next;
        unsafe {
            node.ptr_mut().next.as_mut().unwrap().prev = node.ptr();
        }

        node.ptr_mut().prev = self.ptr();
        self.ptr_mut().next = node.ptr();

        let len = self.ptr_mut().len.clone().unwrap();
        len.fetch_add(1, Ordering::Relaxed);
    }

    pub fn remove(&mut self) {
        if self.is_nil() {
            return;
        }
        self.ptr_mut().node = None;
        let len = self.ptr_mut().len.clone().unwrap();
        len.fetch_sub(1, Ordering::Relaxed);
        self.ptr_mut().len = None;

        unsafe {
            self.ptr_mut().next.as_mut().unwrap().prev = self.ptr_mut().prev;
            self.ptr_mut().prev.as_mut().unwrap().next = self.ptr_mut().next;
        }

        self.ptr_mut().prev = self.ptr();
        self.ptr_mut().next = self.ptr();
    }
}

impl<T> PartialEq for QueueNode<T> {
    fn eq(&self, other: &QueueNode<T>) -> bool {
        self.ptr == other.ptr
    }
}

impl<T> Clone for QueueNode<T> {
    fn clone(&self) -> Self {
        QueueNode::from(self.ptr())
    }
}

impl<T> From<*mut QueueNodePtr<T>> for QueueNode<T> {
    fn from(ptr: *mut QueueNodePtr<T>) -> Self {
        let old_size = unsafe { &mut *ptr }.ref_cnt.fetch_add(1, Ordering::Relaxed);
        if old_size > usize::MAX >> 1 {
            panic!("old_size > usize::MAX >> 1");
        }
        QueueNode { ptr }
    }
}

impl<T> Drop for QueueNode<T> {
    fn drop(&mut self) {
        if self.ptr_mut().ref_cnt.fetch_sub(1, Ordering::Release) != 1 {
            return;
        }
        self.ptr_mut().ref_cnt.load(Ordering::Acquire);
        drop(unsafe { Box::from_raw(self.ptr()) });
    }
}

pub struct Queue<T> {
    node: QueueNode<T>,
    len: Arc<AtomicUsize>,
}

unsafe impl<T: Send> Send for Queue<T> {}
unsafe impl<T: Sync> Sync for Queue<T> {}

impl<T> Drop for Queue<T> {
    fn drop(&mut self) {
        loop {
            let node = self.pop_head();
            if node.is_none() {
                break;
            }
        }
    }
}

impl<T> Queue<T> {
    pub fn new() -> Self {
        let len = Arc::new(AtomicUsize::new(0));
        let node = QueueNode::new_nil();
        node.ptr_mut().node = Some(node.clone());
        node.ptr_mut().len = Some(len.clone());
        Queue { node, len }
    }

    pub fn empty(&self) -> bool {
        self.node.empty()
    }

    pub fn len(&self) -> usize {
        self.len.load(Ordering::Relaxed)
    }

    pub fn insert_head(&mut self, node: QueueNode<T>) {
        if !node.is_nil() {
            panic!("!node.is_nil()");
        }
        node.ptr_mut().node = Some(node.clone());
        node.ptr_mut().len = Some(self.len.clone());

        node.ptr_mut().next = self.node.ptr_mut().next;
        unsafe {
            node.ptr_mut().next.as_mut().unwrap().prev = node.ptr();
        }

        node.ptr_mut().prev = self.node.ptr();
        self.node.ptr_mut().next = node.ptr();

        self.len.fetch_add(1, Ordering::Relaxed);
    }

    pub fn insert_tail(&mut self, node: QueueNode<T>) {
        if !node.is_nil() {
            panic!("!node.is_nil()");
        }
        node.ptr_mut().node = Some(node.clone());
        node.ptr_mut().len = Some(self.len.clone());

        node.ptr_mut().prev = self.node.ptr_mut().prev;
        unsafe {
            node.ptr_mut().prev.as_mut().unwrap().next = node.ptr();
        }
        node.ptr_mut().next = self.node.ptr();
        self.node.ptr_mut().prev = node.ptr();

        self.len.fetch_add(1, Ordering::Relaxed);
    }

    pub fn pop_head(&self) -> Option<QueueNode<T>> {
        if self.empty() {
            return None;
        }
        let mut node = self.head();
        node.remove();
        Some(node)
    }

    pub fn pop_tail(&self) -> Option<QueueNode<T>> {
        if self.empty() {
            return None;
        }
        let mut node = self.last();
        node.remove();
        Some(node)
    }

    pub fn pop_last(&self) -> Option<QueueNode<T>> {
        self.pop_tail()
    }

    pub fn head(&self) -> QueueNode<T> {
        unsafe { &mut *self.node.ptr_mut().next }
            .node
            .clone()
            .unwrap()
    }

    pub fn last(&self) -> QueueNode<T> {
        unsafe { &mut *self.node.ptr_mut().prev }
            .node
            .clone()
            .unwrap()
    }

    pub fn sentinel(&self) -> QueueNode<T> {
        unsafe { &mut *self.node.ptr() }.node.clone().unwrap()
    }

    pub fn sort(&mut self, cmp: QueueNodeCmp<T>) {
        if self.empty() {
            return;
        }

        let q = self.head();
        if q == self.last() {
            return;
        }

        let mut q = q.next();
        loop {
            if q == self.sentinel() {
                break;
            }

            let mut prev = q.prev();
            let next = q.next();

            q.remove();

            loop {
                let ord = cmp(prev.clone(), q.clone());
                let n = match ord {
                    std::cmp::Ordering::Less => -1,
                    std::cmp::Ordering::Equal => 0,
                    std::cmp::Ordering::Greater => 1,
                };

                if n <= 0 {
                    break;
                }

                prev = prev.prev();
                if prev == self.sentinel() {
                    break;
                }
            }
            prev.insert_after(q);
            q = next;
        }
    }
}

#[cfg(test)]
mod tests {

    fn sort_cmp<T: Ord>(a: QueueNode<T>, b: QueueNode<T>) -> std::cmp::Ordering {
        a.data().cmp(&b.data())
    }

    fn test0() {
        println!("start test0");
        let mut queue = Queue::<i32>::new();

        let mut q = queue.head();
        loop {
            if q == queue.sentinel() {
                break;
            }
            println!("Node data: {}", q.data());
            q = q.next();
        }

        queue.sort(sort_cmp);

        let mut q = queue.head();
        loop {
            if q == queue.sentinel() {
                break;
            }
            println!("sort Node data: {}", q.data());
            q = q.next();
        }

        if queue.empty() {
            println!("The list is empty");
        } else {
            println!("The list is not empty");
        }

        if queue.empty() {
            println!("The list is empty");
        } else {
            println!("The list is not empty");
        }
        println!("end test0");
    }

    fn test1() {
        println!("start test1");
        let mut queue = Queue::<i32>::new();
        let mut node3 = QueueNode::new(3);
        queue.insert_head(node3.clone());

        let mut q = queue.head();
        loop {
            if q == queue.sentinel() {
                break;
            }
            println!("Node data: {}", q.data());
            q = q.next();
        }

        queue.sort(sort_cmp);

        let mut q = queue.head();
        loop {
            if q == queue.sentinel() {
                break;
            }
            println!("sort Node data: {}", q.data());
            q = q.next();
        }

        if queue.empty() {
            println!("The list is empty");
        } else {
            println!("The list is not empty");
        }

        println!("The list node3.remove()");
        node3.remove();

        let mut q = queue.head();
        loop {
            if q == queue.sentinel() {
                break;
            }
            println!("remove node3 data: {}", q.data());
            q = q.next();
        }

        if queue.empty() {
            println!("The list is empty");
        } else {
            println!("The list is not empty");
        }
        println!("end test1");
    }

    fn test2() {
        println!("start test2");
        let mut queue = Queue::<i32>::new();
        let mut node3 = QueueNode::new(3);
        let mut node1 = QueueNode::new(1);
        queue.insert_head(node3.clone());
        queue.insert_head(node1.clone());

        let mut q = queue.head();
        loop {
            if q == queue.sentinel() {
                break;
            }
            println!("Node data: {}", q.data());
            q = q.next();
        }

        queue.sort(sort_cmp);

        let mut q = queue.head();
        loop {
            if q == queue.sentinel() {
                break;
            }
            println!("sort Node data: {}", q.data());
            q = q.next();
        }

        if queue.empty() {
            println!("The list is empty");
        } else {
            println!("The list is not empty");
        }

        println!("The list node3.remove()");
        node3.remove();

        let mut q = queue.head();
        loop {
            if q == queue.sentinel() {
                break;
            }
            println!("remove node3 data: {}", q.data());
            q = q.next();
        }

        node1.remove();

        if queue.empty() {
            println!("The list is empty");
        } else {
            println!("The list is not empty");
        }
        println!("end test2");
    }

    fn test3() {
        println!("start test3");
        let mut queue = Queue::<i32>::new();
        let mut node3 = QueueNode::new(3);
        let mut node1 = QueueNode::new(1);
        let mut node7 = QueueNode::new(7);
        queue.insert_head(node3.clone());
        queue.insert_head(node1.clone());
        queue.insert_head(node7.clone());

        let mut q = queue.head();
        loop {
            if q == queue.sentinel() {
                break;
            }
            println!("Node data: {}", q.data());
            q = q.next();
        }

        queue.sort(sort_cmp);

        let mut q = queue.head();
        loop {
            if q == queue.sentinel() {
                break;
            }
            println!("sort Node data: {}", q.data());
            q = q.next();
        }

        if queue.empty() {
            println!("The list is empty");
        } else {
            println!("The list is not empty");
        }

        println!("The list node3.remove()");
        node3.remove();

        let mut q = queue.head();
        loop {
            if q == queue.sentinel() {
                break;
            }
            println!("remove node3 data: {}", q.data());
            q = q.next();
        }

        node1.remove();
        node7.remove();

        if queue.empty() {
            println!("The list is empty");
        } else {
            println!("The list is not empty");
        }
        println!("end test3");
    }

    fn test4() {
        println!("start test4");
        let mut queue = Queue::<i32>::new();
        let mut node3 = QueueNode::new(3);
        let mut node1 = QueueNode::new(1);
        let mut node7 = QueueNode::new(7);
        queue.insert_head(node3.clone());
        queue.insert_head(node1.clone());
        queue.insert_head(node7.clone());

        let mut q = queue.head();
        loop {
            if q == queue.sentinel() {
                break;
            }
            println!("Node data: {}", q.data());
            q = q.next();
        }

        queue.sort(sort_cmp);

        let mut q = queue.head();
        loop {
            if q == queue.sentinel() {
                break;
            }
            println!("sort Node data: {}", q.data());
            q = q.next();
        }

        if queue.empty() {
            println!("The list is empty");
        } else {
            println!("The list is not empty");
        }

        println!("The list node3.remove()");
        node3.remove();

        let mut q = queue.head();
        loop {
            if q == queue.sentinel() {
                break;
            }
            println!("remove node3 data: {}", q.data());
            q = q.next();
        }

        let node13 = QueueNode::new(13);
        let node11 = QueueNode::new(11);
        let node17 = QueueNode::new(17);
        queue.insert_head(node13.clone());
        queue.insert_head(node11.clone());
        queue.insert_tail(node17.clone());

        node1.remove();
        node7.remove();

        if queue.empty() {
            println!("The list is empty");
        } else {
            println!("The list is not empty");
        }

        let mut q = queue.head();
        loop {
            if q == queue.sentinel() {
                break;
            }
            println!("end Node data: {}", q.data());
            q = q.next();
        }

        println!("end test4");
    }
    fn test5() {
        println!("start test5");
        let mut queue = Queue::<i32>::new();
        let mut node3 = QueueNode::new(3);
        let mut node1 = QueueNode::new(1);
        let mut node7 = QueueNode::new(7);
        queue.insert_head(node3.clone());
        queue.insert_head(node1.clone());
        queue.insert_head(node7.clone());

        let mut q = queue.head();
        loop {
            if q == queue.sentinel() {
                break;
            }
            println!("Node data: {}", q.data());
            q = q.next();
        }

        queue.sort(sort_cmp);

        let mut q = queue.head();
        loop {
            if q == queue.sentinel() {
                break;
            }
            println!("sort Node data: {}", q.data());
            q = q.next();
        }

        let node = queue.pop_head();
        if node.is_some() {
            let node = node.unwrap();
            println!("pop_head Node data: {}", node.data());
        }

        let node = queue.pop_tail();
        if node.is_some() {
            let node = node.unwrap();
            println!("pop_tail Node data: {}", node.data());
        }

        if queue.empty() {
            println!("The list is empty");
        } else {
            println!("The list is not empty");
        }

        let mut q = queue.head();
        loop {
            if q == queue.sentinel() {
                break;
            }
            println!("remove node3 data: {}", q.data());
            q = q.next();
        }

        let mut node13 = QueueNode::new(13);
        let node11 = QueueNode::new(11);
        let mut node17 = QueueNode::new(17);
        queue.insert_head(node13.clone());
        queue.insert_tail(node11);
        queue.insert_tail(node17.clone());

        if queue.empty() {
            println!("The list is empty");
        } else {
            println!("The list is not empty");
        }

        let mut q = queue.head();
        loop {
            if q == queue.sentinel() {
                break;
            }
            println!("end1 Node data: {}", q.data());
            q = q.next();
        }

        node13.remove();
        queue.insert_head(node13);

        let mut q = queue.head();
        loop {
            if q == queue.sentinel() {
                break;
            }
            println!("end2 Node data: {}", q.data());
            q = q.next();
        }

        node17.remove();
        queue.insert_tail(node17);
        let mut q = queue.head();
        loop {
            if q == queue.sentinel() {
                break;
            }
            println!("end3 Node data: {}", q.data());
            q = q.next();
        }

        println!("end test5");
    }

    fn test6() {
        println!("start test6");
        let mut node13 = QueueNode::new(13);
        println!("ptr 1:{}", node13.ptr as u64);

        let node133 = node13;
        println!("ptr 1:{}", node133.ptr as u64);

        let mut queue = Queue::<i32>::new();
        let mut node3 = QueueNode::new(3);
        let mut node17 = QueueNode::new(17);
        let mut node15 = QueueNode::new(5);
        queue.insert_head(node3);
        queue.insert_tail(node17);
        queue.insert_tail(node15.clone());

        let mut q = queue.head();
        loop {
            if q == queue.sentinel() {
                break;
            }
            println!("Node data: {}", q.data());
            q = q.next();
        }
        println!("end test6");
    }

    #[tokio::test]
    async fn main_test() {
        test0();
        test1();
        test2();
        test3();
        test4();
        test5();
        test6();
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
        println!("end");
    }
}
