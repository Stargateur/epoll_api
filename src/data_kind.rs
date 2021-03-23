use std::{
    borrow::{Borrow, BorrowMut},
    fmt::{self, Debug, Formatter},
    marker::PhantomData,
    ops::{Deref, DerefMut},
    os::unix::io::{AsRawFd, RawFd},
    rc::Rc,
    sync::Arc,
};

/// The union that `epoll` define
#[derive(Copy, Clone)]
pub union RawData {
    ptr: *mut libc::c_void,
    fd: RawFd,
    _u32: u32,
    _u64: u64,
}
/// 'libc::epoll_event' should be define like this
#[repr(C)]
#[cfg_attr(
    any(
        all(
            target_arch = "x86",
            not(target_env = "musl"),
            not(target_os = "android")
        ),
        target_arch = "x86_64"
    ),
    repr(packed)
)]
pub struct RawEvent {
    pub flags: u32,
    pub data: RawData,
}

static_assertions::assert_eq_size!(libc::epoll_event, RawEvent,);

static_assertions::assert_eq_align!(u8, libc::epoll_event, RawEvent,);

/// Regroup DakaKind type
pub trait DataKind {}

// TODO write a macro_delc! for DataPtr DataFd DataU32 and DataU64

/// This represent Arc mode
#[derive(Debug, Copy, Clone)]
pub struct DataArc<T> {
    phantom: PhantomData<Arc<T>>,
}
impl<T> DataKind for DataArc<T> {}

/// This represent Rc mode
#[derive(Debug, Copy, Clone)]
pub struct DataRc<T> {
    phantom: PhantomData<Rc<T>>,
}
impl<T> DataKind for DataRc<T> {}

/// Data is used to represent user data in EPoll
/// You can only choice from 4 types DataPtr<T>, DataFd, DataU32, DataU64
/// use the appropriate function to create them
pub struct Data<T: DataKind> {
    raw: RawData,
    data_kind: PhantomData<T>,
}

impl<T: DataKind> Data<T> {
    pub fn raw(&self) -> RawData {
        self.raw
    }

    pub fn data_kind(&self) -> PhantomData<T> {
        self.data_kind
    }
}

/// This represent DataFd mode
#[derive(Debug, Copy, Clone)]
pub struct DataFd;
impl DataKind for DataFd {}

impl Data<DataFd> {
    pub fn new_fd(fd: RawFd) -> Self {
        Self {
            raw: RawData { fd },
            data_kind: PhantomData,
        }
    }

    pub fn fd(&self) -> RawFd {
        unsafe { self.raw().fd }
    }
}

impl AsRawFd for Data<DataFd> {
    fn as_raw_fd(&self) -> RawFd {
        self.fd()
    }
}

impl Clone for Data<DataFd> {
    fn clone(&self) -> Self {
        Self::new_fd(self.fd())
    }
}

impl Debug for Data<DataFd> {
    fn fmt(
        &self,
        f: &mut Formatter<'_>,
    ) -> fmt::Result {
        f.debug_struct("Data<DataFd>")
            .field("raw", &self.fd())
            .field("data_kind", &self.data_kind)
            .finish()
    }
}

/// This represent DataU32 mode
#[derive(Debug, Copy, Clone)]
pub struct DataU32;
impl DataKind for DataU32 {}

impl Data<DataU32> {
    pub fn new_u32(_u32: u32) -> Self {
        Self {
            raw: RawData { _u32 },
            data_kind: PhantomData,
        }
    }

    pub fn _u32(&self) -> u32 {
        unsafe { self.raw._u32 }
    }
}

impl Clone for Data<DataU32> {
    fn clone(&self) -> Self {
        Self::new_u32(self._u32())
    }
}

impl Debug for Data<DataU32> {
    fn fmt(
        &self,
        f: &mut Formatter<'_>,
    ) -> fmt::Result {
        f.debug_struct("Data<DataU32>")
            .field("raw", &self._u32())
            .field("data_kind", &self.data_kind)
            .finish()
    }
}

/// This represent DataU64 mode
#[derive(Debug, Copy, Clone)]
pub struct DataU64;
impl DataKind for DataU64 {}

impl Data<DataU64> {
    pub fn new_u64(_u64: u64) -> Self {
        Self {
            raw: RawData { _u64 },
            data_kind: PhantomData,
        }
    }

    pub fn _u64(&self) -> u64 {
        unsafe { self.raw._u64 }
    }
}

impl Clone for Data<DataU64> {
    fn clone(&self) -> Self {
        Self::new_u64(self._u64())
    }
}

impl Debug for Data<DataU64> {
    fn fmt(
        &self,
        f: &mut Formatter<'_>,
    ) -> fmt::Result {
        f.debug_struct("Data<DataU64>")
            .field("raw", &self._u64())
            .field("data_kind", &self.data_kind)
            .finish()
    }
}

/// This represent DataPtr mode
#[derive(Debug, Copy, Clone)]
pub struct DataPtr<T> {
    phantom: PhantomData<*mut T>,
}
impl<T> DataKind for DataPtr<T> {}

impl<T> Data<DataPtr<T>> {
    pub fn new_ptr(t: *mut T) -> Self {
        let ptr = t as *mut _;
        Self {
            raw: RawData { ptr },
            data_kind: PhantomData,
        }
    }

    pub fn ptr(&self) -> *const T {
        unsafe { self.raw.ptr as *const T }
    }

    pub fn ptr_mut(&mut self) -> *mut T {
        unsafe { self.raw.ptr as *mut T }
    }
}

impl<T: Debug> Debug for Data<DataPtr<T>> {
    fn fmt(
        &self,
        f: &mut Formatter<'_>,
    ) -> fmt::Result {
        f.debug_struct("Data<DataPtr<T>>")
            .field("ptr", &self.ptr())
            .field("data_kind", &self.data_kind)
            .finish()
    }
}

/// This represent Box mode
#[derive(Debug, Copy, Clone)]
pub struct DataBox<T> {
    phantom: PhantomData<Box<T>>,
}
impl<T> DataKind for DataBox<T> {}

impl<T> Data<DataBox<T>> {
    pub fn new_box<B>(b: B) -> Self
    where
        B: Into<Box<T>>,
    {
        let ptr = Box::into_raw(b.into()) as *mut _;
        Self {
            raw: RawData { ptr },
            data_kind: PhantomData,
        }
    }

    pub fn into_box(self) -> Box<T> {
        unsafe { Box::from_raw(self.raw.ptr as *mut T) }
    }
}

impl<T: Debug> Debug for Data<DataBox<T>> {
    fn fmt(
        &self,
        f: &mut Formatter<'_>,
    ) -> fmt::Result {
        f.debug_struct("Data<DataBox<T>>")
            .field("inner", self.as_ref())
            .field("data_kind", &self.data_kind)
            .finish()
    }
}

impl<T: Clone> Clone for Data<DataBox<T>> {
    fn clone(&self) -> Self {
        Self::new_box(self.as_ref().clone())
    }
}

impl<T> Borrow<T> for Data<DataBox<T>> {
    fn borrow(&self) -> &T {
        self.as_ref()
    }
}

impl<T> BorrowMut<T> for Data<DataBox<T>> {
    fn borrow_mut(&mut self) -> &mut T {
        self.as_mut()
    }
}

impl<T> AsRef<T> for Data<DataBox<T>> {
    fn as_ref(&self) -> &T {
        unsafe { &*(self.raw.ptr as *const T) }
    }
}

impl<T> AsMut<T> for Data<DataBox<T>> {
    fn as_mut(&mut self) -> &mut T {
        unsafe { &mut *(self.raw.ptr as *mut T) }
    }
}

impl<T> Deref for Data<DataBox<T>> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.as_ref()
    }
}

impl<T> DerefMut for Data<DataBox<T>> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut()
    }
}

impl<T> From<Box<T>> for Data<DataBox<T>> {
    fn from(t: Box<T>) -> Self {
        Self::new_box(t)
    }
}

#[cfg(test)]
mod tests {}
