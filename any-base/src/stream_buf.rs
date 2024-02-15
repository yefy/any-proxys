use bytes::Bytes;

pub struct StreamBuf {
    data: Bytes,
}

impl Default for StreamBuf {
    fn default() -> Self {
        StreamBuf { data: Bytes::new() }
    }
}

impl From<Vec<u8>> for StreamBuf {
    fn from(vec: Vec<u8>) -> StreamBuf {
        let data = Bytes::from(vec);
        Self::new(data)
    }
}

impl StreamBuf {
    pub fn new(data: Bytes) -> StreamBuf {
        StreamBuf { data }
    }

    #[inline]
    pub fn set_bytes(&mut self, data: Bytes) {
        self.data = data;
    }

    #[inline]
    pub fn set_vec(&mut self, vec: Vec<u8>) {
        let data = Bytes::from(vec);
        self.data = data;
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    #[inline]
    pub fn remaining(&self) -> usize {
        self.len()
    }

    #[inline]
    pub fn split(&mut self) -> Bytes {
        self.data.split_to(self.len())
    }

    #[inline]
    pub fn take(&mut self) -> Bytes {
        self.split()
    }

    #[inline]
    pub fn split_to(&mut self, at: usize) -> Bytes {
        if at >= self.len() {
            return self.take();
        }
        self.data.split_to(at)
    }
}
