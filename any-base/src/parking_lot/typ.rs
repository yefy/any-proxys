use parking_lot::{Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};
#[cfg(any(debug_assertions, feature = "anylock-time"))]
use std::collections::HashMap;
use std::sync::Arc;
#[cfg(any(debug_assertions, feature = "anylock-time"))]
use std::time::Instant;

#[cfg(any(debug_assertions, feature = "anylock-time"))]
#[cfg(debug_assertions)]
const MAX_ELAPSED_TIME_MIL: u128 = 100;

#[cfg(any(debug_assertions, feature = "anylock-time"))]
#[cfg(not(debug_assertions))]
const MAX_ELAPSED_TIME_MIL: u128 = 50;

#[cfg(any(debug_assertions, feature = "anylock-time"))]
pub struct MutexInfo {
    w_thread_id: Option<std::thread::ThreadId>,
    r_thread_id_map: HashMap<std::thread::ThreadId, usize>,
    is_check_time: bool,
}

pub struct ArcMutexReadGuard<'a, T: 'a> {
    d: MutexGuard<'a, Option<T>>,
    #[cfg(any(debug_assertions, feature = "anylock-time"))]
    info: Arc<Mutex<MutexInfo>>,
    #[cfg(any(debug_assertions, feature = "anylock-time"))]
    start_time: Instant,
}

impl<'a, T: 'a> ArcMutexReadGuard<'a, T> {
    pub fn new(
        d: MutexGuard<'a, Option<T>>,
        #[cfg(any(debug_assertions, feature = "anylock-time"))] info: Arc<Mutex<MutexInfo>>,
    ) -> Self {
        ArcMutexReadGuard {
            d,
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            info,
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            start_time: Instant::now(),
        }
    }
    pub fn is_none(&self) -> bool {
        self.d.is_none()
    }
    pub fn is_some(&self) -> bool {
        !self.is_none()
    }
}

impl<'a, T: 'a> std::ops::Deref for ArcMutexReadGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.d.deref().as_ref().unwrap()
    }
}

impl<'a, T: 'a> Drop for ArcMutexReadGuard<'a, T> {
    #[inline]
    fn drop(&mut self) {
        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        {
            if self.start_time.elapsed().as_millis() > MAX_ELAPSED_TIME_MIL {
                if self.info.lock().is_check_time {
                    panic!("lock panic");
                }
            }
        }
        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        del_write_lock(&self.info);
    }
}

pub struct ArcMutexWriteGuard<'a, T: 'a> {
    d: MutexGuard<'a, Option<T>>,
    #[cfg(any(debug_assertions, feature = "anylock-time"))]
    info: Arc<Mutex<MutexInfo>>,
    #[cfg(any(debug_assertions, feature = "anylock-time"))]
    start_time: Instant,
}

impl<'a, T: 'a> ArcMutexWriteGuard<'a, T> {
    pub fn new(
        d: MutexGuard<'a, Option<T>>,
        #[cfg(any(debug_assertions, feature = "anylock-time"))] info: Arc<Mutex<MutexInfo>>,
    ) -> Self {
        ArcMutexWriteGuard {
            d,
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            info,
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            start_time: Instant::now(),
        }
    }
    pub fn is_none(&self) -> bool {
        self.d.is_none()
    }
    pub fn is_some(&self) -> bool {
        !self.is_none()
    }
}

impl<'a, T: 'a> std::ops::Deref for ArcMutexWriteGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.d.deref().as_ref().unwrap()
    }
}

impl<'a, T: 'a> std::ops::DerefMut for ArcMutexWriteGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.d.deref_mut().as_mut().unwrap()
    }
}

impl<'a, T: 'a> Drop for ArcMutexWriteGuard<'a, T> {
    #[inline]
    fn drop(&mut self) {
        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        {
            if self.start_time.elapsed().as_millis() > MAX_ELAPSED_TIME_MIL {
                if self.info.lock().is_check_time {
                    panic!("lock panic");
                }
            }
        }
        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        del_write_lock(&self.info);
    }
}

pub struct ArcMutexReadOptionGuard<'a, T: 'a> {
    d: MutexGuard<'a, T>,
    #[cfg(any(debug_assertions, feature = "anylock-time"))]
    info: Arc<Mutex<MutexInfo>>,
    #[cfg(any(debug_assertions, feature = "anylock-time"))]
    start_time: Instant,
}

impl<'a, T: 'a> ArcMutexReadOptionGuard<'a, T> {
    pub fn new(
        d: MutexGuard<'a, T>,
        #[cfg(any(debug_assertions, feature = "anylock-time"))] info: Arc<Mutex<MutexInfo>>,
    ) -> Self {
        ArcMutexReadOptionGuard {
            d,
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            info,
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            start_time: Instant::now(),
        }
    }
}

impl<'a, T: 'a> std::ops::Deref for ArcMutexReadOptionGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.d.deref()
    }
}

impl<'a, T: 'a> Drop for ArcMutexReadOptionGuard<'a, T> {
    #[inline]
    fn drop(&mut self) {
        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        {
            if self.start_time.elapsed().as_millis() > MAX_ELAPSED_TIME_MIL {
                if self.info.lock().is_check_time {
                    panic!("lock panic");
                }
            }
        }
        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        del_write_lock(&self.info);
    }
}

pub struct ArcMutexWriteOptionGuard<'a, T: 'a> {
    d: MutexGuard<'a, T>,
    #[cfg(any(debug_assertions, feature = "anylock-time"))]
    info: Arc<Mutex<MutexInfo>>,
    #[cfg(any(debug_assertions, feature = "anylock-time"))]
    start_time: Instant,
}

impl<'a, T: 'a> ArcMutexWriteOptionGuard<'a, T> {
    pub fn new(
        d: MutexGuard<'a, T>,
        #[cfg(any(debug_assertions, feature = "anylock-time"))] info: Arc<Mutex<MutexInfo>>,
    ) -> Self {
        ArcMutexWriteOptionGuard {
            d,
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            info,
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            start_time: Instant::now(),
        }
    }
}

impl<'a, T: 'a> std::ops::Deref for ArcMutexWriteOptionGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.d.deref()
    }
}

impl<'a, T: 'a> std::ops::DerefMut for ArcMutexWriteOptionGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.d.deref_mut()
    }
}

impl<'a, T: 'a> Drop for ArcMutexWriteOptionGuard<'a, T> {
    #[inline]
    fn drop(&mut self) {
        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        {
            if self.start_time.elapsed().as_millis() > MAX_ELAPSED_TIME_MIL {
                if self.info.lock().is_check_time {
                    panic!("lock panic");
                }
            }
        }
        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        del_write_lock(&self.info);
    }
}

pub struct ArcMutex<T> {
    d: Arc<Mutex<Option<T>>>,
    #[cfg(any(debug_assertions, feature = "anylock-time"))]
    info: Arc<Mutex<MutexInfo>>,
}

impl<T> Clone for ArcMutex<T> {
    fn clone(&self) -> Self {
        ArcMutex {
            d: self.d.clone(),
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            info: self.info.clone(),
        }
    }
}

impl<T> ArcMutex<T> {
    pub fn default() -> Self {
        ArcMutex {
            d: Arc::new(Mutex::new(None)),

            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            info: Arc::new(Mutex::new(MutexInfo {
                w_thread_id: None,
                r_thread_id_map: HashMap::new(),
                is_check_time: false,
            })),
        }
    }
    pub fn new(d: T) -> Self {
        ArcMutex {
            d: Arc::new(Mutex::new(Some(d))),
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            info: Arc::new(Mutex::new(MutexInfo {
                w_thread_id: None,
                r_thread_id_map: HashMap::new(),
                is_check_time: false,
            })),
        }
    }
    pub fn new_check(d: T) -> Self {
        ArcMutex {
            d: Arc::new(Mutex::new(Some(d))),
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            info: Arc::new(Mutex::new(MutexInfo {
                w_thread_id: None,
                r_thread_id_map: HashMap::new(),
                is_check_time: true,
            })),
        }
    }

    pub fn strong_count(&self) -> usize {
        Arc::strong_count(&self.d)
    }

    pub fn set_nil(&self) {
        let mut lock = self.get_write_lock();
        *lock = None;
        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        del_write_lock(&self.info);
    }

    pub fn is_none(&self) -> bool {
        let lock = self.get_write_lock();
        let is_none = lock.is_none();
        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        del_write_lock(&self.info);
        is_none
    }
    pub fn is_some(&self) -> bool {
        !self.is_none()
    }

    pub fn set(&self, d: T) {
        let mut lock = self.get_write_lock();
        *lock = Some(d);
        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        del_write_lock(&self.info);
    }

    pub fn get(&self) -> ArcMutexReadGuard<T> {
        let lock = self.get_write_lock();
        ArcMutexReadGuard::new(
            lock,
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            self.info.clone(),
        )
    }
    pub fn get_mut(&self) -> ArcMutexWriteGuard<T> {
        let lock = self.get_write_lock();
        ArcMutexWriteGuard::new(
            lock,
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            self.info.clone(),
        )
    }

    pub fn get_option(&self) -> ArcMutexReadOptionGuard<Option<T>> {
        let lock = self.get_write_lock();
        ArcMutexReadOptionGuard::new(
            lock,
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            self.info.clone(),
        )
    }
    pub fn get_option_mut(&self) -> ArcMutexWriteOptionGuard<Option<T>> {
        let lock = self.get_write_lock();
        ArcMutexWriteOptionGuard::new(
            lock,
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            self.info.clone(),
        )
    }

    pub unsafe fn take(&self) -> Option<T> {
        let mut lock = self.get_write_lock();
        let data = lock.take();
        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        del_write_lock(&self.info);
        data
    }

    fn get_write_lock(&self) -> MutexGuard<'_, Option<T>> {
        #[cfg(not(any(debug_assertions, feature = "anylock-time")))]
        {
            return self.d.lock();
        }

        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        loop {
            let thread_id = std::thread::current().id();
            let info = &mut *self.info.lock();
            if info.w_thread_id.is_some() {
                if info.w_thread_id == Some(thread_id) {
                    log::error!("info.w_thread_id == thread_id");
                    panic!("info.w_thread_id == thread_id");
                }
            }

            let r_lock_num = info.r_thread_id_map.get(&thread_id);
            if r_lock_num.is_some() && *r_lock_num.unwrap() != 0 {
                log::error!("r_lock_num.is_some()");
                panic!("r_lock_num.is_some()");
            }

            let lock = self.d.try_lock();
            if lock.is_none() {
                continue;
            }
            let lock = lock.unwrap();
            info.w_thread_id = Some(thread_id);
            return lock;
        }
    }
}

pub struct ArcMutexBox<T> {
    d: Arc<Mutex<Option<Box<T>>>>,
    #[cfg(any(debug_assertions, feature = "anylock-time"))]
    info: Arc<Mutex<MutexInfo>>,
}

impl<T> Clone for ArcMutexBox<T> {
    fn clone(&self) -> Self {
        ArcMutexBox {
            d: self.d.clone(),

            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            info: self.info.clone(),
        }
    }
}

impl<T> ArcMutexBox<T> {
    pub fn default() -> Self {
        ArcMutexBox {
            d: Arc::new(Mutex::new(None)),

            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            info: Arc::new(Mutex::new(MutexInfo {
                w_thread_id: None,
                r_thread_id_map: HashMap::new(),
                is_check_time: false,
            })),
        }
    }
    pub fn new(d: T) -> Self {
        ArcMutexBox {
            d: Arc::new(Mutex::new(Some(Box::new(d)))),

            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            info: Arc::new(Mutex::new(MutexInfo {
                w_thread_id: None,
                r_thread_id_map: HashMap::new(),
                is_check_time: false,
            })),
        }
    }

    pub fn strong_count(&self) -> usize {
        Arc::strong_count(&self.d)
    }
    pub fn set_nil(&self) {
        let mut lock = self.get_write_lock();
        *lock = None;
        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        del_write_lock(&self.info);
    }

    pub fn is_none(&self) -> bool {
        let lock = self.get_write_lock();
        let is_none = lock.is_none();
        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        del_write_lock(&self.info);
        is_none
    }
    pub fn is_some(&self) -> bool {
        !self.is_none()
    }

    pub fn set(&self, d: T) {
        let mut lock = self.get_write_lock();
        *lock = Some(Box::new(d));
        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        del_write_lock(&self.info);
    }

    pub fn get(&self) -> ArcMutexReadGuard<Box<T>> {
        let lock = self.get_write_lock();
        ArcMutexReadGuard::<Box<T>>::new(
            lock,
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            self.info.clone(),
        )
    }
    pub fn get_mut(&self) -> ArcMutexWriteGuard<Box<T>> {
        let lock = self.get_write_lock();
        ArcMutexWriteGuard::<Box<T>>::new(
            lock,
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            self.info.clone(),
        )
    }

    pub unsafe fn take(&self) -> Option<T> {
        let mut lock = self.get_write_lock();
        let data = lock.take();
        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        del_write_lock(&self.info);
        if data.is_some() {
            Some(*data.unwrap())
        } else {
            None
        }
    }

    fn get_write_lock(&self) -> MutexGuard<'_, Option<Box<T>>> {
        #[cfg(not(any(debug_assertions, feature = "anylock-time")))]
        {
            return self.d.lock();
        }

        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        loop {
            let thread_id = std::thread::current().id();
            let info = &mut *self.info.lock();
            if info.w_thread_id.is_some() {
                if info.w_thread_id == Some(thread_id) {
                    log::error!("info.w_thread_id == thread_id");
                    panic!("info.w_thread_id == thread_id");
                }
            }

            let r_lock_num = info.r_thread_id_map.get(&thread_id);
            if r_lock_num.is_some() && *r_lock_num.unwrap() != 0 {
                log::error!("r_lock_num.is_some()");
                panic!("r_lock_num.is_some()");
            }
            let lock = self.d.try_lock();
            if lock.is_none() {
                continue;
            }
            let lock = lock.unwrap();
            info.w_thread_id = Some(thread_id);
            return lock;
        }
    }
}

pub struct ArcRwLockReadGuard<'a, T: 'a> {
    d: RwLockReadGuard<'a, Option<T>>,
    #[cfg(any(debug_assertions, feature = "anylock-time"))]
    info: Arc<Mutex<MutexInfo>>,
    #[cfg(any(debug_assertions, feature = "anylock-time"))]
    start_time: Instant,
}

impl<'a, T: 'a> ArcRwLockReadGuard<'a, T> {
    pub fn new(
        d: RwLockReadGuard<'a, Option<T>>,
        #[cfg(any(debug_assertions, feature = "anylock-time"))] info: Arc<Mutex<MutexInfo>>,
    ) -> Self {
        ArcRwLockReadGuard {
            d,
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            info,
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            start_time: Instant::now(),
        }
    }
    pub fn is_none(&self) -> bool {
        self.d.is_none()
    }
    pub fn is_some(&self) -> bool {
        !self.is_none()
    }
}

impl<'a, T: 'a> std::ops::Deref for ArcRwLockReadGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.d.deref().as_ref().unwrap()
    }
}

impl<'a, T: 'a> Drop for ArcRwLockReadGuard<'a, T> {
    #[inline]
    fn drop(&mut self) {
        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        {
            if self.start_time.elapsed().as_millis() > MAX_ELAPSED_TIME_MIL {
                if self.info.lock().is_check_time {
                    panic!("lock panic");
                }
            }
        }
        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        del_read_lock(&self.info);
    }
}

pub struct ArcRwLockWriteGuard<'a, T: 'a> {
    d: RwLockWriteGuard<'a, Option<T>>,
    #[cfg(any(debug_assertions, feature = "anylock-time"))]
    info: Arc<Mutex<MutexInfo>>,
    #[cfg(any(debug_assertions, feature = "anylock-time"))]
    start_time: Instant,
}

impl<'a, T: 'a> ArcRwLockWriteGuard<'a, T> {
    pub fn new(
        d: RwLockWriteGuard<'a, Option<T>>,
        #[cfg(any(debug_assertions, feature = "anylock-time"))] info: Arc<Mutex<MutexInfo>>,
    ) -> Self {
        ArcRwLockWriteGuard {
            d,
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            info,
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            start_time: Instant::now(),
        }
    }
    pub fn is_none(&self) -> bool {
        self.d.is_none()
    }
    pub fn is_some(&self) -> bool {
        !self.is_none()
    }
}

impl<'a, T: 'a> std::ops::Deref for ArcRwLockWriteGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.d.deref().as_ref().unwrap()
    }
}

impl<'a, T: 'a> std::ops::DerefMut for ArcRwLockWriteGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.d.deref_mut().as_mut().unwrap()
    }
}

impl<'a, T: 'a> Drop for ArcRwLockWriteGuard<'a, T> {
    #[inline]
    fn drop(&mut self) {
        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        {
            if self.start_time.elapsed().as_millis() > MAX_ELAPSED_TIME_MIL {
                if self.info.lock().is_check_time {
                    panic!("lock panic");
                }
            }
        }
        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        del_write_lock(&self.info);
    }
}

pub struct ArcRwLock<T> {
    d: Arc<RwLock<Option<T>>>,

    #[cfg(any(debug_assertions, feature = "anylock-time"))]
    info: Arc<Mutex<MutexInfo>>,
}

impl<T> Clone for ArcRwLock<T> {
    fn clone(&self) -> Self {
        ArcRwLock {
            d: self.d.clone(),

            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            info: self.info.clone(),
        }
    }
}

impl<T> ArcRwLock<T> {
    pub fn default() -> Self {
        ArcRwLock {
            d: Arc::new(RwLock::new(None)),

            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            info: Arc::new(Mutex::new(MutexInfo {
                w_thread_id: None,
                r_thread_id_map: HashMap::new(),
                is_check_time: false,
            })),
        }
    }
    pub fn new(d: T) -> Self {
        ArcRwLock {
            d: Arc::new(RwLock::new(Some(d))),

            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            info: Arc::new(Mutex::new(MutexInfo {
                w_thread_id: None,
                r_thread_id_map: HashMap::new(),
                is_check_time: false,
            })),
        }
    }
    pub fn strong_count(&self) -> usize {
        Arc::strong_count(&self.d)
    }
    pub fn set_nil(&self) {
        let mut lock = self.get_write_lock();
        *lock = None;
        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        del_write_lock(&self.info);
    }
    pub fn is_none(&self) -> bool {
        let lock = self.get_read_lock();
        let is_none = lock.is_none();
        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        del_read_lock(&self.info);
        is_none
    }
    pub fn is_some(&self) -> bool {
        !self.is_none()
    }

    pub fn set(&self, d: T) {
        let mut lock = self.get_write_lock();
        *lock = Some(d);
        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        del_write_lock(&self.info);
    }

    pub fn get(&self) -> ArcRwLockReadGuard<T> {
        let lock = self.get_read_lock();
        ArcRwLockReadGuard::new(
            lock,
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            self.info.clone(),
        )
    }
    pub fn get_mut(&self) -> ArcRwLockWriteGuard<T> {
        let lock = self.get_write_lock();
        ArcRwLockWriteGuard::new(
            lock,
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            self.info.clone(),
        )
    }

    pub unsafe fn take(&self) -> Option<T> {
        let mut lock = self.get_write_lock();
        let data = lock.take();
        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        del_write_lock(&self.info);
        data
    }
    fn get_write_lock(&self) -> RwLockWriteGuard<'_, Option<T>> {
        #[cfg(not(any(debug_assertions, feature = "anylock-time")))]
        {
            return self.d.write();
        }

        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        loop {
            let thread_id = std::thread::current().id();
            let info = &mut *self.info.lock();
            if info.w_thread_id.is_some() {
                if info.w_thread_id == Some(thread_id) {
                    log::error!("info.w_thread_id == thread_id");
                    panic!("info.w_thread_id == thread_id");
                }
            }

            let r_lock_num = info.r_thread_id_map.get(&thread_id);
            if r_lock_num.is_some() && *r_lock_num.unwrap() != 0 {
                log::error!("r_lock_num.is_some()");
                panic!("r_lock_num.is_some()");
            }

            let lock = self.d.try_write();
            if lock.is_none() {
                continue;
            }
            let lock = lock.unwrap();
            info.w_thread_id = Some(thread_id);
            return lock;
        }
    }

    fn get_read_lock(&self) -> RwLockReadGuard<'_, Option<T>> {
        #[cfg(not(any(debug_assertions, feature = "anylock-time")))]
        {
            return self.d.read();
        }

        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        loop {
            let thread_id = std::thread::current().id();
            let info = &mut *self.info.lock();
            if info.w_thread_id.is_some() {
                if info.w_thread_id == Some(thread_id) {
                    log::error!("info.w_thread_id == thread_id");
                    panic!("info.w_thread_id == thread_id");
                }
            }

            let lock = self.d.try_read();
            if lock.is_none() {
                continue;
            }
            let lock = lock.unwrap();
            let r_lock_num = info.r_thread_id_map.get_mut(&thread_id);
            if r_lock_num.is_none() {
                info.r_thread_id_map.insert(thread_id, 1);
            } else {
                let r_lock_num = r_lock_num.unwrap();
                *r_lock_num += 1;
            }
            return lock;
        }
    }
}
#[cfg(any(debug_assertions, feature = "anylock-time"))]
fn del_write_lock(info: &Arc<Mutex<MutexInfo>>) {
    let info = &mut *info.lock();
    info.w_thread_id = None;
}
#[cfg(any(debug_assertions, feature = "anylock-time"))]
fn del_read_lock(info: &Arc<Mutex<MutexInfo>>) {
    let info = &mut *info.lock();
    let thread_id = std::thread::current().id();
    let r_lock_num = info.r_thread_id_map.get_mut(&thread_id);
    if r_lock_num.is_none() {
        log::error!("r_lock_num.is_none()");
        return;
    }
    let r_lock_num = r_lock_num.unwrap();
    if *r_lock_num == 0 {
        log::error!("r_lock_num == 0");
        return;
    }
    *r_lock_num -= 1;
}
