use std::sync::{Arc, Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};

pub struct ArcMutexReadGuard<'a, T: 'a> {
    d: MutexGuard<'a, Option<T>>,
}

impl<'a, T: 'a> ArcMutexReadGuard<'a, T> {
    pub fn new(d: MutexGuard<'a, Option<T>>) -> Self {
        ArcMutexReadGuard { d }
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

pub struct ArcMutexWriteGuard<'a, T: 'a> {
    d: MutexGuard<'a, Option<T>>,
}

impl<'a, T: 'a> ArcMutexWriteGuard<'a, T> {
    pub fn new(d: MutexGuard<'a, Option<T>>) -> Self {
        ArcMutexWriteGuard { d }
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

pub struct ArcMutexReadOptionGuard<'a, T: 'a> {
    d: MutexGuard<'a, T>,
}

impl<'a, T: 'a> ArcMutexReadOptionGuard<'a, T> {
    pub fn new(d: MutexGuard<'a, T>) -> Self {
        ArcMutexReadOptionGuard { d }
    }
}

impl<'a, T: 'a> std::ops::Deref for ArcMutexReadOptionGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.d.deref()
    }
}

pub struct ArcMutexWriteOptionGuard<'a, T: 'a> {
    d: MutexGuard<'a, T>,
}

impl<'a, T: 'a> ArcMutexWriteOptionGuard<'a, T> {
    pub fn new(d: MutexGuard<'a, T>) -> Self {
        ArcMutexWriteOptionGuard { d }
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

pub struct ArcMutex<T> {
    d: Arc<Mutex<Option<T>>>,
}

impl<T> Clone for ArcMutex<T> {
    fn clone(&self) -> Self {
        ArcMutex { d: self.d.clone() }
    }
}

impl<T> ArcMutex<T> {
    pub fn default() -> Self {
        ArcMutex {
            d: Arc::new(Mutex::new(None)),
        }
    }
    pub fn new(d: T) -> Self {
        ArcMutex {
            d: Arc::new(Mutex::new(Some(d))),
        }
    }

    pub fn set_nil(&self) {
        *self.d.lock().unwrap() = None;
    }

    pub fn is_none(&self) -> bool {
        self.d.lock().unwrap().is_none()
    }
    pub fn is_some(&self) -> bool {
        !self.is_none()
    }

    pub fn set(&self, d: T) {
        *self.d.lock().unwrap() = Some(d);
    }

    pub fn get(&self) -> ArcMutexReadGuard<T> {
        ArcMutexReadGuard::new(self.d.lock().unwrap())
    }
    pub fn get_mut(&self) -> ArcMutexWriteGuard<T> {
        ArcMutexWriteGuard::new(self.d.lock().unwrap())
    }

    pub fn get_option(&self) -> ArcMutexReadOptionGuard<Option<T>> {
        ArcMutexReadOptionGuard::new(self.d.lock().unwrap())
    }
    pub fn get_option_mut(&self) -> ArcMutexWriteOptionGuard<Option<T>> {
        ArcMutexWriteOptionGuard::new(self.d.lock().unwrap())
    }

    pub unsafe fn take(&self) -> T {
        self.d.lock().unwrap().take().unwrap()
    }
}

pub struct ArcMutexBox<T> {
    d: Arc<Mutex<Option<Box<T>>>>,
}

impl<T> Clone for ArcMutexBox<T> {
    fn clone(&self) -> Self {
        ArcMutexBox { d: self.d.clone() }
    }
}

impl<T> ArcMutexBox<T> {
    pub fn default() -> Self {
        ArcMutexBox {
            d: Arc::new(Mutex::new(None)),
        }
    }
    pub fn new(d: T) -> Self {
        ArcMutexBox {
            d: Arc::new(Mutex::new(Some(Box::new(d)))),
        }
    }

    pub fn set_nil(&self) {
        *self.d.lock().unwrap() = None;
    }

    pub fn is_none(&self) -> bool {
        self.d.lock().unwrap().is_none()
    }
    pub fn is_some(&self) -> bool {
        !self.is_none()
    }

    pub fn set(&self, d: T) {
        *self.d.lock().unwrap() = Some(Box::new(d));
    }

    pub fn get(&self) -> ArcMutexReadGuard<Box<T>> {
        ArcMutexReadGuard::<Box<T>>::new(self.d.lock().unwrap())
    }
    pub fn get_mut(&self) -> ArcMutexWriteGuard<Box<T>> {
        ArcMutexWriteGuard::<Box<T>>::new(self.d.lock().unwrap())
    }

    pub unsafe fn take(&self) -> T {
        *self.d.lock().unwrap().take().unwrap()
    }
}

pub struct ArcRwLockReadGuard<'a, T: 'a> {
    d: RwLockReadGuard<'a, Option<T>>,
}

impl<'a, T: 'a> ArcRwLockReadGuard<'a, T> {
    pub fn new(d: RwLockReadGuard<'a, Option<T>>) -> Self {
        ArcRwLockReadGuard { d }
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

pub struct ArcRwLockWriteGuard<'a, T: 'a> {
    d: RwLockWriteGuard<'a, Option<T>>,
}

impl<'a, T: 'a> ArcRwLockWriteGuard<'a, T> {
    pub fn new(d: RwLockWriteGuard<'a, Option<T>>) -> Self {
        ArcRwLockWriteGuard { d }
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

pub struct ArcRwLock<T> {
    d: Arc<RwLock<Option<T>>>,
}

impl<T> Clone for ArcRwLock<T> {
    fn clone(&self) -> Self {
        ArcRwLock { d: self.d.clone() }
    }
}

impl<T> ArcRwLock<T> {
    pub fn default() -> Self {
        ArcRwLock {
            d: Arc::new(RwLock::new(None)),
        }
    }
    pub fn new(d: T) -> Self {
        ArcRwLock {
            d: Arc::new(RwLock::new(Some(d))),
        }
    }
    pub fn set_nil(&self) {
        *self.d.write().unwrap() = None;
    }
    pub fn is_none(&self) -> bool {
        self.d.read().unwrap().is_none()
    }
    pub fn is_some(&self) -> bool {
        !self.is_none()
    }

    pub fn set(&self, d: T) {
        *self.d.write().unwrap() = Some(d);
    }

    pub fn get(&self) -> ArcRwLockReadGuard<T> {
        ArcRwLockReadGuard::new(self.d.read().unwrap())
    }
    pub fn get_mut(&self) -> ArcRwLockWriteGuard<T> {
        ArcRwLockWriteGuard::new(self.d.write().unwrap())
    }

    pub unsafe fn take(&self) -> T {
        self.d.write().unwrap().take().unwrap()
    }
}
