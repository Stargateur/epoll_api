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

// TODO write a macro_delc! for Ptr Fd U32 and U64

/// This represent Arc mode
#[derive(Debug, Copy, Clone)]
pub struct ArcPtr<T> {
    phantom: PhantomData<Arc<T>>,
}
impl<T> DataKind for ArcPtr<T> {}

/// This represent Rc mode
#[derive(Debug, Copy, Clone)]
pub struct RcPtr<T> {
    phantom: PhantomData<Rc<T>>,
}
impl<T> DataKind for RcPtr<T> {}

/// Data is used to represent user data in EPoll
/// You can only choice from 4 types Ptr<T>, Fd, U32, U64
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

/// This represent Fd mode
#[derive(Debug, Copy, Clone)]
pub struct Fd;
impl DataKind for Fd {}

impl Data<Fd> {
    pub fn new_fd(fd: RawFd) -> Self {
        Self {
            raw: RawData { fd },
            data_kind: PhantomData,
        }
    }

    pub fn fd(&self) -> RawFd {
        unsafe { self.raw.fd }
    }
}

impl AsRawFd for Data<Fd> {
    fn as_raw_fd(&self) -> RawFd {
        self.fd()
    }
}

impl Clone for Data<Fd> {
    fn clone(&self) -> Self {
        Self::new_fd(self.fd())
    }
}

impl Debug for Data<Fd> {
    fn fmt(
        &self,
        f: &mut Formatter<'_>,
    ) -> fmt::Result {
        f.debug_struct("Data<Fd>")
            .field("raw", &self.fd())
            .field("data_kind", &self.data_kind)
            .finish()
    }
}

/// This represent U32 mode
#[derive(Debug, Copy, Clone)]
pub struct U32;
impl DataKind for U32 {}

impl Data<U32> {
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

impl Clone for Data<U32> {
    fn clone(&self) -> Self {
        Self::new_u32(self._u32())
    }
}

impl Debug for Data<U32> {
    fn fmt(
        &self,
        f: &mut Formatter<'_>,
    ) -> fmt::Result {
        f.debug_struct("Data<U32>")
            .field("raw", &self._u32())
            .field("data_kind", &self.data_kind)
            .finish()
    }
}

/// This represent U64 mode
#[derive(Debug, Copy, Clone)]
pub struct U64;
impl DataKind for U64 {}

impl Data<U64> {
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

impl Clone for Data<U64> {
    fn clone(&self) -> Self {
        Self::new_u64(self._u64())
    }
}

impl Debug for Data<U64> {
    fn fmt(
        &self,
        f: &mut Formatter<'_>,
    ) -> fmt::Result {
        f.debug_struct("Data<U64>")
            .field("raw", &self._u64())
            .field("data_kind", &self.data_kind)
            .finish()
    }
}

/// This represent Ptr mode
#[derive(Debug, Copy, Clone)]
pub struct Ptr<T> {
    phantom: PhantomData<*mut T>,
}
impl<T> DataKind for Ptr<T> {}

impl<T> Data<Ptr<T>> {
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

impl<T: Debug> Debug for Data<Ptr<T>> {
    fn fmt(
        &self,
        f: &mut Formatter<'_>,
    ) -> fmt::Result {
        f.debug_struct("Data<Ptr<T>>")
            .field("ptr", &self.ptr())
            .field("data_kind", &self.data_kind)
            .finish()
    }
}

/// This represent Box mode
#[derive(Debug, Copy, Clone)]
pub struct BoxPtr<T> {
    phantom: PhantomData<Box<T>>,
}
impl<T> DataKind for BoxPtr<T> {}

impl<T> Data<BoxPtr<T>> {
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

impl<T: Debug> Debug for Data<BoxPtr<T>> {
    fn fmt(
        &self,
        f: &mut Formatter<'_>,
    ) -> fmt::Result {
        f.debug_struct("Data<BoxPtr<T>>")
            .field("inner", self.as_ref())
            .field("data_kind", &self.data_kind)
            .finish()
    }
}

impl<T: Clone> Clone for Data<BoxPtr<T>> {
    fn clone(&self) -> Self {
        Self::new_box(self.as_ref().clone())
    }
}

impl<T> Borrow<T> for Data<BoxPtr<T>> {
    fn borrow(&self) -> &T {
        self.as_ref()
    }
}

impl<T> BorrowMut<T> for Data<BoxPtr<T>> {
    fn borrow_mut(&mut self) -> &mut T {
        self.as_mut()
    }
}

impl<T> AsRef<T> for Data<BoxPtr<T>> {
    fn as_ref(&self) -> &T {
        unsafe { &*(self.raw.ptr as *const T) }
    }
}

impl<T> AsMut<T> for Data<BoxPtr<T>> {
    fn as_mut(&mut self) -> &mut T {
        unsafe { &mut *(self.raw.ptr as *mut T) }
    }
}

impl<T> Deref for Data<BoxPtr<T>> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.as_ref()
    }
}

impl<T> DerefMut for Data<BoxPtr<T>> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut()
    }
}

impl<T> From<Box<T>> for Data<BoxPtr<T>> {
    fn from(t: Box<T>) -> Self {
        Self::new_box(t)
    }
}
