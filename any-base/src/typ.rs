#[cfg(feature = "anyspawn-parking-lot")]
pub use crate::parking_lot::typ::{
    ArcMutex, ArcMutexBox, ArcMutexReadGuard, ArcMutexWriteGuard, ArcRwLock, ArcRwLockReadGuard,
    ArcRwLockWriteGuard,
};
#[cfg(feature = "anyspawn-parking-lot")]
pub use parking_lot::Mutex;

// #[cfg(feature = "anyspawn-std-sync")]
// pub use crate::std_sync::typ::{
//     ArcMutex, ArcMutexBox, ArcMutexReadGuard, ArcMutexWriteGuard, ArcRwLock, ArcRwLockReadGuard, ArcRwLockWriteGuard,
// };
// #[cfg(feature = "anyspawn-std-sync")]
// pub use std::sync::Mutex;

#[cfg(any(debug_assertions, feature = "anylock-time"))]
use crate::util::spawn_str_id;
use std::cell::Ref;
use std::cell::RefCell;
use std::cell::RefMut;
use std::cell::UnsafeCell;
#[cfg(any(debug_assertions, feature = "anylock-time"))]
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

pub type Share<T> = ArcMutex<T>;
pub type ShareRw<T> = ArcRwLock<T>;

#[cfg(any(debug_assertions, feature = "anylock-time"))]
use std::time::Instant;

#[cfg(any(debug_assertions, feature = "anylock-time"))]
#[cfg(debug_assertions)]
const MAX_ELAPSED_TIME_MIL: u128 = 100;

#[cfg(any(debug_assertions, feature = "anylock-time"))]
#[cfg(not(debug_assertions))]
const MAX_ELAPSED_TIME_MIL: u128 = 50;

#[cfg(any(debug_assertions, feature = "anylock-time"))]
pub struct MutexInfoTokio {
    w_thread_id: Option<String>,
    r_thread_id_map: HashMap<String, usize>,
    is_check_time: bool,
}

pub struct ArcMutexReadGuardTokio<'a, T: 'a> {
    d: tokio::sync::MutexGuard<'a, Option<T>>,
    #[cfg(any(debug_assertions, feature = "anylock-time"))]
    info: Arc<Mutex<MutexInfoTokio>>,
    #[cfg(any(debug_assertions, feature = "anylock-time"))]
    start_time: Instant,
}

impl<'a, T: 'a> ArcMutexReadGuardTokio<'a, T> {
    pub fn new(
        d: tokio::sync::MutexGuard<'a, Option<T>>,
        #[cfg(any(debug_assertions, feature = "anylock-time"))] info: Arc<Mutex<MutexInfoTokio>>,
    ) -> Self {
        ArcMutexReadGuardTokio {
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

impl<'a, T: 'a> std::ops::Deref for ArcMutexReadGuardTokio<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.d.deref().as_ref().unwrap()
    }
}

impl<'a, T: 'a> Drop for ArcMutexReadGuardTokio<'a, T> {
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
        del_write_lock_tokio(&self.info);
    }
}

pub struct ArcMutexWriteGuardTokio<'a, T: 'a> {
    d: tokio::sync::MutexGuard<'a, Option<T>>,
    #[cfg(any(debug_assertions, feature = "anylock-time"))]
    info: Arc<Mutex<MutexInfoTokio>>,
    #[cfg(any(debug_assertions, feature = "anylock-time"))]
    start_time: Instant,
}

impl<'a, T: 'a> ArcMutexWriteGuardTokio<'a, T> {
    pub fn new(
        d: tokio::sync::MutexGuard<'a, Option<T>>,
        #[cfg(any(debug_assertions, feature = "anylock-time"))] info: Arc<Mutex<MutexInfoTokio>>,
    ) -> Self {
        ArcMutexWriteGuardTokio {
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

impl<'a, T: 'a> std::ops::Deref for ArcMutexWriteGuardTokio<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.d.deref().as_ref().unwrap()
    }
}

impl<'a, T: 'a> std::ops::DerefMut for ArcMutexWriteGuardTokio<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.d.deref_mut().as_mut().unwrap()
    }
}

impl<'a, T: 'a> Drop for ArcMutexWriteGuardTokio<'a, T> {
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
        del_write_lock_tokio(&self.info);
    }
}

pub struct ArcMutexReadOptionGuardTokio<'a, T: 'a> {
    d: tokio::sync::MutexGuard<'a, T>,
    #[cfg(any(debug_assertions, feature = "anylock-time"))]
    info: Arc<Mutex<MutexInfoTokio>>,
    #[cfg(any(debug_assertions, feature = "anylock-time"))]
    start_time: Instant,
}

impl<'a, T: 'a> ArcMutexReadOptionGuardTokio<'a, T> {
    pub fn new(
        d: tokio::sync::MutexGuard<'a, T>,
        #[cfg(any(debug_assertions, feature = "anylock-time"))] info: Arc<Mutex<MutexInfoTokio>>,
    ) -> Self {
        ArcMutexReadOptionGuardTokio {
            d,
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            info,
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            start_time: Instant::now(),
        }
    }
}

impl<'a, T: 'a> std::ops::Deref for ArcMutexReadOptionGuardTokio<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.d.deref()
    }
}

impl<'a, T: 'a> Drop for ArcMutexReadOptionGuardTokio<'a, T> {
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
        del_write_lock_tokio(&self.info);
    }
}

pub struct ArcMutexWriteOptionGuardTokio<'a, T: 'a> {
    d: tokio::sync::MutexGuard<'a, T>,
    #[cfg(any(debug_assertions, feature = "anylock-time"))]
    info: Arc<Mutex<MutexInfoTokio>>,
    #[cfg(any(debug_assertions, feature = "anylock-time"))]
    start_time: Instant,
}

impl<'a, T: 'a> ArcMutexWriteOptionGuardTokio<'a, T> {
    pub fn new(
        d: tokio::sync::MutexGuard<'a, T>,
        #[cfg(any(debug_assertions, feature = "anylock-time"))] info: Arc<Mutex<MutexInfoTokio>>,
    ) -> Self {
        ArcMutexWriteOptionGuardTokio {
            d,
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            info,
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            start_time: Instant::now(),
        }
    }
}

impl<'a, T: 'a> std::ops::Deref for ArcMutexWriteOptionGuardTokio<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.d.deref()
    }
}

impl<'a, T: 'a> std::ops::DerefMut for ArcMutexWriteOptionGuardTokio<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.d.deref_mut()
    }
}

impl<'a, T: 'a> Drop for ArcMutexWriteOptionGuardTokio<'a, T> {
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
        del_write_lock_tokio(&self.info);
    }
}

pub struct ArcMutexTokio<T> {
    d: Arc<tokio::sync::Mutex<Option<T>>>,
    #[cfg(any(debug_assertions, feature = "anylock-time"))]
    info: Arc<Mutex<MutexInfoTokio>>,
}

impl<T> Clone for ArcMutexTokio<T> {
    fn clone(&self) -> Self {
        ArcMutexTokio {
            d: self.d.clone(),
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            info: self.info.clone(),
        }
    }
}

impl<T> ArcMutexTokio<T> {
    pub fn default() -> Self {
        ArcMutexTokio {
            d: Arc::new(tokio::sync::Mutex::new(None)),
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            info: Arc::new(Mutex::new(MutexInfoTokio {
                w_thread_id: None,
                r_thread_id_map: HashMap::new(),
                is_check_time: false,
            })),
        }
    }
    pub fn new(d: T) -> Self {
        ArcMutexTokio {
            d: Arc::new(tokio::sync::Mutex::new(Some(d))),
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            info: Arc::new(Mutex::new(MutexInfoTokio {
                w_thread_id: None,
                r_thread_id_map: HashMap::new(),
                is_check_time: false,
            })),
        }
    }

    pub fn new_check(d: T) -> Self {
        ArcMutexTokio {
            d: Arc::new(tokio::sync::Mutex::new(Some(d))),
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            info: Arc::new(Mutex::new(MutexInfoTokio {
                w_thread_id: None,
                r_thread_id_map: HashMap::new(),
                is_check_time: true,
            })),
        }
    }

    pub fn strong_count(&self) -> usize {
        Arc::strong_count(&self.d)
    }
    pub async fn set_nil(&self) -> Option<T> {
        let mut lock = self.get_write_lock().await;
        let data = lock.take();
        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        del_write_lock_tokio(&self.info);
        data
    }

    pub async fn is_none(&self) -> bool {
        let lock = self.get_write_lock().await;
        let is_none = lock.is_none();
        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        del_write_lock_tokio(&self.info);
        is_none
    }
    pub async fn is_some(&self) -> bool {
        !self.is_none().await
    }

    pub async fn set(&self, d: T) {
        let mut lock = self.get_write_lock().await;
        *lock = Some(d);
        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        del_write_lock_tokio(&self.info);
    }

    pub async fn get(&self) -> ArcMutexReadGuardTokio<T> {
        let lock = self.get_write_lock().await;
        ArcMutexReadGuardTokio::new(
            lock,
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            self.info.clone(),
        )
    }
    pub async fn get_mut(&self) -> ArcMutexWriteGuardTokio<T> {
        let lock = self.get_write_lock().await;
        ArcMutexWriteGuardTokio::new(
            lock,
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            self.info.clone(),
        )
    }

    pub async fn get_option(&self) -> ArcMutexReadOptionGuardTokio<Option<T>> {
        let lock = self.get_write_lock().await;
        ArcMutexReadOptionGuardTokio::new(
            lock,
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            self.info.clone(),
        )
    }
    pub async fn get_option_mut(&self) -> ArcMutexWriteOptionGuardTokio<Option<T>> {
        let lock = self.get_write_lock().await;
        ArcMutexWriteOptionGuardTokio::new(
            lock,
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            self.info.clone(),
        )
    }

    pub async unsafe fn take(&self) -> Option<T> {
        let mut lock = self.get_write_lock().await;
        let data = lock.take();
        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        del_write_lock_tokio(&self.info);
        data
    }

    async fn get_write_lock(&self) -> tokio::sync::MutexGuard<'_, Option<T>> {
        #[cfg(not(any(debug_assertions, feature = "anylock-time")))]
        {
            return self.d.lock().await;
        }

        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        {
            let thread_id = spawn_str_id();
            {
                let info = &mut *self.info.lock();
                if info.w_thread_id.is_some() {
                    if info.w_thread_id == Some(thread_id.clone()) {
                        log::error!("info.w_thread_id == thread_id");
                        panic!("info.w_thread_id == thread_id");
                    }
                }

                let r_lock_num = info.r_thread_id_map.get(&thread_id);
                if r_lock_num.is_some() && *r_lock_num.unwrap() != 0 {
                    log::error!("r_lock_num.is_some()");
                    panic!("r_lock_num.is_some()");
                }
            }

            let lock = self.d.lock().await;
            let info = &mut *self.info.lock();
            info.w_thread_id = Some(thread_id);
            return lock;
        }
    }
}

pub struct ArcMutexAnyReadGuardTokio<'a> {
    d: tokio::sync::MutexGuard<'a, Option<Box<dyn std::any::Any>>>,
    #[cfg(any(debug_assertions, feature = "anylock-time"))]
    info: Arc<Mutex<MutexInfoTokio>>,
    #[cfg(any(debug_assertions, feature = "anylock-time"))]
    start_time: Instant,
}

unsafe impl Send for ArcMutexAnyReadGuardTokio<'_> {}
unsafe impl Sync for ArcMutexAnyReadGuardTokio<'_> {}

impl<'a> ArcMutexAnyReadGuardTokio<'a> {
    pub fn new(
        d: tokio::sync::MutexGuard<'a, Option<Box<dyn std::any::Any>>>,
        #[cfg(any(debug_assertions, feature = "anylock-time"))] info: Arc<Mutex<MutexInfoTokio>>,
    ) -> Self {
        ArcMutexAnyReadGuardTokio {
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
    pub fn get_any<T: 'static>(&self) -> &T {
        self.d.as_ref().unwrap().downcast_ref::<T>().unwrap()
    }
}

impl<'a> std::ops::Deref for ArcMutexAnyReadGuardTokio<'a> {
    type Target = Box<dyn std::any::Any>;

    fn deref(&self) -> &Self::Target {
        self.d.deref().as_ref().unwrap()
    }
}

impl<'a> Drop for ArcMutexAnyReadGuardTokio<'a> {
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
        del_write_lock_tokio(&self.info);
    }
}

pub struct ArcMutexAnyWriteGuardTokio<'a> {
    d: tokio::sync::MutexGuard<'a, Option<Box<dyn std::any::Any>>>,

    #[cfg(any(debug_assertions, feature = "anylock-time"))]
    info: Arc<Mutex<MutexInfoTokio>>,
    #[cfg(any(debug_assertions, feature = "anylock-time"))]
    start_time: Instant,
}

unsafe impl Send for ArcMutexAnyWriteGuardTokio<'_> {}
unsafe impl Sync for ArcMutexAnyWriteGuardTokio<'_> {}

impl<'a> ArcMutexAnyWriteGuardTokio<'a> {
    pub fn new(
        d: tokio::sync::MutexGuard<'a, Option<Box<dyn std::any::Any>>>,
        #[cfg(any(debug_assertions, feature = "anylock-time"))] info: Arc<Mutex<MutexInfoTokio>>,
    ) -> Self {
        ArcMutexAnyWriteGuardTokio {
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
    pub fn get_any<T: 'static>(&self) -> &T {
        self.d.as_ref().unwrap().downcast_ref::<T>().unwrap()
    }

    pub fn get_any_mut<T: 'static>(&mut self) -> &mut T {
        self.d.as_mut().unwrap().downcast_mut::<T>().unwrap()
    }
}

impl<'a> std::ops::Deref for ArcMutexAnyWriteGuardTokio<'a> {
    type Target = Box<dyn std::any::Any>;

    fn deref(&self) -> &Self::Target {
        self.d.deref().as_ref().unwrap()
    }
}

impl<'a> std::ops::DerefMut for ArcMutexAnyWriteGuardTokio<'a> {
    fn deref_mut(&mut self) -> &mut Box<dyn std::any::Any> {
        self.d.deref_mut().as_mut().unwrap()
    }
}

impl<'a> Drop for ArcMutexAnyWriteGuardTokio<'a> {
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
        del_write_lock_tokio(&self.info);
    }
}

pub struct ArcMutexAnyTokio {
    d: Arc<tokio::sync::Mutex<Option<Box<dyn std::any::Any>>>>,
    #[cfg(any(debug_assertions, feature = "anylock-time"))]
    info: Arc<Mutex<MutexInfoTokio>>,
}

unsafe impl Send for ArcMutexAnyTokio {}
unsafe impl Sync for ArcMutexAnyTokio {}

impl Clone for ArcMutexAnyTokio {
    fn clone(&self) -> Self {
        ArcMutexAnyTokio {
            d: self.d.clone(),
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            info: self.info.clone(),
        }
    }
}

impl ArcMutexAnyTokio {
    pub fn default() -> Self {
        ArcMutexAnyTokio {
            d: Arc::new(tokio::sync::Mutex::new(None)),

            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            info: Arc::new(Mutex::new(MutexInfoTokio {
                w_thread_id: None,
                r_thread_id_map: HashMap::new(),
                is_check_time: false,
            })),
        }
    }
    pub fn new(d: Box<dyn std::any::Any>) -> Self {
        ArcMutexAnyTokio {
            d: Arc::new(tokio::sync::Mutex::new(Some(d))),

            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            info: Arc::new(Mutex::new(MutexInfoTokio {
                w_thread_id: None,
                r_thread_id_map: HashMap::new(),
                is_check_time: false,
            })),
        }
    }

    pub fn strong_count(&self) -> usize {
        Arc::strong_count(&self.d)
    }
    pub async fn set_nil(&self) -> Option<Box<dyn std::any::Any>> {
        let mut lock = self.get_write_lock().await;
        let data = lock.take();
        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        del_write_lock_tokio(&self.info);
        data
    }

    pub async fn is_none(&self) -> bool {
        let lock = self.get_write_lock().await;
        let is_none = lock.is_none();
        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        del_write_lock_tokio(&self.info);
        is_none
    }
    pub async fn is_some(&self) -> bool {
        !self.is_none().await
    }

    pub async fn set(&self, d: Box<dyn std::any::Any>) {
        let mut lock = self.get_write_lock().await;
        *lock = Some(Box::new(d));
        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        del_write_lock_tokio(&self.info);
    }

    pub async fn get(&self) -> ArcMutexAnyReadGuardTokio {
        let lock = self.get_write_lock().await;
        ArcMutexAnyReadGuardTokio::new(
            lock,
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            self.info.clone(),
        )
    }
    pub async fn get_mut(&self) -> ArcMutexAnyWriteGuardTokio {
        let lock = self.get_write_lock().await;
        ArcMutexAnyWriteGuardTokio::new(
            lock,
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            self.info.clone(),
        )
    }

    pub async unsafe fn take(&self) -> Option<Box<dyn std::any::Any>> {
        let mut lock = self.get_write_lock().await;
        let data = lock.take();
        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        del_write_lock_tokio(&self.info);
        data
    }

    async fn get_write_lock(&self) -> tokio::sync::MutexGuard<'_, Option<Box<dyn std::any::Any>>> {
        #[cfg(not(any(debug_assertions, feature = "anylock-time")))]
        {
            return self.d.lock().await;
        }

        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        {
            let thread_id = spawn_str_id();
            {
                let info = &mut *self.info.lock();
                if info.w_thread_id.is_some() {
                    if info.w_thread_id == Some(thread_id.clone()) {
                        log::error!("info.w_thread_id == thread_id");
                        panic!("info.w_thread_id == thread_id");
                    }
                }

                let r_lock_num = info.r_thread_id_map.get(&thread_id);
                if r_lock_num.is_some() && *r_lock_num.unwrap() != 0 {
                    log::error!("r_lock_num.is_some()");
                    panic!("r_lock_num.is_some()");
                }
            }
            let lock = self.d.lock().await;
            let info = &mut *self.info.lock();
            info.w_thread_id = Some(thread_id);
            return lock;
        }
    }
}

pub struct ArcMutexBoxTokio<T> {
    d: Arc<tokio::sync::Mutex<Option<Box<T>>>>,
    #[cfg(any(debug_assertions, feature = "anylock-time"))]
    info: Arc<Mutex<MutexInfoTokio>>,
}

impl<T> Clone for ArcMutexBoxTokio<T> {
    fn clone(&self) -> Self {
        ArcMutexBoxTokio {
            d: self.d.clone(),

            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            info: self.info.clone(),
        }
    }
}

impl<T> ArcMutexBoxTokio<T> {
    pub fn default() -> Self {
        ArcMutexBoxTokio {
            d: Arc::new(tokio::sync::Mutex::new(None)),

            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            info: Arc::new(Mutex::new(MutexInfoTokio {
                w_thread_id: None,
                r_thread_id_map: HashMap::new(),
                is_check_time: false,
            })),
        }
    }
    pub fn new(d: T) -> Self {
        ArcMutexBoxTokio {
            d: Arc::new(tokio::sync::Mutex::new(Some(Box::new(d)))),

            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            info: Arc::new(Mutex::new(MutexInfoTokio {
                w_thread_id: None,
                r_thread_id_map: HashMap::new(),
                is_check_time: false,
            })),
        }
    }

    pub fn strong_count(&self) -> usize {
        Arc::strong_count(&self.d)
    }
    pub async fn set_nil(&self) -> Option<T> {
        let mut lock = self.get_write_lock().await;
        let data = lock.take();
        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        del_write_lock_tokio(&self.info);
        if data.is_some() {
            Some(*data.unwrap())
        } else {
            None
        }
    }

    pub async fn is_none(&self) -> bool {
        let lock = self.get_write_lock().await;
        let is_none = lock.is_none();
        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        del_write_lock_tokio(&self.info);
        is_none
    }
    pub async fn is_some(&self) -> bool {
        !self.is_none().await
    }

    pub async fn set(&self, d: T) {
        let mut lock = self.get_write_lock().await;
        *lock = Some(Box::new(d));
        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        del_write_lock_tokio(&self.info);
    }

    pub async fn get(&self) -> ArcMutexReadGuardTokio<Box<T>> {
        let lock = self.get_write_lock().await;
        ArcMutexReadGuardTokio::<Box<T>>::new(
            lock,
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            self.info.clone(),
        )
    }
    pub async fn get_mut(&self) -> ArcMutexWriteGuardTokio<Box<T>> {
        let lock = self.get_write_lock().await;
        ArcMutexWriteGuardTokio::<Box<T>>::new(
            lock,
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            self.info.clone(),
        )
    }

    pub async unsafe fn take(&self) -> Option<T> {
        let mut lock = self.get_write_lock().await;
        let data = lock.take();
        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        del_write_lock_tokio(&self.info);
        if data.is_some() {
            Some(*data.unwrap())
        } else {
            None
        }
    }

    async fn get_write_lock(&self) -> tokio::sync::MutexGuard<'_, Option<Box<T>>> {
        #[cfg(not(any(debug_assertions, feature = "anylock-time")))]
        {
            return self.d.lock().await;
        }

        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        {
            let thread_id = spawn_str_id();
            {
                let info = &mut *self.info.lock();
                if info.w_thread_id.is_some() {
                    if info.w_thread_id == Some(thread_id.clone()) {
                        log::error!("info.w_thread_id == thread_id");
                        panic!("info.w_thread_id == thread_id");
                    }
                }

                let r_lock_num = info.r_thread_id_map.get(&thread_id);
                if r_lock_num.is_some() && *r_lock_num.unwrap() != 0 {
                    log::error!("r_lock_num.is_some()");
                    panic!("r_lock_num.is_some()");
                }
            }
            let lock = self.d.lock().await;
            let info = &mut *self.info.lock();
            info.w_thread_id = Some(thread_id);
            return lock;
        }
    }
}

pub struct ArcRwLockReadTokioGuard<'a, T: 'a> {
    d: tokio::sync::RwLockReadGuard<'a, Option<T>>,
    #[cfg(any(debug_assertions, feature = "anylock-time"))]
    info: Arc<Mutex<MutexInfoTokio>>,
    #[cfg(any(debug_assertions, feature = "anylock-time"))]
    start_time: Instant,
}

impl<'a, T: 'a> ArcRwLockReadTokioGuard<'a, T> {
    pub fn new(
        d: tokio::sync::RwLockReadGuard<'a, Option<T>>,
        #[cfg(any(debug_assertions, feature = "anylock-time"))] info: Arc<Mutex<MutexInfoTokio>>,
    ) -> Self {
        ArcRwLockReadTokioGuard {
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

impl<'a, T: 'a> std::ops::Deref for ArcRwLockReadTokioGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.d.deref().as_ref().unwrap()
    }
}

impl<'a, T: 'a> Drop for ArcRwLockReadTokioGuard<'a, T> {
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
        del_read_lock_tokio(&self.info);
    }
}

pub struct ArcRwLockWriteTokioGuard<'a, T: 'a> {
    d: tokio::sync::RwLockWriteGuard<'a, Option<T>>,
    #[cfg(any(debug_assertions, feature = "anylock-time"))]
    info: Arc<Mutex<MutexInfoTokio>>,
    #[cfg(any(debug_assertions, feature = "anylock-time"))]
    start_time: Instant,
}

impl<'a, T: 'a> ArcRwLockWriteTokioGuard<'a, T> {
    pub fn new(
        d: tokio::sync::RwLockWriteGuard<'a, Option<T>>,
        #[cfg(any(debug_assertions, feature = "anylock-time"))] info: Arc<Mutex<MutexInfoTokio>>,
    ) -> Self {
        ArcRwLockWriteTokioGuard {
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

impl<'a, T: 'a> std::ops::Deref for ArcRwLockWriteTokioGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.d.deref().as_ref().unwrap()
    }
}

impl<'a, T: 'a> std::ops::DerefMut for ArcRwLockWriteTokioGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.d.deref_mut().as_mut().unwrap()
    }
}

impl<'a, T: 'a> Drop for ArcRwLockWriteTokioGuard<'a, T> {
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
        del_write_lock_tokio(&self.info);
    }
}

pub struct ArcRwLockTokio<T> {
    d: Arc<tokio::sync::RwLock<Option<T>>>,

    #[cfg(any(debug_assertions, feature = "anylock-time"))]
    info: Arc<Mutex<MutexInfoTokio>>,
}

impl<T> Clone for ArcRwLockTokio<T> {
    fn clone(&self) -> Self {
        ArcRwLockTokio {
            d: self.d.clone(),
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            info: self.info.clone(),
        }
    }
}

impl<T> ArcRwLockTokio<T> {
    pub fn default() -> Self {
        ArcRwLockTokio {
            d: Arc::new(tokio::sync::RwLock::new(None)),

            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            info: Arc::new(Mutex::new(MutexInfoTokio {
                w_thread_id: None,
                r_thread_id_map: HashMap::new(),
                is_check_time: false,
            })),
        }
    }
    pub fn new(d: T) -> Self {
        ArcRwLockTokio {
            d: Arc::new(tokio::sync::RwLock::new(Some(d))),
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            info: Arc::new(Mutex::new(MutexInfoTokio {
                w_thread_id: None,
                r_thread_id_map: HashMap::new(),
                is_check_time: false,
            })),
        }
    }
    pub fn strong_count(&self) -> usize {
        Arc::strong_count(&self.d)
    }
    pub async fn set_nil(&self) -> Option<T> {
        let mut lock = self.get_write_lock().await;
        let data = lock.take();
        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        del_write_lock_tokio(&self.info);
        data
    }
    pub async fn is_none(&self) -> bool {
        let lock = self.get_read_lock().await;
        let is_none = lock.is_none();
        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        del_read_lock_tokio(&self.info);
        is_none
    }
    pub async fn is_some(&self) -> bool {
        !self.is_none().await
    }

    pub async fn set(&self, d: T) {
        let mut lock = self.get_write_lock().await;
        *lock = Some(d);
        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        del_write_lock_tokio(&self.info);
    }

    pub async fn get(&self) -> ArcRwLockReadTokioGuard<T> {
        let lock = self.get_read_lock().await;
        ArcRwLockReadTokioGuard::new(
            lock,
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            self.info.clone(),
        )
    }
    pub async fn get_mut(&self) -> ArcRwLockWriteTokioGuard<T> {
        let lock = self.get_write_lock().await;
        ArcRwLockWriteTokioGuard::new(
            lock,
            #[cfg(any(debug_assertions, feature = "anylock-time"))]
            self.info.clone(),
        )
    }

    pub async unsafe fn take(&self) -> Option<T> {
        let mut lock = self.get_write_lock().await;
        let data = lock.take();
        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        del_write_lock_tokio(&self.info);
        data
    }

    async fn get_write_lock(&self) -> tokio::sync::RwLockWriteGuard<'_, Option<T>> {
        #[cfg(not(any(debug_assertions, feature = "anylock-time")))]
        {
            return self.d.write().await;
        }

        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        {
            let thread_id = spawn_str_id();
            {
                let info = &mut *self.info.lock();
                if info.w_thread_id.is_some() {
                    if info.w_thread_id == Some(thread_id.clone()) {
                        log::error!("info.w_thread_id == thread_id");
                        panic!("info.w_thread_id == thread_id");
                    }
                }

                let r_lock_num = info.r_thread_id_map.get(&thread_id);
                if r_lock_num.is_some() && *r_lock_num.unwrap() != 0 {
                    log::error!("r_lock_num.is_some()");
                    panic!("r_lock_num.is_some()");
                }
            }

            let lock = self.d.write().await;
            let info = &mut *self.info.lock();
            info.w_thread_id = Some(thread_id);
            return lock;
        }
    }

    async fn get_read_lock(&self) -> tokio::sync::RwLockReadGuard<'_, Option<T>> {
        #[cfg(not(any(debug_assertions, feature = "anylock-time")))]
        {
            return self.d.read().await;
        }

        #[cfg(any(debug_assertions, feature = "anylock-time"))]
        {
            let thread_id = spawn_str_id();
            {
                let info = &mut *self.info.lock();
                if info.w_thread_id.is_some() {
                    if info.w_thread_id == Some(thread_id.clone()) {
                        log::error!("info.w_thread_id == thread_id");
                        panic!("info.w_thread_id == thread_id");
                    }
                }
            }

            let lock = self.d.read().await;
            let info = &mut *self.info.lock();
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

pub struct ArcUnsafeAny {
    d: Arc<UnsafeCell<Option<Box<dyn std::any::Any>>>>,
}

unsafe impl Send for ArcUnsafeAny {}
unsafe impl Sync for ArcUnsafeAny {}

impl Clone for ArcUnsafeAny {
    fn clone(&self) -> Self {
        ArcUnsafeAny { d: self.d.clone() }
    }
}

impl ArcUnsafeAny {
    pub fn default() -> Self {
        ArcUnsafeAny {
            d: Arc::new(UnsafeCell::new(None)),
        }
    }

    pub fn new(d: Box<dyn std::any::Any>) -> Self {
        ArcUnsafeAny {
            d: Arc::new(UnsafeCell::new(Some(d))),
        }
    }
    pub fn strong_count(&self) -> usize {
        Arc::strong_count(&self.d)
    }
    pub async fn set_nil(&self) {
        let data = unsafe { &mut *self.d.get() };
        *data = None;
    }
    pub fn is_none(&self) -> bool {
        unsafe { &*self.d.get() }.is_none()
    }
    pub fn is_some(&self) -> bool {
        !self.is_none()
    }

    pub fn set(&self, d: Box<dyn std::any::Any>) {
        *unsafe { &mut *self.d.get() } = Some(d);
    }

    pub fn get<T: 'static>(&self) -> &T {
        unsafe { &*self.d.get() }
            .as_ref()
            .unwrap()
            .downcast_ref::<T>()
            .unwrap()
    }

    pub fn get_mut<T: 'static>(&self) -> &mut T {
        unsafe { &mut *self.d.get() }
            .as_mut()
            .unwrap()
            .downcast_mut::<T>()
            .unwrap()
    }

    pub unsafe fn take<T: 'static>(&self) -> T {
        *unsafe { &mut *self.d.get() }
            .take()
            .unwrap()
            .downcast::<T>()
            .unwrap()
    }
}

pub struct OptionAny {
    d: Option<Box<dyn std::any::Any>>,
}

impl OptionAny {
    pub fn default() -> Self {
        OptionAny { d: None }
    }

    pub fn new(d: Box<dyn std::any::Any>) -> Self {
        OptionAny { d: Some(d) }
    }
    pub fn is_none(&self) -> bool {
        self.d.is_none()
    }
    pub fn is_some(&self) -> bool {
        !self.is_none()
    }
    pub fn set(&mut self, d: Box<dyn std::any::Any>) {
        self.d = Some(d);
    }

    pub fn get<T: 'static>(&self) -> &T {
        self.d.as_ref().unwrap().downcast_ref::<T>().unwrap()
    }

    pub fn get_mut<T: 'static>(&mut self) -> &mut T {
        self.d.as_mut().unwrap().downcast_mut::<T>().unwrap()
    }

    pub unsafe fn take<T: 'static>(&mut self) -> T {
        *self.d.take().unwrap().downcast::<T>().unwrap()
    }
}

pub struct OptionExt<T> {
    d: Option<T>,
}

impl<T> Clone for OptionExt<T>
where
    T: Clone,
{
    #[inline]
    fn clone(&self) -> Self {
        match &self.d {
            Some(x) => OptionExt::new(x.clone()),
            None => OptionExt::default(),
        }
    }
}

impl<T> From<Option<T>> for OptionExt<T> {
    fn from(data: Option<T>) -> OptionExt<T> {
        OptionExt::new_option(data)
    }
}

impl<T> OptionExt<T> {
    pub fn default() -> Self {
        OptionExt { d: None }
    }
    pub fn new(d: T) -> Self {
        OptionExt { d: Some(d) }
    }
    pub fn new_option(d: Option<T>) -> Self {
        OptionExt { d }
    }
    pub fn set_nil(&mut self) {
        self.d = None;
    }

    pub fn is_none(&self) -> bool {
        self.d.is_none()
    }
    pub fn is_some(&self) -> bool {
        !self.is_none()
    }

    pub fn set(&mut self, d: T) {
        self.d = Some(d);
    }

    pub fn get(&self) -> &T {
        self.d.as_ref().unwrap()
    }

    pub fn get_mut(&mut self) -> &mut T {
        self.d.as_mut().unwrap()
    }

    pub fn unwrap(self) -> T {
        match self.d {
            Some(val) => val,
            None => panic!("called `Option::unwrap()` on a `None` value"),
        }
    }

    pub fn as_ref(&self) -> &T {
        match &self.d {
            Some(val) => val,
            None => panic!("called `Option::unwrap()` on a `None` value"),
        }
    }

    pub unsafe fn take(&mut self) -> T {
        self.d.take().unwrap()
    }

    pub unsafe fn take_op(&mut self) -> Option<T> {
        self.d.take()
    }
}

impl<T> std::ops::Deref for OptionExt<T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.d.as_ref().unwrap()
    }
}

impl<T> std::ops::DerefMut for OptionExt<T> {
    fn deref_mut(&mut self) -> &mut T {
        self.d.as_mut().unwrap()
    }
}

pub struct OptionArcx<T> {
    d: Option<Arc<T>>,
}

impl<T> Clone for OptionArcx<T> {
    #[inline]
    fn clone(&self) -> Self {
        match &self.d {
            Some(x) => OptionArcx::new_arc(x.clone()),
            None => OptionArcx::default(),
        }
    }
}

impl<T> From<Option<T>> for OptionArcx<T> {
    fn from(data: Option<T>) -> OptionArcx<T> {
        OptionArcx::new_option(data)
    }
}

impl<T> OptionArcx<T> {
    pub fn default() -> Self {
        OptionArcx { d: None }
    }
    pub fn new(d: T) -> Self {
        OptionArcx {
            d: Some(Arc::new(d)),
        }
    }
    pub fn new_arc(d: Arc<T>) -> Self {
        OptionArcx { d: Some(d) }
    }
    pub fn new_option(d: Option<T>) -> Self {
        match d {
            Some(data) => OptionArcx::new(data),
            None => OptionArcx::default(),
        }
    }
    pub fn set_nil(&mut self) {
        self.d = None;
    }

    pub fn is_none(&self) -> bool {
        self.d.is_none()
    }
    pub fn is_some(&self) -> bool {
        !self.is_none()
    }

    pub fn strong_count(&self) -> usize {
        match &self.d {
            Some(x) => Arc::strong_count(x),
            None => 0,
        }
    }

    pub fn set(&mut self, d: T) {
        self.d = Some(Arc::new(d));
    }

    pub fn get(&self) -> &T {
        self.d.as_ref().unwrap()
    }

    pub fn unwrap(self) -> Arc<T> {
        match self.d {
            Some(val) => val,
            None => panic!("called `Option::unwrap()` on a `None` value"),
        }
    }

    pub fn as_ref(&self) -> &T {
        match &self.d {
            Some(val) => val,
            None => panic!("called `Option::unwrap()` on a `None` value"),
        }
    }

    pub unsafe fn take(&mut self) -> Arc<T> {
        self.d.take().unwrap()
    }

    pub unsafe fn take_op(&mut self) -> Option<Arc<T>> {
        self.d.take()
    }
}

impl<T> std::ops::Deref for OptionArcx<T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.d.as_ref().unwrap()
    }
}

pub struct RcRef<'a, T: 'a> {
    d: Ref<'a, Option<T>>,
}

impl<'a, T: 'a> RcRef<'a, T> {
    pub fn new(d: Ref<'a, Option<T>>) -> Self {
        RcRef { d }
    }
    pub fn is_none(&self) -> bool {
        self.d.is_none()
    }
    pub fn is_some(&self) -> bool {
        !self.is_none()
    }
}

impl<'a, T: 'a> std::ops::Deref for RcRef<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.d.deref().as_ref().unwrap()
    }
}

pub struct RcRefMut<'a, T: 'a> {
    d: RefMut<'a, Option<T>>,
}

impl<'a, T: 'a> RcRefMut<'a, T> {
    pub fn new(d: RefMut<'a, Option<T>>) -> Self {
        RcRefMut { d }
    }
    pub fn is_none(&self) -> bool {
        self.d.is_none()
    }
    pub fn is_some(&self) -> bool {
        !self.is_none()
    }
}

impl<'a, T: 'a> std::ops::Deref for RcRefMut<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.d.deref().as_ref().unwrap()
    }
}

impl<'a, T: 'a> std::ops::DerefMut for RcRefMut<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.d.deref_mut().as_mut().unwrap()
    }
}

pub struct RcCell<T> {
    d: Rc<RefCell<Option<T>>>,
}

impl<T> Clone for RcCell<T> {
    fn clone(&self) -> Self {
        RcCell { d: self.d.clone() }
    }
}

impl<T> RcCell<T> {
    pub fn default() -> Self {
        RcCell {
            d: Rc::new(RefCell::new(None)),
        }
    }
    pub fn new(d: T) -> Self {
        RcCell {
            d: Rc::new(RefCell::new(Some(d))),
        }
    }

    pub fn set_nil(&self) {
        *self.d.borrow_mut() = None;
    }

    pub fn is_none(&self) -> bool {
        self.d.borrow().is_none()
    }
    pub fn is_some(&self) -> bool {
        !self.is_none()
    }

    pub fn set(&self, d: T) {
        *self.d.borrow_mut() = Some(d);
    }

    pub fn get(&self) -> RcRef<T> {
        RcRef::new(self.d.borrow())
    }

    pub fn get_mut(&self) -> RcRefMut<T> {
        RcRefMut::new(self.d.borrow_mut())
    }

    pub unsafe fn take(&self) -> Option<T> {
        self.d.borrow_mut().take()
    }
}

#[cfg(any(debug_assertions, feature = "anylock-time"))]
fn del_write_lock_tokio(info: &Arc<Mutex<MutexInfoTokio>>) {
    let info = &mut *info.lock();
    info.w_thread_id = None;
}
#[cfg(any(debug_assertions, feature = "anylock-time"))]
fn del_read_lock_tokio(info: &Arc<Mutex<MutexInfoTokio>>) {
    let info = &mut *info.lock();
    let thread_id = spawn_str_id();
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
