use crate::typ::{ArcMutex, ArcMutexTokio, ArcRwLock};
use crate::util::ArcString;
use anyhow::anyhow;
use anyhow::Result;
use bytes::Bytes;
use bytes::{Buf, BytesMut};
use std::collections::VecDeque;
use std::fs::File;
use std::io::IoSlice;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Clone)]
pub struct FileUniq {
    pub uniq: u64,
}

impl PartialEq for FileUniq {
    fn eq(&self, other: &Self) -> bool {
        self.uniq == other.uniq
    }
}

impl Default for FileUniq {
    fn default() -> Self {
        FileUniq { uniq: 0 }
    }
}

impl FileUniq {
    pub fn new(file: &File) -> Result<Self> {
        #[cfg(windows)]
        let uniq = Self::window_uniq(file)?;
        #[cfg(unix)]
        let uniq = Self::linux_uniq(file)?;
        Ok(FileUniq { uniq })
    }

    pub fn get_uniq(&self) -> u64 {
        self.uniq
    }

    #[cfg(not(any(unix, windows)))]
    pub fn other_uniq(file: &File) -> Result<u64> {
        Ok(0)
    }

    #[cfg(windows)]
    pub fn window_uniq(file: &File) -> Result<u64> {
        use std::os::windows::io::AsRawHandle;
        use winapi::um::fileapi::GetFileInformationByHandle;
        use winapi::um::fileapi::BY_HANDLE_FILE_INFORMATION;
        use winapi::um::winnt::HANDLE;

        // 获取文件句柄
        let handle = file.as_raw_handle() as HANDLE;

        // 定义 BY_HANDLE_FILE_INFORMATION 结构体
        let mut cache_file_info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };

        // 调用 WinAPI 函数获取文件信息
        let result = unsafe {
            GetFileInformationByHandle(
                handle,
                &mut cache_file_info as *mut BY_HANDLE_FILE_INFORMATION as *mut _,
            )
        };

        if result == 0 {
            return Err(anyhow!("Failed to get file information"));
        }

        // 返回文件 inode 号码
        Ok(
            ((cache_file_info.nFileIndexHigh as u64) << 32)
                | (cache_file_info.nFileIndexLow as u64),
        )
    }

    #[cfg(unix)]
    pub fn linux_uniq(file: &File) -> Result<u64> {
        use std::os::unix::fs::MetadataExt;
        let metadata = file
            .metadata()
            .map_err(|e| anyhow!("err:linux_uniq =>e:{}", e))?;
        Ok(metadata.ino())
    }
}

pub struct FileExtFix {
    pub file_fd: i32,
    pub file_uniq: FileUniq,
    pub is_directio: AtomicBool,
    pub is_del_file: AtomicBool,
}

impl FileExtFix {
    pub fn default() -> Self {
        Self::new(0, FileUniq::default())
    }
    pub fn new_arc(file_fd: i32, file_uniq: FileUniq) -> Arc<Self> {
        Arc::new(Self::new(file_fd, file_uniq))
    }

    pub fn new(file_fd: i32, file_uniq: FileUniq) -> Self {
        FileExtFix {
            file_fd,
            file_uniq,
            is_directio: AtomicBool::new(false),
            is_del_file: AtomicBool::new(false),
        }
    }

    pub fn new_fd(file_fd: i32) -> Self {
        FileExtFix {
            file_fd,
            file_uniq: FileUniq::default(),
            is_directio: AtomicBool::new(false),
            is_del_file: AtomicBool::new(false),
        }
    }

    pub fn is_sendfile(&self) -> bool {
        self.file_fd > 0 && !self.is_directio.load(Ordering::SeqCst)
    }

    #[cfg(unix)]
    pub fn directio_on(&self) -> anyhow::Result<()> {
        use crate::util::directio_on;
        directio_on(self.file_fd)?;
        self.is_directio.store(true, Ordering::SeqCst);
        Ok(())
    }

    pub fn is_uniq_eq(&self, file_share: &Arc<FileExtFix>) -> bool {
        self.file_uniq == file_share.file_uniq
    }

    pub fn is_uniq_and_fd_eq(&self, file_share: &Arc<FileExtFix>) -> bool {
        self.file_uniq == file_share.file_uniq && self.file_fd == file_share.file_fd
    }
}

pub struct FileExt {
    pub async_lock: ArcMutexTokio<()>,
    pub file: ArcMutex<File>,
    pub fix: Arc<FileExtFix>,
    pub file_path: ArcRwLock<ArcString>,
    pub file_len: u64,
}

impl FileExt {
    pub fn default_arc() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn default() -> Self {
        FileExt {
            async_lock: ArcMutexTokio::default(),
            file: ArcMutex::default(),
            fix: Arc::new(FileExtFix::default()),
            file_path: ArcRwLock::default(),
            file_len: 0,
        }
    }
    pub fn new_fd_arc(file_fd: i32) -> Arc<Self> {
        Arc::new(Self::new_fd(file_fd))
    }
    pub fn new_fd(file_fd: i32) -> Self {
        FileExt {
            async_lock: ArcMutexTokio::default(),
            file: ArcMutex::default(),
            fix: Arc::new(FileExtFix::new_fd(file_fd)),
            file_path: ArcRwLock::default(),
            file_len: 0,
        }
    }

    pub fn is_sendfile(&self) -> bool {
        self.fix.is_sendfile()
    }

    #[cfg(unix)]
    pub fn directio_on(&self) -> anyhow::Result<()> {
        self.fix.directio_on()
    }

    pub fn is_uniq_eq(&self, fix: &Arc<FileExtFix>) -> bool {
        self.fix.is_uniq_eq(fix)
    }

    pub fn is_uniq_and_fd_eq(&self, fix: &Arc<FileExtFix>) -> bool {
        self.fix.is_uniq_and_fd_eq(fix)
    }

    pub fn unlink(&self, file_id: Option<u64>) {
        self.fix.is_del_file.store(false, Ordering::SeqCst);
        if self.file_path.is_none() {
            return;
        }
        let file_path = self.file_path.get().clone();
        if !Path::new(file_path.as_str()).exists() {
            return;
        }

        if let Err(e) = unlink(file_path.as_str(), file_id) {
            self.fix.is_del_file.store(true, Ordering::SeqCst);
            log::trace!(target: "ext_tmp", "err:unlink file(wait count == 1 start remove_file):{} => e:{}",
                                file_path.as_str(), e);
        }
    }
    pub fn create_sparse_file(&self, file_size: u64) -> std::io::Result<()> {
        create_sparse_file(&*self.file.get(), file_size, Some(self.fix.file_fd))
    }
}

impl Drop for FileExt {
    fn drop(&mut self) {
        if self.fix.is_del_file.load(Ordering::SeqCst) && self.file.strong_count() == 1 {
            if self.file_path.is_none() {
                return;
            }
            let file_path = self.file_path.get().clone();
            if !Path::new(file_path.as_str()).exists() {
                return;
            }

            if self.file.is_some() {
                let file = unsafe { self.file.take() };
                drop(file);
            }
            log::trace!(target: "ext", "remove_file:{}", file_path.as_str());
            if let Err(e) = unlink(file_path.as_str(), None) {
                log::error!(
                    "err:remove_file file => file_path:{}, e:{}",
                    file_path.as_str(),
                    e
                );
            }
        }
    }
}

/*
pub struct FileExt {
    pub async_lock_w: ArcMutexTokio<()>,
    pub file_w: ArcMutex<File>,
    pub file_w_fd: i32,
    pub file_w_uniq: FileUniq,
    pub async_lock_r: ArcMutexTokio<()>,
    pub file_r: ArcMutex<File>,
    pub file_r_fd: i32,
    pub file_r_uniq: FileUniq,
    pub file_path: ArcString,
    pub is_del_file: bool,
}

impl FileExt {
    pub fn unlink(&mut self) {
        self.is_del_file = false;
        if let Err(e) = unlink(self.file_path.as_str()) {
            self.is_del_file = true;
            log::trace!(target: "ext", "err:unlink file(wait count == 1 start remove_file):{} => e:{}",
                self.file_path.as_str(), e);
        }
    }
    pub fn create_sparse_file(&self, file_size: u64) -> std::io::Result<()> {
        create_sparse_file(&*self.file_w.get(), file_size, Some(self.file_w_fd))
    }
}
impl Drop for FileExt {
    fn drop(&mut self) {
        if self.is_del_file && self.file_w.strong_count() == 1 && self.file_r.strong_count() == 1 {
            if self.file_w.is_none() || self.file_r.is_none() {
                log::error!(
                    "err:self.file_w.is_none() || self.file_r.is_none() => file_path:{}",
                    self.file_path.as_str()
                );
                return;
            }
            log::trace!(target: "ext", "remove_file:{}", self.file_path.as_str());
            let file_w = unsafe { self.file_w.take() };
            drop(file_w);
            let file_r = unsafe { self.file_r.take() };
            drop(file_r);
            if let Err(e) = std::fs::remove_file(self.file_path.as_str()) {
                log::error!(
                    "err:remove_file file => file_path:{}, e:{}",
                    self.file_path.as_str(),
                    e
                );
            }
        }
    }
}

 */

pub fn unlink(file_path: &str, file_id: Option<u64>) -> std::io::Result<()> {
    if !Path::new(file_path).exists() {
        return Ok(());
    }

    let file_path_tmp = if file_id.is_some() {
        format!("{}_{}_tmp", file_path, file_id.unwrap())
    } else {
        format!("{}_tmp", file_path)
    };

    std::fs::rename(file_path, &file_path_tmp)?;

    let ret = {
        #[cfg(unix)]
        {
            log::trace!(target: "ext", "unlink:{}", file_path_tmp);
            use std::ffi::CString;
            let c_filename = CString::new(file_path_tmp.as_bytes())?;
            let res = unsafe { libc::unlink(c_filename.as_ptr()) };
            if res != 0 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(())
            }
        }
        #[cfg(not(unix))]
        {
            std::fs::remove_file(&file_path_tmp)
        }
    };
    if let Err(e) = ret {
        std::fs::rename(&file_path_tmp, file_path)?;
        return Err(e);
    }
    return Ok(());
}

pub fn create_sparse_file(file: &File, file_size: u64, _fd: Option<i32>) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use libc::ftruncate;
        let fd = if _fd.is_some() {
            _fd.unwrap()
        } else {
            use std::os::unix::io::AsRawFd;
            file.as_raw_fd()
        };
        // 使用 ftruncate 函数来创建一个空洞文件
        log::trace!(target: "ext", "ftruncate fd:{}, file_size:{}", fd, file_size);
        let ret = unsafe { ftruncate(fd, file_size as i64) };

        // 检查 ftruncate 的返回值
        if ret != 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    #[cfg(not(unix))]
    file.set_len(file_size)?;
    Ok(())
}

pub struct FileCacheBytes {
    datas: VecDeque<Bytes>,
    size: usize,
    page_size: usize,
}

impl FileCacheBytes {
    #[inline]
    pub fn remaining(&self) -> usize {
        self.size
    }

    #[inline]
    pub fn advance(&mut self, mut cnt: usize) {
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
            return;
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

    pub fn page_chunks_copy(&mut self, is_end: bool) -> Option<FileCacheBytes> {
        if self.remaining() <= 0 {
            return None;
        }

        if is_end {
            return Some(FileCacheBytes {
                datas: self.datas.clone(),
                size: self.size,
                page_size: self.page_size,
            });
        }

        let mut page_size = (self.remaining() / self.page_size) * self.page_size;
        if page_size <= 0 {
            return None;
        }

        let size = page_size;
        let mut datas = VecDeque::with_capacity(self.datas.len());

        for buf in &self.datas {
            if page_size <= 0 {
                break;
            }

            let buf = if buf.len() > page_size {
                buf.slice(0..page_size)
            } else {
                buf.clone()
            };
            page_size -= buf.len();
            datas.push_back(buf);
        }

        return Some(FileCacheBytes {
            datas,
            size,
            page_size: self.page_size,
        });
    }

    pub fn chunk_all_bytes(&self) -> Bytes {
        if self.datas.len() == 1 {
            return self.datas.front().unwrap().clone();
        }

        let mut cache = BytesMut::with_capacity(self.size);
        for data in &self.datas {
            cache.extend_from_slice(&data.as_ref());
        }
        cache.freeze()
    }

    pub fn chunks_vectored<'a>(&'a self, dst: &mut [IoSlice<'a>]) -> usize {
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
}

impl FileCacheBytes {
    pub fn new(page_size: usize) -> Self {
        FileCacheBytes {
            datas: VecDeque::with_capacity(10),
            size: 0,
            page_size,
        }
    }

    pub fn push_back(&mut self, data: Bytes) {
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
