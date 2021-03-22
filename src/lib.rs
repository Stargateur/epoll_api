pub mod data_kind;

mod timeout;
pub use timeout::TimeOut;

mod max_events;
pub use max_events::MaxEvents;

pub mod utils;

pub use epoll::{ControlOptions, Events as Flags};

use std::{
    collections::hash_map::{Entry, HashMap},
    fmt::{self, Debug, Formatter},
    io::{self, ErrorKind},
    mem::MaybeUninit,
    os::unix::io::{AsRawFd, RawFd},
    ptr::null_mut,
};

use data_kind::*;

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
    flags: Flags,
    data: Data<T>,
}

static_assertions::assert_eq_size!(
    Event<DataPtr<()>>,
    Event<DataFd>,
    Event<DataU32>,
    Event<DataU64>,
    RawEvent,
);

static_assertions::assert_eq_align!(
    u8,
    Event<DataPtr<()>>,
    Event<DataFd>,
    Event<DataU32>,
    Event<DataU64>,
    RawEvent,
);

impl<T: DataKind> Event<T> {
    pub fn new(
        flags: Flags,
        data: Data<T>,
    ) -> Self {
        Self { flags, data }
    }

    pub fn flags(&self) -> Flags {
        self.flags
    }

    pub fn data(&self) -> &Data<T> {
        // https://github.com/rust-lang/rust/issues/46043
        // it's safe cause Event align is 1
        unsafe { &self.data }
    }

    pub fn data_mut(&mut self) -> &mut Data<T> {
        // https://github.com/rust-lang/rust/issues/46043
        // it's safe cause Event align is 1
        unsafe { &mut self.data }
    }

    pub fn into_data(self) -> Data<T> {
        self.data
    }
}

impl<T: DataKind> Clone for Event<T>
where
    Data<T>: Clone,
{
    fn clone(&self) -> Self {
        Self::new(self.flags, self.data().clone())
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
            .field("flags", &self.flags())
            .field("data", self.data())
            .finish()
    }
}

/// This represent an EPoll instance
/// You will need to choice between 4 datas types
/// RawFd, u32, u64, DataPtr<T>
/// This is enforced cause epoll doesn't allow to diffenciate
/// the union its use internally to stock user data
/// and anyway mix between data type don't make much sense
///
/// This will disallow any miss use about the union at compile time
///
/// Notice that while this is safe this currently can't prevent leak
/// You will need to handle this a little yourself by calling `into_inner()`
/// when you use the DataPtr<T> type
pub struct EPoll<T: DataKind> {
    api: Api<T>,
    buffer: Vec<MaybeUninit<Event<T>>>,
}

impl<T: DataKind> AsRawFd for EPoll<T> {
    fn as_raw_fd(&self) -> RawFd {
        self.api.as_raw_fd()
    }
}

impl<T: DataKind> Debug for EPoll<T>
where
    Data<T>: Debug,
{
    fn fmt(
        &self,
        f: &mut Formatter<'_>,
    ) -> fmt::Result {
        write!(f, "{:?}", self.api)
    }
}

impl<T> EPoll<DataBox<T>> {
    pub fn drop(self) {
        let (_, datas) = self.close();

        for (_, data) in datas {
            data.into_box();
        }
    }
}

pub trait EPollApi<T: DataKind> {
    /// Safe wrapper to add an event for `libc::epoll_ctl`
    fn add(
        &mut self,
        fd: RawFd,
        event: Event<T>,
    ) -> io::Result<&mut Data<T>>;

    fn mod_flags(
        &mut self,
        fd: RawFd,
        flags: Flags,
    ) -> io::Result<()>;

    /// This return all data associed with this epoll fd
    /// You CAN'T modify direclt Event<T> the only thing you can modify
    /// is Event<DataPtr<T>> because it's a reference
    /// if you want modify the direct value of Event<T>
    /// you will need to use `ctl_mod()`
    fn get_datas(&self) -> &HashMap<RawFd, Data<T>>;

    fn get_data_mut(
        &mut self,
        fd: RawFd,
    ) -> Option<&mut Data<T>>;
}

pub struct Wait<'a, T: DataKind> {
    pub api: &'a mut Api<T>,
    pub events: &'a mut [Event<T>],
}

impl<'a, T: DataKind> Wait<'a, T> {
    fn new(
        api: &'a mut Api<T>,
        events: &'a mut [Event<T>],
    ) -> Self {
        Self { api, events }
    }
}
pub struct Api<T: DataKind> {
    fd: RawFd,
    datas: HashMap<RawFd, Data<T>>,
}

impl<T: DataKind> AsRawFd for Api<T> {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

impl<T: DataKind> Debug for Api<T>
where
    Data<T>: Debug,
{
    fn fmt(
        &self,
        f: &mut Formatter<'_>,
    ) -> fmt::Result {
        f.debug_struct("Api")
            .field("fd", &self.fd)
            .field("datas", &self.datas)
            .finish()
    }
}

impl<T: DataKind> Api<T> {
    fn new(fd: RawFd) -> Self {
        Self {
            fd,
            datas: Default::default(),
        }
    }
}

impl<T: DataKind> EPollApi<T> for Api<T> {
    fn add(
        &mut self,
        fd: RawFd,
        mut event: Event<T>,
    ) -> io::Result<&mut Data<T>> {
        match self.datas.entry(fd) {
            Entry::Occupied(_) => Err(ErrorKind::AlreadyExists.into()),
            Entry::Vacant(v) => {
                let op = ControlOptions::EPOLL_CTL_ADD as i32;
                let event_ptr = &mut event as *mut _ as *mut libc::epoll_event;

                if unsafe { libc::epoll_ctl(self.fd, op, fd, event_ptr) } < 0 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(v.insert(event.into_data()))
                }
            }
        }
    }

    fn mod_flags(
        &mut self,
        fd: RawFd,
        flags: Flags,
    ) -> io::Result<()> {
        match self.datas.entry(fd) {
            Entry::Occupied(o) => {
                let data = o.into_mut().raw();
                let flags = flags.bits();

                let mut raw_event = RawEvent { flags, data };
                let event = &mut raw_event as *mut _ as *mut libc::epoll_event;
                let op = ControlOptions::EPOLL_CTL_MOD as i32;

                if unsafe { libc::epoll_ctl(self.fd, op, fd, event) } < 0 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(())
                }
            }
            Entry::Vacant(_) => Err(ErrorKind::NotFound.into()),
        }
    }

    fn get_datas(&self) -> &HashMap<RawFd, Data<T>> {
        &self.datas
    }

    fn get_data_mut(
        &mut self,
        fd: RawFd,
    ) -> Option<&mut Data<T>> {
        self.datas.get_mut(&fd)
    }
}

impl<T: DataKind> EPollApi<T> for EPoll<T> {
    fn add(
        &mut self,
        fd: RawFd,
        event: Event<T>,
    ) -> io::Result<&mut Data<T>> {
        self.api.add(fd, event)
    }

    fn mod_flags(
        &mut self,
        fd: RawFd,
        flags: Flags,
    ) -> io::Result<()> {
        self.api.mod_flags(fd, flags)
    }

    fn get_datas(&self) -> &HashMap<RawFd, Data<T>> {
        self.api.get_datas()
    }

    fn get_data_mut(
        &mut self,
        fd: RawFd,
    ) -> Option<&mut Data<T>> {
        self.api.get_data_mut(fd)
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
    pub fn new<N: Into<MaxEvents>>(
        close_exec: bool,
        max_events: N,
    ) -> io::Result<Self> {
        let max_events = max_events.into();
        #[cfg(feature = "log")]
        log::trace!(
            "new: close_exec: {:?}, max_events: {:?}",
            close_exec,
            max_events
        );
        let fd = epoll::create(close_exec)?;

        Ok(Self {
            api: Api::new(fd),
            buffer: Vec::with_capacity(max_events.into()),
        })
    }

    /// Safe wrapper to modify an event for `libc::epoll_ctl`
    /// return the old value
    pub fn mod_event(
        &mut self,
        fd: RawFd,
        mut event: Event<T>,
    ) -> io::Result<(Data<T>, &mut Data<T>)> {
        match self.api.datas.entry(fd) {
            Entry::Occupied(o) => {
                let new = &mut event as *mut _ as *mut libc::epoll_event;
                let op = ControlOptions::EPOLL_CTL_MOD as i32;

                if unsafe { libc::epoll_ctl(self.api.fd, op, fd, new) } < 0 {
                    Err(io::Error::last_os_error())
                } else {
                    let data = o.into_mut();
                    let old = std::mem::replace(data, event.into_data());
                    Ok((old, data))
                }
            }
            Entry::Vacant(_) => Err(ErrorKind::NotFound.into()),
        }
    }

    /// Safe wrapper to delete an event for `libc::epoll_ctl`
    pub fn del(
        &mut self,
        fd: RawFd,
    ) -> io::Result<Data<T>> {
        match self.api.datas.entry(fd) {
            Entry::Occupied(o) => {
                let event = null_mut() as *mut libc::epoll_event;
                let op = ControlOptions::EPOLL_CTL_DEL as i32;

                if unsafe { libc::epoll_ctl(self.api.fd, op, fd, event) } < 0 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(o.remove())
                }
            }
            Entry::Vacant(_) => Err(ErrorKind::NotFound.into()),
        }
    }

    /// Safe wrapper for `libc::close`
    /// this will return the datas
    /// For Event<DataPtr<T>> only if you want to free ressource
    /// you will need to call `Event<DataPtr<T>>::into_inner()`
    /// This could be improve if we could specialize Drop
    /// https://github.com/rust-lang/rust/issues/46893
    pub fn close(self) -> (io::Result<()>, HashMap<RawFd, Data<T>>) {
        let ret = if unsafe { libc::close(self.as_raw_fd()) } < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        };

        (ret, self.api.datas)
    }

    /// Safe wrapper for `libc::epoll_wait`
    /// The timeout argument is in milliseconds
    ///
    /// ## Notes
    ///
    /// * If `timeout` is negative, it will block until an event is received.
    pub fn wait<N: Into<TimeOut>>(
        &mut self,
        timeout: N,
    ) -> io::Result<Wait<T>> {
        #[cfg(feature = "log")]
        log::trace!("=> wait");
        unsafe {
            let ret = libc::epoll_wait(
                self.as_raw_fd(),
                self.buffer.as_mut_ptr() as *mut libc::epoll_event,
                self.buffer.capacity() as libc::c_int,
                timeout.into().into(),
            );

            if ret < 0 {
                let e = io::Error::last_os_error();
                #[cfg(feature = "log")]
                {
                    log::error!("{}", e);
                    log::trace!("<= wait");
                }
                Err(e)
            } else {
                let num_events = ret as usize;
                self.buffer.set_len(num_events);

                // https://doc.rust-lang.org/std/mem/union.MaybeUninit.html#method.slice_assume_init_ref
                let buffer = &mut *(self.buffer.as_mut_slice() as *mut _ as *mut [Event<T>]);

                let wait = Wait::new(&mut self.api, buffer);
                #[cfg(feature = "log")]
                log::trace!("<= wait");
                Ok(wait)
            }
        }
    }

    /// This resize the buffer used to recieve event
    pub fn resize_buffer<N: Into<MaxEvents>>(
        &mut self,
        max_events: N,
    ) {
        self.buffer
            .resize_with(max_events.into().into(), MaybeUninit::uninit);
        self.buffer.shrink_to_fit();
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
        let epoll = EPoll::new(close_exec, max_events).unwrap();

        let ret = is_epoll_fd_close_exec(epoll.as_raw_fd());
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
        create::<DataU32>(false, 42);
    }

    #[test]
    fn create_true() {
        create::<DataU32>(true, 42);
    }

    #[test]
    fn create_with_zero() {
        create::<DataU32>(false, 0);
    }

    #[test]
    fn create_with_one() {
        create::<DataU32>(false, 1);
    }

    #[test]
    #[should_panic]
    fn create_with_max() {
        create::<DataU32>(false, usize::MAX);
    }
}
