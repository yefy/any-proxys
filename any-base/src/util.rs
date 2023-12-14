use serde::ser::Serializer;
use serde::Serialize;
use std::ops::Deref;
use std::sync::Arc;

pub struct StreamMsg {
    datas: Vec<Vec<u8>>,
    len: usize,
    pos: usize,
    index: usize,
    index_pos: usize,
    cache: Vec<u8>,
}

impl DynamicReset for StreamMsg {
    fn reset(&mut self) {
        self.reset()
    }
}

impl Default for StreamMsg {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamMsg {
    pub fn new() -> Self {
        StreamMsg {
            datas: Vec::with_capacity(10),
            len: 0,
            pos: 0,
            index: 0,
            index_pos: 0,
            cache: Vec::with_capacity(0),
        }
    }

    pub fn push_msg(&mut self, msg: StreamMsg) {
        self.len += msg.len();
        let datas = msg.take();
        self.datas.extend(datas);
    }

    pub fn push(&mut self, data: Vec<u8>) {
        self.len += data.len();
        self.datas.push(data);
    }

    pub fn reset(&mut self) {
        unsafe { self.datas.set_len(0) };
        self.len = 0;
        self.pos = 0;
        self.index = 0;
        self.index_pos = 0;
        unsafe { self.cache.set_len(0) };
    }

    pub fn init(&mut self) {
        self.pos = 0;
        self.index = 0;
        self.index_pos = 0;
        unsafe { self.cache.set_len(0) };
    }

    pub fn len(&self) -> usize {
        self.len - self.pos
    }
    pub fn take(mut self) -> Vec<Vec<u8>> {
        let mut datas: Vec<Vec<u8>> = Vec::with_capacity(10);
        swap(&mut self.datas, &mut datas);
        self.reset();
        datas
    }

    pub fn to_vec(mut self) -> Vec<u8> {
        if self.datas.len() == 1 && self.pos == 0 {
            let datas = self.datas.pop().unwrap();
            self.reset();
            return datas;
        }

        let mut data = Vec::with_capacity(self.len());
        let mut pos = self.pos;
        for v in &self.datas {
            let mut start = 0;
            let end = v.len();
            if pos >= end {
                pos -= end;
                continue;
            }
            pos = 0;
            start += pos;
            data.extend_from_slice(&v.as_slice()[start..end]);
        }
        self.reset();
        data
    }

    pub fn consume_range(&mut self, start: usize, end: usize) -> &[u8] {
        if start != self.pos {
            panic!("start != self.pos");
        }
        let size = end - start;
        self.consume(size)
    }
    pub fn consume(&mut self, mut size: usize) -> &[u8] {
        if size > self.len() {
            panic!("consume size:{} > self.len():{}", size, self.len());
        }
        if self.cache.capacity() < size {
            self.cache.resize(size, 0);
        }
        unsafe { self.cache.set_len(0) };
        let mut cache_end = 0;
        loop {
            if self.pos >= self.len {
                break;
            }

            if size <= 0 {
                break;
            }

            let data = &self.datas[self.index];
            let left = data.len() - self.index_pos;
            if size <= left {
                let consume = size;
                size -= consume;
                let start = self.index_pos;
                let end = self.index_pos + consume;
                self.pos += consume;
                self.index_pos += consume;
                if consume == left {
                    self.index += 1;
                    self.index_pos = 0;
                }
                if cache_end > 0 {
                    cache_end += consume;
                    self.cache.extend_from_slice(&data.as_slice()[start..end]);
                } else {
                    return &data.as_slice()[start..end];
                }
            } else {
                let consume = left;
                size -= consume;
                let start = self.index_pos;
                let end = self.index_pos + consume;
                self.pos += consume;
                self.index += 1;
                self.index_pos = 0;
                cache_end += consume;
                self.cache.extend_from_slice(&data.as_slice()[start..end]);
            }
        }
        return &self.cache.as_slice()[..cache_end];
    }

    pub fn skip(&mut self, mut size: usize) {
        if size > self.len() {
            panic!("skip size:{} > self.len():{}", size, self.len());
        }

        loop {
            let data = &self.datas[self.index];
            let left = data.len() - self.index_pos;
            if size <= left {
                let consume = size;
                self.pos += consume;
                self.index_pos += consume;
                if consume == left {
                    self.index += 1;
                    self.index_pos = 0;
                }
                break;
            } else {
                let consume = left;
                size -= consume;
                self.pos += consume;
                self.index += 1;
                self.index_pos = 0;
            }
        }
        return;
    }

    pub fn rollback(&mut self, mut size: usize) {
        if size > self.pos {
            panic!("size > self.pos");
        }
        loop {
            if size <= self.index_pos {
                self.index_pos -= size;
                self.pos -= size;
                return;
            } else {
                if self.index <= 0 {
                    panic!("self.index <= 0");
                }
                size -= self.index_pos;
                self.pos -= self.index_pos;
                self.index -= 1;
                self.index_pos = self.datas[self.index].len();
            }
        }
    }

    pub fn init_and_data(&mut self, start: usize, end: usize) -> &[u8] {
        if start != self.pos {
            self.init();
            self.skip(start);
        }
        self.consume_range(start, end)
    }
}

#[derive(Clone)]
pub struct ArcString {
    str: Arc<String>,
}

impl From<&str> for ArcString {
    /// Converts a `&str` into a [`String`].
    ///
    /// The result is allocated on the heap.
    #[inline]
    fn from(s: &str) -> ArcString {
        ArcString::new(s.to_owned())
    }
}
impl Eq for ArcString {}

impl From<String> for ArcString {
    /// Converts a `&str` into a [`String`].
    ///
    /// The result is allocated on the heap.
    #[inline]
    fn from(s: String) -> ArcString {
        ArcString::new(s)
    }
}

impl Default for ArcString {
    /// Converts a `&str` into a [`String`].
    ///
    /// The result is allocated on the heap.
    #[inline]
    fn default() -> Self {
        ArcString::new("".to_string())
    }
}

impl ArcString {
    pub fn new(str: String) -> Self {
        ArcString { str: Arc::new(str) }
    }

    pub fn as_str(&self) -> &str {
        &self.str
    }

    pub fn string(&self) -> String {
        self.str.to_string()
    }
}

impl Deref for ArcString {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.str
    }
}
use std::fmt;
impl fmt::Debug for ArcString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Drain").field(&self.as_str()).finish()
    }
}

impl fmt::Display for ArcString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.str, f)
    }
}

impl PartialEq for ArcString {
    fn eq(&self, other: &ArcString) -> bool {
        PartialEq::eq(&self[..], &other[..])
    }
    fn ne(&self, other: &ArcString) -> bool {
        PartialEq::ne(&self[..], &other[..])
    }
}

impl Serialize for ArcString {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.str)
    }
}

use dynamic_pool::DynamicReset;
use serde::de::{Deserialize, Deserializer};
use std::mem::swap;

impl<'de> Deserialize<'de> for ArcString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(ArcString::new(s))
    }
}
