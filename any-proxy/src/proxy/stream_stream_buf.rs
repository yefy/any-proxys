use any_base::file_ext::FileExt;
use any_base::io::async_write_msg::BufFile;
use any_base::io::async_write_msg::{MsgWriteBuf, MsgWriteBufFile};
use any_base::util::StreamReadMsg;
use bytes::Buf;
use bytes::{Bytes, BytesMut};
use std::cmp::min;
use std::collections::{LinkedList, VecDeque};
use std::io::IoSlice;
use std::mem::swap;
use std::sync::Arc;

pub trait StreamStreamBuf {
    type Obj;
    fn remaining(&self) -> usize;

    fn chunk(&mut self, size: usize) -> &[u8];

    fn chunk_all(&mut self) -> &[u8];

    fn advance(&mut self, cnt: usize);

    fn has_remaining(&self) -> bool {
        self.remaining() > 0
    }

    fn chunk_bytes(&mut self, size: usize) -> Bytes;

    fn chunk_bytes_all(&mut self) -> Bytes;

    fn chunks_vectored<'a>(&'a self, dst: &mut [IoSlice<'a>]) -> usize;

    fn split_to(&mut self, size: usize) -> Self::Obj;
}

#[derive(Clone)]
pub struct StreamStreamFile {
    pub file_ext: Arc<FileExt>,
    pub seek: u64,
    pub size: usize,
    pub notify_tx: Option<Vec<tokio::sync::mpsc::UnboundedSender<()>>>,
}

impl std::fmt::Display for StreamStreamFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "file_fd: {}", self.file_ext.fix.file_fd)?;
        write!(f, "file_uniq: {}", self.file_ext.fix.file_uniq.get_uniq())?;
        write!(f, "seek: {}", self.seek)?;
        write!(f, "size: {}", self.size)
    }
}

impl std::fmt::Debug for StreamStreamFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamStreamFile")
            .field("file_fd", &self.file_ext.fix.file_fd)
            .field("file_uniq", &self.file_ext.fix.file_uniq.get_uniq())
            .field("seek", &self.seek)
            .field("size", &self.size)
            .finish()
    }
}

impl StreamStreamFile {
    pub fn new(file_ext: Arc<FileExt>, seek: u64, size: usize) -> StreamStreamFile {
        StreamStreamFile {
            file_ext,
            seek,
            size,
            notify_tx: Some(Vec::new()),
        }
    }

    pub fn from_file(buf_file: MsgWriteBufFile) -> StreamStreamFile {
        let (file_ext, seek, size, _, notify_tx) = buf_file.split();
        StreamStreamFile {
            file_ext,
            seek,
            size,
            notify_tx,
        }
    }

    pub fn to_msg_write_buf(
        &mut self,
    ) -> (
        Arc<FileExt>,
        u64,
        usize,
        Option<Vec<tokio::sync::mpsc::UnboundedSender<()>>>,
    ) {
        return (
            self.file_ext.clone(),
            self.seek,
            self.size,
            self.notify_tx.take(),
        );
    }
}

impl BufFile for StreamStreamFile {
    #[inline]
    fn is_file(&self) -> bool {
        true
    }
    #[inline]
    fn remaining_(&self) -> usize {
        self.size
    }

    #[inline]
    fn advance_(&mut self, cnt: usize) {
        assert!(
            cnt <= self.remaining_(),
            "cannot advance past `remaining`: {:?} <= {:?}",
            cnt,
            self.remaining_(),
        );

        if cnt > self.remaining_() {
            panic!("cnt > self.remaining()");
        }

        self.seek += cnt as u64;
        self.size -= cnt;

        if self.size == 0 {
            let notify_tx = self.notify_tx.take();
            if notify_tx.is_some() {
                for tx in notify_tx.unwrap() {
                    let _ = tx.send(());
                }
            }
        }
    }
}

pub struct StreamStreamBytes {
    datas: LinkedList<Bytes>,
    size: usize,
    cache: Vec<u8>,
    cache_all: Cursor,
}

impl BufFile for StreamStreamBytes {
    fn is_file(&self) -> bool {
        false
    }

    fn remaining_(&self) -> usize {
        self.remaining()
    }

    fn advance_(&mut self, cnt: usize) {
        self.advance(cnt)
    }
}

impl StreamStreamBuf for StreamStreamBytes {
    type Obj = StreamStreamBytes;
    #[inline]
    fn remaining(&self) -> usize {
        self.size
    }

    #[inline]
    fn chunk(&mut self, mut size: usize) -> &[u8] {
        if self.remaining() <= 0 || size > self.remaining() {
            panic!("self.remaining() <= 0");
        }

        if size == 0 {
            let first = self.datas.front().unwrap();
            return first.as_ref();
        }

        if size <= self.datas.front().unwrap().len() {
            return &self.datas.front().unwrap().as_ref()[0..size];
        }

        if size == self.remaining_() {
            return &self.chunk_all()[0..size];
        }
        if self.cache_all.is_some() {
            return &self.chunk_all()[0..size];
        }

        if self.cache.capacity() < size {
            self.cache.resize(size, 0);
        }
        unsafe { self.cache.set_len(0) };

        for data in &self.datas {
            let n = min(size, data.len());
            size -= n;
            if n == 0 {
                break;
            }
            self.cache.extend_from_slice(&data.as_ref()[0..n]);
        }
        return self.cache.as_slice();
    }

    #[inline]
    fn chunk_all(&mut self) -> &[u8] {
        if self.datas.len() == 1 {
            return self.datas.front().unwrap().as_ref();
        }

        if self.cache_all.is_some() {
            return self.cache_all.chunk();
        }

        self.cache_all.resize(self.remaining_());
        for data in &self.datas {
            self.cache_all.extend_from_slice(&data.as_ref());
        }
        self.cache_all.chunk()
    }

    #[inline]
    fn advance(&mut self, mut cnt: usize) {
        assert!(
            cnt <= self.remaining(),
            "cannot advance past `remaining`: {:?} <= {:?}",
            cnt,
            self.remaining(),
        );

        if cnt > self.remaining() || self.remaining() <= 0 {
            panic!("cnt > self.remaining()")
        }
        self.size -= cnt;
        if self.size <= 0 {
            self.datas.clear();
            self.cache.clear();
            self.cache_all.reset();
            return;
        }
        if self.cache_all.is_some() {
            self.cache_all.advance(cnt);
        }

        loop {
            let first = self.datas.front_mut().unwrap();
            if cnt >= first.len() {
                cnt -= first.len();
                self.datas.pop_front();
            } else {
                first.advance(cnt);
                cnt = 0;
            }
            if cnt <= 0 {
                return;
            }
        }
    }

    fn chunk_bytes(&mut self, mut size: usize) -> Bytes {
        if self.remaining() <= 0 || size > self.remaining() {
            panic!("self.remaining() <= 0");
        }

        if size == 0 {
            let first = self.datas.front().unwrap();
            return first.clone();
        }

        let first = self.datas.front().unwrap();
        if size <= first.len() {
            return first.slice(0..size);
        }

        if size == self.remaining_() {
            return self.chunk_bytes_all();
        }

        if self.cache_all.is_some() {
            return self.chunk_bytes_all().slice(0..size);
        }

        let mut cache = Vec::with_capacity(size);
        for data in &self.datas {
            let n = min(size, data.len());
            size -= n;
            if n == 0 {
                break;
            }
            cache.extend_from_slice(&data.as_ref()[0..n]);
        }
        return cache.into();
    }

    fn chunk_bytes_all(&mut self) -> Bytes {
        if self.datas.len() == 1 {
            return self.datas.front().unwrap().clone();
        }
        self.chunk_all();
        self.cache_all.chunk_bytes()
    }

    fn chunks_vectored<'a>(&'a self, dst: &mut [IoSlice<'a>]) -> usize {
        if dst.is_empty() {
            return 0;
        }
        let mut vecs = 0;
        for buf in &self.datas {
            vecs += buf.chunks_vectored(&mut dst[vecs..]);
            if vecs == dst.len() {
                break;
            }
        }
        vecs
    }

    fn split_to(&mut self, mut size: usize) -> StreamStreamBytes {
        if size <= 0 || size >= self.remaining() {
            panic!("size <= 0 || size >= self.remaining()")
        }
        self.size -= size;
        if self.cache_all.is_some() {
            if self.size <= 0 {
                self.cache_all.reset();
            } else {
                self.cache_all.advance(size);
            }
        }
        let mut datas = VecDeque::with_capacity(10);
        loop {
            let len = self.datas.front().unwrap().len();
            if size >= len {
                datas.push_back(self.datas.pop_front().unwrap());
                size -= len;
            } else {
                datas.push_back(self.datas.front_mut().unwrap().split_to(size));
                size = 0;
            }

            if size <= 0 {
                let mut bytes = StreamStreamBytes::new();
                bytes.push_back_deque(datas);
                return bytes;
            }
        }
    }
}

impl StreamStreamBytes {
    pub fn new() -> Self {
        StreamStreamBytes {
            datas: LinkedList::new(),
            size: 0,
            cache: Vec::with_capacity(0),
            cache_all: Cursor::new(0),
        }
    }

    pub fn push_back(&mut self, data: Bytes) {
        if self.cache_all.is_some() {
            self.cache_all.extend_from_slice(data.as_ref());
        }
        self.size += data.len();
        self.datas.push_back(data);
    }

    pub fn push_back_vec(&mut self, datas: Vec<Bytes>) {
        for data in datas {
            self.push_back(data);
        }
    }

    pub fn push_back_deque(&mut self, datas: VecDeque<Bytes>) {
        for data in datas {
            self.push_back(data);
        }
    }
}

pub struct StreamStreamBuffer {
    pub data: BytesMut,
    pub size: usize,
    pub max_size: usize,
}

impl BufFile for StreamStreamBuffer {
    fn is_file(&self) -> bool {
        false
    }

    fn remaining_(&self) -> usize {
        self.remaining()
    }

    fn advance_(&mut self, cnt: usize) {
        self.advance(cnt)
    }
}

impl StreamStreamBuf for StreamStreamBuffer {
    type Obj = StreamStreamBuffer;
    fn remaining(&self) -> usize {
        self.size as usize
    }

    fn chunk(&mut self, size: usize) -> &[u8] {
        if size <= 0 || size > self.remaining() {
            panic!("size <= 0 || size > self.remaining()")
        }

        &self.chunk_all()[0..size as usize]
    }

    fn chunk_all(&mut self) -> &[u8] {
        &self.data.as_ref()[0..self.size]
    }

    fn advance(&mut self, cnt: usize) {
        assert!(
            cnt <= self.remaining(),
            "cannot advance past `remaining`: {:?} <= {:?}",
            cnt,
            self.remaining(),
        );

        if cnt > self.remaining() || self.remaining() <= 0 {
            panic!("cnt > self.remaining()")
        }

        self.data.advance(cnt);
        self.size -= cnt;
        self.max_size -= cnt;
    }

    fn has_remaining(&self) -> bool {
        self.remaining() > 0
    }

    fn chunk_bytes(&mut self, size: usize) -> Bytes {
        self.chunk(size).to_vec().into()
    }

    fn chunk_bytes_all(&mut self) -> Bytes {
        self.chunk_all().to_vec().into()
    }

    fn chunks_vectored<'a>(&'a self, dst: &mut [IoSlice<'a>]) -> usize {
        self.data.chunks_vectored(dst)
    }

    fn split_to(&mut self, size: usize) -> StreamStreamBuffer {
        if size <= 0 || size >= self.remaining() {
            panic!("size <= 0 || size >= self.remaining()")
        }
        self.size -= size;
        self.max_size -= size;

        StreamStreamBuffer {
            data: self.data.split_to(size),
            size,
            max_size: size,
        }
    }
}

impl StreamStreamBuffer {
    pub fn new(read_buffer_size: usize) -> Self {
        StreamStreamBuffer {
            data: BytesMut::zeroed(read_buffer_size),
            size: 0,
            max_size: read_buffer_size,
        }
    }

    pub fn to_bytes(mut self) -> Bytes {
        self.data.split_to(self.size as usize).freeze()
    }

    pub fn free_data_mut(&mut self) -> &mut [u8] {
        return &mut self.data.as_mut()[self.size as usize..self.max_size as usize];
    }

    pub fn add_size(&mut self, size: usize) {
        self.size += size;
    }

    pub fn max_size(&self) -> usize {
        self.max_size as usize
    }

    pub fn is_full(&self) -> bool {
        self.size == self.max_size
    }

    pub fn resize_max_size(&mut self, size: usize) {
        if size < self.size {
            return;
        }

        if self.data.capacity() < size {
            self.data.resize(size, 0);
        }

        unsafe { self.data.set_len(size) };
        self.max_size = size;
    }
}

pub enum StreamStreamReadBuf<'a> {
    Bytes(&'a mut StreamStreamBytes),
    Buffer(&'a mut StreamStreamBuffer),
}

pub struct StreamStreamCacheReadBuf<'a> {
    pub data: StreamStreamReadBuf<'a>,
    pub is_cache: bool,
    pub is_flush: bool,
}

impl StreamStreamCacheReadBuf<'_> {
    pub fn from_bytes(
        data: &mut StreamStreamBytes,
        is_cache: bool,
        is_flush: bool,
    ) -> StreamStreamCacheReadBuf {
        StreamStreamCacheReadBuf {
            data: StreamStreamReadBuf::Bytes(data),
            is_cache,
            is_flush,
        }
    }

    pub fn from_buffer(
        data: &mut StreamStreamBuffer,
        is_cache: bool,
        is_flush: bool,
    ) -> StreamStreamCacheReadBuf {
        StreamStreamCacheReadBuf {
            data: StreamStreamReadBuf::Buffer(data),
            is_cache,
            is_flush,
        }
    }

    pub fn bytes(&mut self) -> Option<&mut StreamStreamBytes> {
        match &mut self.data {
            StreamStreamReadBuf::Bytes(data) => Some(data),
            StreamStreamReadBuf::Buffer(_data) => None,
        }
    }

    pub fn buffer(&mut self) -> Option<&mut StreamStreamBuffer> {
        match &mut self.data {
            StreamStreamReadBuf::Bytes(_data) => None,
            StreamStreamReadBuf::Buffer(data) => Some(data),
        }
    }
}

impl StreamStreamBuf for StreamStreamCacheReadBuf<'_> {
    type Obj = StreamStreamCacheRead;
    fn remaining(&self) -> usize {
        match &self.data {
            StreamStreamReadBuf::Bytes(data) => data.remaining(),
            StreamStreamReadBuf::Buffer(data) => data.remaining(),
        }
    }

    fn chunk(&mut self, size: usize) -> &[u8] {
        match &mut self.data {
            StreamStreamReadBuf::Bytes(data) => data.chunk(size),
            StreamStreamReadBuf::Buffer(data) => data.chunk(size),
        }
    }

    fn chunk_all(&mut self) -> &[u8] {
        match &mut self.data {
            StreamStreamReadBuf::Bytes(data) => data.chunk_all(),
            StreamStreamReadBuf::Buffer(data) => data.chunk_all(),
        }
    }

    fn advance(&mut self, cnt: usize) {
        match &mut self.data {
            StreamStreamReadBuf::Bytes(data) => data.advance(cnt),
            StreamStreamReadBuf::Buffer(data) => data.advance(cnt),
        }
    }

    fn has_remaining(&self) -> bool {
        self.remaining() > 0
    }

    fn chunk_bytes(&mut self, size: usize) -> Bytes {
        match &mut self.data {
            StreamStreamReadBuf::Bytes(data) => data.chunk_bytes(size),
            StreamStreamReadBuf::Buffer(data) => data.chunk_bytes(size),
        }
    }

    fn chunk_bytes_all(&mut self) -> Bytes {
        match &mut self.data {
            StreamStreamReadBuf::Bytes(data) => data.chunk_bytes_all(),
            StreamStreamReadBuf::Buffer(data) => data.chunk_bytes_all(),
        }
    }

    fn chunks_vectored<'a>(&'a self, dst: &mut [IoSlice<'a>]) -> usize {
        match &self.data {
            StreamStreamReadBuf::Bytes(data) => data.chunks_vectored(dst),
            StreamStreamReadBuf::Buffer(data) => data.chunks_vectored(dst),
        }
    }

    fn split_to(&mut self, size: usize) -> StreamStreamCacheRead {
        match &mut self.data {
            StreamStreamReadBuf::Bytes(data) => {
                let mut data = StreamStreamCacheRead::from_bytes(data.split_to(size));
                data.is_cache = self.is_cache;
                data.is_flush = self.is_flush;
                data
            }
            StreamStreamReadBuf::Buffer(data) => {
                let mut data = StreamStreamCacheRead::from_buffer(data.split_to(size));
                data.is_cache = self.is_cache;
                data.is_flush = self.is_flush;
                data
            }
        }
    }
}

pub enum StreamStreamRead {
    Bytes(StreamStreamBytes),
    Buffer(StreamStreamBuffer),
    File(StreamStreamFile),
}

pub struct StreamStreamCacheRead {
    pub data: StreamStreamRead,
    pub is_cache: bool,
    pub is_flush: bool,
}

impl BufFile for StreamStreamCacheRead {
    #[inline]
    fn is_file(&self) -> bool {
        match &self.data {
            StreamStreamRead::Bytes(data) => data.is_file(),
            StreamStreamRead::Buffer(data) => data.is_file(),
            StreamStreamRead::File(data) => data.is_file(),
        }
    }
    #[inline]
    fn remaining_(&self) -> usize {
        match &self.data {
            StreamStreamRead::Bytes(data) => data.remaining(),
            StreamStreamRead::Buffer(data) => data.remaining(),
            StreamStreamRead::File(data) => data.remaining_(),
        }
    }

    #[inline]
    fn advance_(&mut self, cnt: usize) {
        match &mut self.data {
            StreamStreamRead::Bytes(data) => data.advance(cnt),
            StreamStreamRead::Buffer(data) => data.advance(cnt),
            StreamStreamRead::File(data) => data.advance_(cnt),
        }
    }
}

impl StreamStreamCacheRead {
    pub fn to_cache_write(self) -> StreamStreamCacheWrite {
        let StreamStreamCacheRead {
            data,
            is_cache,
            is_flush: _,
        } = self;
        match data {
            StreamStreamRead::Bytes(data) => StreamStreamCacheWrite::from_bytes(data, is_cache),
            StreamStreamRead::Buffer(data) => {
                let mut bytes = StreamStreamBytes::new();
                bytes.push_back(data.to_bytes());
                StreamStreamCacheWrite::from_bytes(bytes, is_cache)
            }
            StreamStreamRead::File(data) => StreamStreamCacheWrite::from_file(data, is_cache),
        }
    }
    pub fn from_bytes(data: StreamStreamBytes) -> StreamStreamCacheRead {
        StreamStreamCacheRead {
            data: StreamStreamRead::Bytes(data),
            is_cache: false,
            is_flush: false,
        }
    }

    pub fn from_buffer(data: StreamStreamBuffer) -> StreamStreamCacheRead {
        StreamStreamCacheRead {
            data: StreamStreamRead::Buffer(data),
            is_cache: false,
            is_flush: false,
        }
    }

    pub fn from_file(data: StreamStreamFile) -> StreamStreamCacheRead {
        StreamStreamCacheRead {
            data: StreamStreamRead::File(data),
            is_cache: false,
            is_flush: false,
        }
    }

    pub fn file(&mut self) -> Option<&mut StreamStreamFile> {
        match &mut self.data {
            StreamStreamRead::Bytes(_data) => None,
            StreamStreamRead::Buffer(_data) => None,
            StreamStreamRead::File(data) => Some(data),
        }
    }

    pub fn buffer(&mut self) -> Option<StreamStreamCacheReadBuf> {
        match &mut self.data {
            StreamStreamRead::Bytes(data) => Some(StreamStreamCacheReadBuf::from_bytes(
                data,
                self.is_cache,
                self.is_flush,
            )),
            StreamStreamRead::Buffer(data) => Some(StreamStreamCacheReadBuf::from_buffer(
                data,
                self.is_cache,
                self.is_flush,
            )),
            StreamStreamRead::File(_data) => None,
        }
    }

    pub fn bytes(&mut self) -> Option<&mut StreamStreamBytes> {
        match &mut self.data {
            StreamStreamRead::Bytes(data) => Some(data),
            StreamStreamRead::Buffer(_data) => None,
            StreamStreamRead::File(_data) => None,
        }
    }

    pub fn buf(&mut self) -> Option<&mut StreamStreamBuffer> {
        match &mut self.data {
            StreamStreamRead::Bytes(_data) => None,
            StreamStreamRead::Buffer(data) => Some(data),
            StreamStreamRead::File(_data) => None,
        }
    }

    pub fn resize_max_size(&mut self, size: usize) {
        match &mut self.data {
            StreamStreamRead::Bytes(_data) => {}
            StreamStreamRead::Buffer(data) => data.resize_max_size(size),
            StreamStreamRead::File(_data) => {}
        }
    }

    pub fn max_size(&self) -> usize {
        match &self.data {
            StreamStreamRead::Bytes(_data) => 0,
            StreamStreamRead::Buffer(data) => data.max_size(),
            StreamStreamRead::File(_data) => 0,
        }
    }

    pub fn is_full(&self) -> bool {
        match &self.data {
            StreamStreamRead::Bytes(_data) => false,
            StreamStreamRead::Buffer(data) => data.is_full(),
            StreamStreamRead::File(_data) => true,
        }
    }

    pub fn push_back_ream_msg(&mut self, msg: StreamReadMsg) -> Option<StreamStreamCacheRead> {
        let mut buf = self.buffer().unwrap();
        let buf = buf.bytes().unwrap();
        let (data, file) = msg.split();
        buf.push_back_deque(data);

        let file = if file.is_some() {
            self.is_flush = true;
            let file = StreamStreamFile::from_file(file.unwrap());
            let mut file = StreamStreamCacheRead::from_file(file);
            file.is_cache = self.is_cache;
            file.is_flush = true;
            Some(file)
        } else {
            None
        };
        return file;
    }

    pub fn add_size(&mut self, size: usize) {
        let mut buf = self.buffer().unwrap();
        let buf = buf.buffer().unwrap();
        buf.add_size(size);
    }
}

pub enum StreamStreamWrite {
    Bytes(StreamStreamBytes),
    File(StreamStreamFile),
}

pub struct StreamStreamCacheWrite {
    pub data: StreamStreamWrite,
    pub is_cache: bool,
}

impl StreamStreamCacheWrite {
    pub fn from_bytes(data: StreamStreamBytes, is_cache: bool) -> StreamStreamCacheWrite {
        StreamStreamCacheWrite {
            data: StreamStreamWrite::Bytes(data),
            is_cache,
        }
    }

    pub fn from_file(data: StreamStreamFile, is_cache: bool) -> StreamStreamCacheWrite {
        StreamStreamCacheWrite {
            data: StreamStreamWrite::File(data),
            is_cache,
        }
    }

    pub fn file(&self) -> Option<&StreamStreamFile> {
        match &self.data {
            StreamStreamWrite::Bytes(_data) => None,
            StreamStreamWrite::File(data) => Some(data),
        }
    }

    pub fn file_mut(&mut self) -> Option<&mut StreamStreamFile> {
        match &mut self.data {
            StreamStreamWrite::Bytes(_data) => None,
            StreamStreamWrite::File(data) => Some(data),
        }
    }

    pub fn to_file(self) -> Option<StreamStreamFile> {
        match self.data {
            StreamStreamWrite::Bytes(_data) => None,
            StreamStreamWrite::File(data) => Some(data),
        }
    }

    pub fn bytes_mut(&mut self) -> Option<&mut StreamStreamBytes> {
        match &mut self.data {
            StreamStreamWrite::Bytes(data) => Some(data),
            StreamStreamWrite::File(_data) => None,
        }
    }

    pub fn bytes(&self) -> Option<&StreamStreamBytes> {
        match &self.data {
            StreamStreamWrite::Bytes(data) => Some(data),
            StreamStreamWrite::File(_data) => None,
        }
    }

    pub fn to_bytes(self) -> Option<StreamStreamBytes> {
        match self.data {
            StreamStreamWrite::Bytes(data) => Some(data),
            StreamStreamWrite::File(_data) => None,
        }
    }

    // pub fn msg_write_buf(&mut self) -> MsgWriteBuf {
    //     match &mut self.data {
    //         StreamStreamWrite::Bytes(data) => MsgWriteBuf::from_bytes(data.chunk_bytes_all()),
    //         StreamStreamWrite::File(data) => {
    //             let (fr, file_fd, seek, size) = data.copy();
    //             MsgWriteBuf::from_file(fr, file_fd, seek, size)
    //         }
    //     }
    // }
}

impl BufFile for StreamStreamCacheWrite {
    #[inline]
    fn is_file(&self) -> bool {
        match &self.data {
            StreamStreamWrite::Bytes(data) => data.is_file(),
            StreamStreamWrite::File(data) => data.is_file(),
        }
    }
    #[inline]
    fn remaining_(&self) -> usize {
        match &self.data {
            StreamStreamWrite::Bytes(data) => data.remaining(),
            StreamStreamWrite::File(data) => data.remaining_(),
        }
    }

    #[inline]
    fn advance_(&mut self, cnt: usize) {
        match &mut self.data {
            StreamStreamWrite::Bytes(data) => data.advance(cnt),
            StreamStreamWrite::File(data) => data.advance_(cnt),
        }
    }
}

pub struct StreamStreamCacheWriteDeque {
    datas: LinkedList<StreamStreamCacheWrite>,
    size: usize,
    cache: Vec<u8>,
    cache_all: Cursor,
}

impl StreamStreamCacheWriteDeque {
    pub fn is_full(&self) -> bool {
        self.cache_all.is_full()
    }
    pub fn is_file(&self) -> bool {
        if self.is_none() {
            return true;
        }
        return self.datas.front().unwrap().is_file();
    }

    pub fn is_none(&self) -> bool {
        return self.remaining_() <= 0;
    }

    pub fn remaining_(&self) -> usize {
        self.remaining()
    }

    pub fn advance_(&mut self, cnt: usize) -> VecDeque<(bool, usize)> {
        self.advance(cnt)
    }

    #[inline]
    pub fn remaining(&self) -> usize {
        self.size
    }

    #[inline]
    pub fn chunk(&mut self, mut size: usize) -> &[u8] {
        if self.remaining() <= 0 || size > self.remaining() {
            panic!("self.remaining() <= 0");
        }

        if size == 0 {
            let first = self
                .datas
                .front()
                .unwrap()
                .bytes()
                .unwrap()
                .datas
                .front()
                .unwrap();
            return first.as_ref();
        }

        if size
            <= self
                .datas
                .front()
                .unwrap()
                .bytes()
                .unwrap()
                .datas
                .front()
                .unwrap()
                .len()
        {
            return &self
                .datas
                .front()
                .unwrap()
                .bytes()
                .unwrap()
                .datas
                .front()
                .unwrap()
                .as_ref()[0..size];
        }

        if size == self.remaining_() {
            return self.chunk_all();
        }
        if self.cache_all.is_some() {
            return &self.chunk_all()[0..size];
        }

        if self.cache.capacity() < size {
            self.cache.resize(size, 0);
        }
        unsafe { self.cache.set_len(0) };

        for data in &self.datas {
            let data = data.bytes().unwrap();
            for data in &data.datas {
                let n = min(size, data.len());
                size -= n;
                if n == 0 {
                    break;
                }
                self.cache.extend_from_slice(&data.as_ref()[0..n]);
            }
        }
        return self.cache.as_slice();
    }

    #[inline]
    pub fn chunk_all(&mut self) -> &[u8] {
        if self.datas.len() == 1 && self.datas.front().unwrap().bytes().unwrap().datas.len() == 1 {
            return self
                .datas
                .front()
                .unwrap()
                .bytes()
                .unwrap()
                .datas
                .front()
                .unwrap()
                .as_ref();
        }

        if self.cache_all.is_some() {
            return self.cache_all.chunk();
        }

        self.cache_all.resize(self.remaining());
        for data in &self.datas {
            let data = data.bytes().unwrap();
            for data in &data.datas {
                self.cache_all.extend_from_slice(&data.as_ref());
            }
        }
        self.cache_all.chunk()
    }

    #[inline]
    pub fn advance(&mut self, mut cnt: usize) -> VecDeque<(bool, usize)> {
        assert!(
            cnt <= self.remaining(),
            "cannot advance past `remaining`: {:?} <= {:?}",
            cnt,
            self.remaining(),
        );

        if cnt > self.remaining() || self.remaining() <= 0 {
            panic!("cnt > self.remaining()")
        }
        self.size -= cnt;
        if self.cache_all.is_some() {
            self.cache_all.advance(cnt);
        }

        let mut ret = VecDeque::with_capacity(5);
        loop {
            let first = self.datas.front_mut().unwrap();
            if cnt >= first.remaining_() {
                cnt -= first.remaining_();
                ret.push_back((first.is_cache, first.remaining_()));
                self.datas.pop_front();
            } else {
                ret.push_back((first.is_cache, cnt));
                first.advance_(cnt);
                cnt = 0;
            }
            if cnt <= 0 {
                return ret;
            }
        }
    }

    pub fn chunk_bytes(&mut self, mut size: usize) -> Bytes {
        if self.remaining() <= 0 || size > self.remaining() {
            panic!("self.remaining() <= 0");
        }

        if size == 0 {
            let first = self
                .datas
                .front()
                .unwrap()
                .bytes()
                .unwrap()
                .datas
                .front()
                .unwrap();
            return first.clone();
        }

        let first = self
            .datas
            .front()
            .unwrap()
            .bytes()
            .unwrap()
            .datas
            .front()
            .unwrap();
        if size <= first.len() {
            return first.slice(0..size);
        }

        if size == self.remaining_() {
            return self.chunk_bytes_all();
        }

        if self.cache_all.is_some() {
            return self.chunk_bytes_all().slice(0..size);
        }

        let mut cache = Vec::with_capacity(size);
        for data in &self.datas {
            let data = data.bytes().unwrap();
            for data in &data.datas {
                let n = min(size, data.len());
                size -= n;
                if n == 0 {
                    break;
                }
                cache.extend_from_slice(&data.as_ref()[0..n]);
            }
        }
        return cache.into();
    }

    fn chunk_bytes_all(&mut self) -> Bytes {
        if self.datas.len() == 1 && self.datas.front().unwrap().bytes().unwrap().datas.len() == 1 {
            return self
                .datas
                .front()
                .unwrap()
                .bytes()
                .unwrap()
                .datas
                .front()
                .unwrap()
                .clone();
        }
        self.chunk_all();
        self.cache_all.chunk_bytes()
    }

    pub fn chunks_vectored<'a>(&'a self, dst: &mut [IoSlice<'a>]) -> usize {
        if dst.is_empty() {
            return 0;
        }
        let mut vecs = 0;
        for buf in &self.datas {
            vecs += buf.bytes().unwrap().chunks_vectored(&mut dst[vecs..]);
            if vecs == dst.len() {
                break;
            }
        }
        vecs
    }

    pub fn new(max_cache_size: usize) -> Self {
        StreamStreamCacheWriteDeque {
            datas: LinkedList::new(),
            size: 0,
            cache: Vec::with_capacity(0),
            cache_all: Cursor::new(max_cache_size),
        }
    }

    pub fn push_back(
        &mut self,
        write_size: usize,
        mut data: StreamStreamCacheWrite,
    ) -> Option<StreamStreamCacheWrite> {
        loop {
            if self.is_none() {
                break;
            }

            if data.remaining_() >= write_size {
                return Some(data);
            }

            if self.is_file() != data.is_file() {
                return Some(data);
            }

            if self.is_full() {
                return Some(data);
            }

            if self.remaining_() >= write_size {
                return Some(data);
            }

            let sum = self.remaining_() as u64 + data.remaining_() as u64;
            if sum > usize::max_value() as u64 {
                return Some(data);
            }

            break;
        }

        self.size += data.remaining_();
        if data.is_file() {
            let a = self.datas.back_mut();
            if a.is_some() {
                let a = a.unwrap().file_mut().unwrap();
                let b = data.file_mut().unwrap();
                if a.file_ext.is_uniq_and_fd_eq(&b.file_ext.fix) && a.seek + a.size as u64 == b.seek
                {
                    a.size += b.size;
                    if a.notify_tx.is_none() {
                        a.notify_tx = b.notify_tx.take();
                    } else {
                        if b.notify_tx.is_some() {
                            a.notify_tx
                                .as_mut()
                                .unwrap()
                                .extend(b.notify_tx.take().unwrap());
                        }
                    }
                    return None;
                }
            }
            self.datas.push_back(data);
            return None;
        }

        if self.cache_all.is_some() {
            let data = data.bytes_mut().unwrap();
            for data in &data.datas {
                self.cache_all.extend_from_slice(data.as_ref());
            }
        }
        self.datas.push_back(data);
        return None;
    }

    pub fn file(&mut self) -> Option<&mut StreamStreamFile> {
        self.datas.front_mut().unwrap().file_mut()
    }

    pub fn msg_write_buf(&mut self) -> MsgWriteBuf {
        if self.is_file() {
            let (fr, seek, size, notify_tx) = self.file().unwrap().to_msg_write_buf();
            return MsgWriteBuf::from_file(fr, seek, size, notify_tx);
        }

        return MsgWriteBuf::from_bytes(self.chunk_bytes_all());
    }
}

pub(crate) struct Cursor {
    bytes: Vec<u8>,
    pos: usize,
    max: usize,
}

impl Cursor {
    #[inline]
    pub fn new(max: usize) -> Cursor {
        Cursor {
            bytes: Vec::with_capacity(max),
            pos: 0,
            max,
        }
    }

    pub fn is_some(&self) -> bool {
        self.remaining() > 0
    }

    pub fn resize(&mut self, size: usize) {
        if size <= self.max {
            return;
        }
        self.bytes.reserve(size - self.max);
        self.max = size;
    }

    pub fn extend_from_slice(&mut self, data: &[u8]) {
        self.bytes.extend_from_slice(data)
    }

    pub fn is_full(&self) -> bool {
        self.is_some() && self.bytes.len() >= self.max
    }

    #[inline]
    pub fn remaining(&self) -> usize {
        self.bytes.len() - self.pos
    }

    #[inline]
    pub fn chunk(&self) -> &[u8] {
        &self.bytes[self.pos..]
    }

    #[inline]
    pub fn chunk_bytes(&mut self) -> Bytes {
        let mut bytes = Vec::with_capacity(self.max);
        swap(&mut self.bytes, &mut bytes);
        let pos = self.pos;
        self.pos = 0;
        bytes.drain(0..pos);
        Bytes::from(bytes)
    }

    #[inline]
    pub fn reset(&mut self) {
        self.pos = 0;
        unsafe { self.bytes.set_len(0) }
    }

    #[inline]
    pub fn advance(&mut self, cnt: usize) {
        debug_assert!(self.pos + cnt <= self.bytes.len());
        self.pos += cnt;
        if self.pos == self.bytes.len() {
            self.pos = 0;
            unsafe { self.bytes.set_len(0) }
        }
    }
}
