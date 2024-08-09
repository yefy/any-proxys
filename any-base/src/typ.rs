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

use std::cell::Ref;
use std::cell::RefCell;
use std::cell::RefMut;
use std::cell::UnsafeCell;
use std::rc::Rc;
use std::sync::Arc;

pub type Share<T> = ArcMutex<T>;
pub type ShareRw<T> = ArcRwLock<T>;

pub struct ArcMutexReadGuardTokio<'a, T: 'a> {
    d: tokio::sync::MutexGuard<'a, Option<T>>,
}

impl<'a, T: 'a> ArcMutexReadGuardTokio<'a, T> {
    pub fn new(d: tokio::sync::MutexGuard<'a, Option<T>>) -> Self {
        ArcMutexReadGuardTokio { d }
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

pub struct ArcMutexWriteGuardTokio<'a, T: 'a> {
    d: tokio::sync::MutexGuard<'a, Option<T>>,
}

impl<'a, T: 'a> ArcMutexWriteGuardTokio<'a, T> {
    pub fn new(d: tokio::sync::MutexGuard<'a, Option<T>>) -> Self {
        ArcMutexWriteGuardTokio { d }
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

pub struct ArcMutexTokio<T> {
    d: Arc<tokio::sync::Mutex<Option<T>>>,
}

impl<T> Clone for ArcMutexTokio<T> {
    fn clone(&self) -> Self {
        ArcMutexTokio { d: self.d.clone() }
    }
}

impl<T> ArcMutexTokio<T> {
    pub fn default() -> Self {
        ArcMutexTokio {
            d: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }
    pub fn new(d: T) -> Self {
        ArcMutexTokio {
            d: Arc::new(tokio::sync::Mutex::new(Some(d))),
        }
    }

    pub fn strong_count(&self) -> usize {
        Arc::strong_count(&self.d)
    }
    pub async fn set_nil(&self) {
        *self.d.lock().await = None;
    }

    pub async fn is_none(&self) -> bool {
        self.d.lock().await.is_none()
    }
    pub async fn is_some(&self) -> bool {
        !self.is_none().await
    }

    pub async fn set(&self, d: T) {
        *self.d.lock().await = Some(d);
    }

    pub async fn get(&self) -> ArcMutexReadGuardTokio<T> {
        ArcMutexReadGuardTokio::new(self.d.lock().await)
    }
    pub async fn get_mut(&self) -> ArcMutexWriteGuardTokio<T> {
        ArcMutexWriteGuardTokio::new(self.d.lock().await)
    }

    pub async unsafe fn take(&self) -> T {
        self.d.lock().await.take().unwrap()
    }
}

pub struct ArcMutexAnyReadGuardTokio<'a> {
    d: tokio::sync::MutexGuard<'a, Option<Box<dyn std::any::Any>>>,
}

unsafe impl Send for ArcMutexAnyReadGuardTokio<'_> {}
unsafe impl Sync for ArcMutexAnyReadGuardTokio<'_> {}

impl<'a> ArcMutexAnyReadGuardTokio<'a> {
    pub fn new(d: tokio::sync::MutexGuard<'a, Option<Box<dyn std::any::Any>>>) -> Self {
        ArcMutexAnyReadGuardTokio { d }
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

pub struct ArcMutexAnyWriteGuardTokio<'a> {
    d: tokio::sync::MutexGuard<'a, Option<Box<dyn std::any::Any>>>,
}

unsafe impl Send for ArcMutexAnyWriteGuardTokio<'_> {}
unsafe impl Sync for ArcMutexAnyWriteGuardTokio<'_> {}

impl<'a> ArcMutexAnyWriteGuardTokio<'a> {
    pub fn new(d: tokio::sync::MutexGuard<'a, Option<Box<dyn std::any::Any>>>) -> Self {
        ArcMutexAnyWriteGuardTokio { d }
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

pub struct ArcMutexAnyTokio {
    d: Arc<tokio::sync::Mutex<Option<Box<dyn std::any::Any>>>>,
}

unsafe impl Send for ArcMutexAnyTokio {}
unsafe impl Sync for ArcMutexAnyTokio {}

impl Clone for ArcMutexAnyTokio {
    fn clone(&self) -> Self {
        ArcMutexAnyTokio { d: self.d.clone() }
    }
}

impl ArcMutexAnyTokio {
    pub fn default() -> Self {
        ArcMutexAnyTokio {
            d: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }
    pub fn new(d: Box<dyn std::any::Any>) -> Self {
        ArcMutexAnyTokio {
            d: Arc::new(tokio::sync::Mutex::new(Some(d))),
        }
    }

    pub fn strong_count(&self) -> usize {
        Arc::strong_count(&self.d)
    }
    pub async fn set_nil(&self) {
        *self.d.lock().await = None;
    }

    pub async fn is_none(&self) -> bool {
        self.d.lock().await.is_none()
    }
    pub async fn is_some(&self) -> bool {
        !self.is_none().await
    }

    pub async fn set(&self, d: Box<dyn std::any::Any>) {
        *self.d.lock().await = Some(d);
    }

    pub async fn get(&self) -> ArcMutexAnyReadGuardTokio {
        ArcMutexAnyReadGuardTokio::new(self.d.lock().await)
    }
    pub async fn get_mut(&self) -> ArcMutexAnyWriteGuardTokio {
        ArcMutexAnyWriteGuardTokio::new(self.d.lock().await)
    }

    pub async unsafe fn take(&self) -> Box<dyn std::any::Any> {
        self.d.lock().await.take().unwrap()
    }
}

pub struct ArcRwLockReadTokioGuard<'a, T: 'a> {
    d: tokio::sync::RwLockReadGuard<'a, Option<T>>,
}

impl<'a, T: 'a> ArcRwLockReadTokioGuard<'a, T> {
    pub fn new(d: tokio::sync::RwLockReadGuard<'a, Option<T>>) -> Self {
        ArcRwLockReadTokioGuard { d }
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

pub struct ArcRwLockWriteTokioGuard<'a, T: 'a> {
    d: tokio::sync::RwLockWriteGuard<'a, Option<T>>,
}

impl<'a, T: 'a> ArcRwLockWriteTokioGuard<'a, T> {
    pub fn new(d: tokio::sync::RwLockWriteGuard<'a, Option<T>>) -> Self {
        ArcRwLockWriteTokioGuard { d }
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

pub struct ArcRwLockTokio<T> {
    d: Arc<tokio::sync::RwLock<Option<T>>>,
}

impl<T> Clone for ArcRwLockTokio<T> {
    fn clone(&self) -> Self {
        ArcRwLockTokio { d: self.d.clone() }
    }
}

impl<T> ArcRwLockTokio<T> {
    pub fn default() -> Self {
        ArcRwLockTokio {
            d: Arc::new(tokio::sync::RwLock::new(None)),
        }
    }
    pub fn new(d: T) -> Self {
        ArcRwLockTokio {
            d: Arc::new(tokio::sync::RwLock::new(Some(d))),
        }
    }
    pub fn strong_count(&self) -> usize {
        Arc::strong_count(&self.d)
    }
    pub async fn set_nil(&self) {
        *self.d.write().await = None;
    }
    pub async fn is_none(&self) -> bool {
        self.d.read().await.is_none()
    }
    pub async fn is_some(&self) -> bool {
        !self.is_none().await
    }

    pub async fn set(&self, d: T) {
        *self.d.write().await = Some(d);
    }

    pub async fn get(&self) -> ArcRwLockReadTokioGuard<T> {
        ArcRwLockReadTokioGuard::new(self.d.read().await)
    }
    pub async fn get_mut(&self) -> ArcRwLockWriteTokioGuard<T> {
        ArcRwLockWriteTokioGuard::new(self.d.write().await)
    }

    pub async unsafe fn take(&self) -> T {
        self.d.write().await.take().unwrap()
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

    // pub fn get(&self) -> &T {
    //     self.d.as_ref().unwrap()
    // }
    //
    // pub fn get_mut(&mut self) -> &mut T {
    //     self.d.as_mut().unwrap()
    // }

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
