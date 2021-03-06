// https://github.com/Kestrer/bounded-integer/issues/5
#![allow(clippy::manual_range_contains)]

use std::{
    collections::hash_map::{Entry, HashMap},
    fmt::{self, Debug, Formatter},
    io::{self, ErrorKind},
    marker::PhantomData,
    mem::MaybeUninit,
    os::unix::io::RawFd,
    ptr::null_mut,
};

pub use epoll::{ControlOptions, Events};

bounded_integer::bounded_integer! {
    #[repr(usize)]
    pub struct MaxEvents { 1..2147483647 }
}

#[derive(Copy, Clone)]
union RawData {
    ptr: *mut libc::c_void,
    fd: RawFd,
    _u32: u32,
    _u64: u64,
}

/// Regroup DakaKind type
pub trait DataKind {}

// TODO write a macro_delc! for Ptr Fd U32 and U64

/// This represent Ptr mode
#[derive(Debug, Copy, Clone)]
pub struct Ptr<T> {
    phantom: PhantomData<*mut T>,
}
impl<T> DataKind for Ptr<T> {}

/// This represent Fd mode
#[derive(Debug, Copy, Clone)]
pub struct Fd;
impl DataKind for Fd {}

/// This represent U32 mode
#[derive(Debug, Copy, Clone)]
pub struct U32;
impl DataKind for U32 {}

/// This represent U64 mode
#[derive(Debug, Copy, Clone)]
pub struct U64;
impl DataKind for U64 {}

/// Data is used to represent user data in EPoll
/// You can only choice from 4 types Ptr<T>, Fd, U32, U64
/// use the appropriate function to create them
pub struct Data<T: DataKind> {
    raw: RawData,
    data_kind: PhantomData<T>,
}

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

impl<T> Data<Ptr<T>> {
    pub fn new_ptr(t: T) -> Self
    where
        T: Into<Box<T>>,
    {
        let ptr = Box::into_raw(t.into()) as *mut _;
        Self {
            raw: RawData { ptr },
            data_kind: PhantomData,
        }
    }

    pub fn ptr(&self) -> &T {
        unsafe { &*(self.raw.ptr as *const T) }
    }

    pub fn ptr_mut(&mut self) -> &mut T {
        unsafe { &mut *(self.raw.ptr as *mut T) }
    }

    pub fn into_inner(self) -> Box<T> {
        unsafe { Box::from_raw(self.raw.ptr as *mut T) }
    }
}

impl<T: Debug> Debug for Data<Ptr<T>> {
    fn fmt(
        &self,
        f: &mut Formatter<'_>,
    ) -> fmt::Result {
        f.debug_struct("Data<Ptr<T>>")
            .field("raw", &self.ptr())
            .field("data_kind", &self.data_kind)
            .finish()
    }
}

impl<T: Clone> Clone for Data<Ptr<T>> {
    fn clone(&self) -> Self {
        Self::new_ptr(self.ptr().clone())
    }
}

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

/// 'libc::epoll_event' equivalent.
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
pub struct Event<T: DataKind> {
    events: Events,
    data: Data<T>,
}

static_assertions::assert_eq_size!(
    libc::epoll_event,
    Event<Ptr<()>>,
    Event<Fd>,
    Event<U32>,
    Event<U64>,
);

impl<T: DataKind> Event<T> {
    pub fn new(
        events: Events,
        data: Data<T>,
    ) -> Self {
        Self { events, data }
    }

    pub fn events(&self) -> Events {
        self.events
    }

    pub fn data(&self) -> &Data<T> {
        // https://github.com/rust-lang/rust/issues/46043
        // it's safe cause Event align is 1
        static_assertions::assert_eq_align!(
            u8,
            libc::epoll_event,
            Event<Ptr<()>>,
            Event<Fd>,
            Event<U32>,
            Event<U64>,
        );
        unsafe { &self.data }
    }
}

impl<T: DataKind> Clone for Event<T>
where
    Data<T>: Clone,
{
    fn clone(&self) -> Self {
        Self::new(self.events, self.data().clone())
    }
}

impl<T: DataKind> Debug for Event<T>
where
    Data<T>: Debug,
{
    fn fmt(
        &self,
        f: &mut Formatter<'_>,
    ) -> fmt::Result {
        f.debug_struct("Event")
            .field("events", &self.events())
            .field("data", self.data())
            .finish()
    }
}

/// This represent an EPoll instance
/// You will need to choice between 4 datas types
/// RawFd, u32, u64, Ptr<T>
/// This is enforced cause epoll doesn't allow to diffenciate
/// the union its use internally to stock user data
/// and anyway mix between data type don't make much sense
///
/// This will disallow any miss use about the union at compile time
///
/// Notice that while this is safe this currently can't prevent leak
/// You will need to handle this a little yourself by calling `into_inner()`
/// when you use the Ptr<T> type
pub struct EPoll<T: DataKind> {
    fd: RawFd,
    datas: HashMap<RawFd, Event<T>>,
    buffer: Vec<MaybeUninit<Event<T>>>,
}

impl<T: DataKind> Debug for EPoll<T>
where
    Data<T>: Debug,
{
    fn fmt(
        &self,
        f: &mut Formatter<'_>,
    ) -> fmt::Result {
        f.debug_struct("EPoll")
            .field("fd", &self.fd)
            .field("datas", &self.datas)
            .finish()
    }
}

impl<T: DataKind> EPoll<T> {
    /// Creates a new epoll file descriptor.
    ///
    /// If `close_exec` is true, `FD_CLOEXEC` will be set on the file descriptor of EPoll.
    ///
    /// ## Notes
    ///
    /// * `epoll_create1()` is the underlying syscall.
    pub fn create(
        close_exec: bool,
        max_events: MaxEvents,
    ) -> io::Result<Self> {
        let fd = epoll::create(close_exec)?;

        Ok(Self {
            fd,
            datas: Default::default(),
            buffer: Vec::with_capacity(max_events.into()),
        })
    }

    /// Safe wrapper to add an event for `libc::epoll_ctl`
    pub fn ctl_add(
        &mut self,
        fd: RawFd,
        event: Event<T>,
    ) -> io::Result<()> {
        match self.datas.entry(fd) {
            Entry::Occupied(_) => Err(ErrorKind::AlreadyExists.into()),
            Entry::Vacant(v) => {
                let event = v.insert(event) as *mut _ as *mut libc::epoll_event;
                let op = ControlOptions::EPOLL_CTL_ADD as i32;

                if unsafe { libc::epoll_ctl(self.fd, op, fd, event) } < 0 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(())
                }
            }
        }
    }

    /// Safe wrapper to modify an event for `libc::epoll_ctl`
    /// return the old value
    pub fn ctl_mod(
        &mut self,
        fd: RawFd,
        mut event: Event<T>,
    ) -> io::Result<Event<T>> {
        match self.datas.entry(fd) {
            Entry::Occupied(mut o) => {
                let new = &mut event as *mut _ as *mut libc::epoll_event;
                let op = ControlOptions::EPOLL_CTL_MOD as i32;

                if unsafe { libc::epoll_ctl(self.fd, op, fd, new) } < 0 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(o.insert(event))
                }
            }
            Entry::Vacant(_) => Err(ErrorKind::NotFound.into()),
        }
    }

    /// Safe wrapper to delete an event for `libc::epoll_ctl`
    pub fn ctl_del(
        &mut self,
        fd: RawFd,
    ) -> io::Result<Event<T>> {
        match self.datas.entry(fd) {
            Entry::Occupied(o) => {
                let event = null_mut() as *mut libc::epoll_event;
                let op = ControlOptions::EPOLL_CTL_DEL as i32;

                if unsafe { libc::epoll_ctl(self.fd, op, fd, event) } < 0 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(o.remove())
                }
            }
            Entry::Vacant(_) => Err(ErrorKind::NotFound.into()),
        }
    }

    /// Safe wrapper for `libc::epoll_wait`
    /// The timeout argument is in milliseconds
    ///
    /// ## Notes
    ///
    /// * If `timeout` is negative, it will block until an event is received.
    pub fn wait(
        &mut self,
        timeout: i32,
    ) -> io::Result<&mut [Event<T>]> {
        let timeout = if timeout < 0 { -1 } else { timeout };

        let num_events = unsafe {
            let ret = libc::epoll_wait(
                self.fd,
                self.buffer.as_mut_ptr() as *mut libc::epoll_event,
                self.buffer.capacity() as i32,
                timeout,
            );
            if ret < 0 {
                return Err(io::Error::last_os_error());
            } else {
                ret as usize
            }
        };

        unsafe {
            self.buffer.set_len(num_events);
        }

        // https://doc.rust-lang.org/std/mem/union.MaybeUninit.html#method.slice_assume_init_ref
        Ok(unsafe { &mut *(self.buffer.as_mut_slice() as *mut _ as *mut [Event<T>]) })
    }

    /// This resize the buffer used to recieve event
    pub fn resize_buffer(
        &mut self,
        max_events: MaxEvents,
    ) {
        self.buffer
            .resize_with(max_events.into(), MaybeUninit::uninit);
        self.buffer.shrink_to_fit();
    }

    /// Safe wrapper for `libc::close`
    /// this will return the datas
    /// For Event<Ptr<T>> only if you want to free ressource
    /// you will need to call `Event<Ptr<T>>::into_inner()`
    /// This could be improve if we could specialize Drop
    /// https://github.com/rust-lang/rust/issues/46893
    pub fn close(self) -> io::Result<HashMap<RawFd, Event<T>>> {
        if unsafe { libc::close(self.fd) } < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(self.datas)
        }
    }

    /// This return all data associed with this epoll fd
    /// You CAN'T modify direclt Event<T> the only thing you can modify
    /// is Event<Ptr<T>> because it's a reference
    /// if you want modify the direct value of Event<T>
    /// you will need to use `ctl_mod()`
    pub fn get_data(&self) -> &HashMap<RawFd, Event<T>> {
        &self.datas
    }
}

#[cfg(test)]
mod tests_epoll {
    use crate::*;

    fn is_epoll_fd_close_exec(fd: RawFd) -> bool {
        let ret = unsafe { libc::fcntl(fd, libc::F_GETFD, 0) };
        if ret == -1 {
            panic!("fcntl return an error {}", io::Error::last_os_error());
        }

        (ret & libc::FD_CLOEXEC) == libc::FD_CLOEXEC
    }

    #[test]
    #[should_panic]
    fn bad_fd() {
        is_epoll_fd_close_exec(-1);
    }

    fn create<T: DataKind>(
        close_exec: bool,
        max_events: usize,
    ) -> EPoll<T> {
        let max_events = MaxEvents::new(max_events).unwrap();
        let epoll = EPoll::create(close_exec, max_events).unwrap();

        let ret = is_epoll_fd_close_exec(epoll.fd);
        assert_eq!(ret, close_exec, "close_exec: {}", ret);

        let capacity = epoll.buffer.capacity();
        assert!(
            capacity >= max_events,
            "max_events: {} should be >= {}",
            capacity,
            max_events
        );

        epoll
    }

    #[test]
    fn create_false() {
        create::<U32>(false, 42);
    }

    #[test]
    fn create_true() {
        create::<U32>(true, 42);
    }

    #[test]
    #[should_panic]
    fn create_with_zero() {
        create::<U32>(false, 0);
    }

    #[test]
    fn create_with_one() {
        create::<U32>(false, 1);
    }

    #[test]
    #[should_panic]
    fn create_with_max() {
        create::<U32>(false, usize::MAX);
    }
}
