use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

pub struct RoundAsyncChannel<T> {
    index: AtomicUsize,
    senders: Vec<async_channel::Sender<T>>,
    is_close: AtomicBool,
    lock: Arc<tokio::sync::Mutex<bool>>,
}

impl<T> RoundAsyncChannel<T> {
    pub fn new() -> RoundAsyncChannel<T> {
        RoundAsyncChannel {
            index: AtomicUsize::new(0),
            senders: Vec::new(),
            is_close: AtomicBool::new(false),
            lock: Arc::new(tokio::sync::Mutex::new(false)),
        }
    }

    pub fn get_lock(&self) -> Arc<tokio::sync::Mutex<bool>> {
        self.lock.clone()
    }

    pub fn is_full(&self) -> bool {
        for (_, sender) in self.senders.iter().enumerate() {
            if sender.is_full() {
                return true;
            }
        }
        false
    }

    pub fn get_index(&self) -> usize {
        self.index.load(Ordering::Relaxed)
    }

    pub fn fetch_add_index(&self) -> usize {
        self.index.fetch_add(1, Ordering::Relaxed)
    }

    pub fn next_index(&self) -> usize {
        let index = self.fetch_add_index();
        index % self.senders.len()
    }

    pub fn round_sender_clone(&self) -> async_channel::Sender<T> {
        self.senders[self.next_index()].clone()
    }

    pub fn senders(&self) -> Vec<async_channel::Sender<T>> {
        self.senders.clone()
    }

    pub async fn round_send(&self, value: T) -> Result<(), async_channel::SendError<T>> {
        self.senders[self.next_index()].send(value).await
    }

    pub fn close(&self) {
        if self.is_close() {
            return;
        }
        for (_, sender) in self.senders.iter().enumerate() {
            sender.close();
        }
        self.is_close.store(true, Ordering::Relaxed);
    }

    pub fn clear(&self) {
        self.is_close.store(true, Ordering::Relaxed);
    }

    pub fn is_close(&self) -> bool {
        self.is_close.load(Ordering::Relaxed)
    }

    pub fn channel(&mut self, buffer: usize) -> anyhow::Result<async_channel::Receiver<T>> {
        if self.is_close() {
            return Err(anyhow::anyhow!("self.is_close()"));
        }
        let (tx, rx) = if buffer <= 0 {
            async_channel::unbounded()
        } else {
            async_channel::bounded(buffer)
        };
        self.senders.push(tx);
        Ok(rx)
    }

    pub fn add_sender(&mut self, tx: async_channel::Sender<T>) {
        self.senders.push(tx);
    }
}

impl<T> Drop for RoundAsyncChannel<T> {
    fn drop(&mut self) {
        self.close();
    }
}
