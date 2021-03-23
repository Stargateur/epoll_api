pub mod data_kind;

mod time_out;
pub use time_out::TimeOut;

mod max_events;
pub use max_events::MaxEvents;

pub mod utils;

use epoll::ControlOptions;
pub use epoll::Events as Flags;

use data_kind::*;

use std::{
    collections::hash_map::{Entry, HashMap},
    fmt::{self, Debug, Formatter},
    io::{self, ErrorKind},
    mem::MaybeUninit,
    os::unix::io::{AsRawFd, RawFd},
    ptr::null_mut,
};

use snafu::Snafu;

use tracing::{info, instrument};

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
    fn add<Fd: AsRawFd>(
        &mut self,
        fd: Fd,
        event: Event<T>,
    ) -> io::Result<&mut Data<T>>;

    fn mod_flags<Fd: AsRawFd>(
        &mut self,
        fd: Fd,
        flags: Flags,
    ) -> io::Result<()>;

    /// This return all data associed with this epoll fd
    /// You CAN'T modify direclt Event<T> the only thing you can modify
    /// is Event<DataPtr<T>> because it's a reference
    /// if you want modify the direct value of Event<T>
    /// you will need to use `ctl_mod()`
    fn get_datas(&self) -> &HashMap<RawFd, Data<T>>;

    fn get_data_mut<Fd: AsRawFd>(
        &mut self,
        fd: Fd,
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
    #[instrument(skip(self, fd, event), level = "trace")]
    fn add<Fd: AsRawFd>(
        &mut self,
        fd: Fd,
        mut event: Event<T>,
    ) -> io::Result<&mut Data<T>> {
        let fd = fd.as_raw_fd();
        info!(self.fd, fd, flags = ?event.flags());

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

    #[instrument(skip(self, fd, flags), level = "trace")]
    fn mod_flags<Fd: AsRawFd>(
        &mut self,
        fd: Fd,
        flags: Flags,
    ) -> io::Result<()> {
        let fd = fd.as_raw_fd();
        info!(self.fd, fd, ?flags);

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

    fn get_data_mut<Fd: AsRawFd>(
        &mut self,
        fd: Fd,
    ) -> Option<&mut Data<T>> {
        let fd = fd.as_raw_fd();

        self.datas.get_mut(&fd)
    }
}

impl<T: DataKind> EPollApi<T> for EPoll<T> {
    fn add<Fd: AsRawFd>(
        &mut self,
        fd: Fd,
        event: Event<T>,
    ) -> io::Result<&mut Data<T>> {
        self.api.add(fd, event)
    }

    fn mod_flags<Fd: AsRawFd>(
        &mut self,
        fd: Fd,
        flags: Flags,
    ) -> io::Result<()> {
        self.api.mod_flags(fd, flags)
    }

    fn get_datas(&self) -> &HashMap<RawFd, Data<T>> {
        self.api.get_datas()
    }

    fn get_data_mut<Fd: AsRawFd>(
        &mut self,
        fd: Fd,
    ) -> Option<&mut Data<T>> {
        self.api.get_data_mut(fd)
    }
}

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("IO: code: {} source: {}", code, source))]
    IO {
        code: libc::c_int,
        source: std::io::Error,
    },

    #[snafu(display("Fd: {} is not register in this EPoll instance", fd))]
    FdNotFound { fd: RawFd },
}

impl From<libc::c_int> for Error {
    fn from(code: libc::c_int) -> Self {
        Self::IO {
            code,
            source: io::Error::last_os_error(),
        }
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
    #[instrument(skip(close_exec, max_events) level = "trace")]
    pub fn new<N: Into<MaxEvents>>(
        close_exec: bool,
        max_events: N,
    ) -> Result<Self, Error> {
        let max_events = max_events.into();
        info!(close_exec, ?max_events);
        let max_events = max_events.into();

        let flags = if close_exec { libc::EPOLL_CLOEXEC } else { 0 };
        let ret = unsafe { libc::epoll_create1(flags) };

        if ret < 0 {
            Err(ret.into())
        } else {
            Ok(Self {
                api: Api::new(ret),
                buffer: Vec::with_capacity(max_events),
            })
        }
    }

    /// Safe wrapper to modify an event for `libc::epoll_ctl`
    /// return the old value
    #[instrument(skip(self, event, fd), level = "trace")]
    pub fn mod_event<Fd: AsRawFd>(
        &mut self,
        fd: Fd,
        mut event: Event<T>,
    ) -> Result<(Data<T>, &mut Data<T>), Error> {
        let fd = fd.as_raw_fd();
        info!(self.api.fd, fd);

        match self.api.datas.entry(fd) {
            Entry::Occupied(o) => {
                let new = &mut event as *mut _ as *mut libc::epoll_event;
                let op = ControlOptions::EPOLL_CTL_MOD as i32;

                let ret = unsafe { libc::epoll_ctl(self.api.fd, op, fd, new) };
                if ret < 0 {
                    Err(ret.into())
                } else {
                    let data = o.into_mut();
                    let old = std::mem::replace(data, event.into_data());
                    Ok((old, data))
                }
            }
            Entry::Vacant(_) => Err(Error::FdNotFound { fd }),
        }
    }

    /// Safe wrapper to delete an event for `libc::epoll_ctl`
    #[instrument(skip(self, fd), level = "trace")]
    pub fn del<Fd: AsRawFd>(
        &mut self,
        fd: Fd,
    ) -> Result<Data<T>, Error> {
        let fd = fd.as_raw_fd();
        info!(self.api.fd, fd);

        match self.api.datas.entry(fd) {
            Entry::Occupied(o) => {
                let event = null_mut() as *mut libc::epoll_event;
                let op = ControlOptions::EPOLL_CTL_DEL as i32;

                let ret = unsafe { libc::epoll_ctl(self.api.fd, op, fd, event) };
                if ret < 0 {
                    Err(ret.into())
                } else {
                    Ok(o.remove())
                }
            }
            Entry::Vacant(_) => Err(Error::FdNotFound { fd }),
        }
    }

    /// Safe wrapper for `libc::close`
    /// this will return the datas
    /// For Event<DataPtr<T>> only if you want to free ressource
    /// you will need to call `Event<DataPtr<T>>::into_inner()`
    /// This could be improve if we could specialize Drop
    /// https://github.com/rust-lang/rust/issues/46893
    #[instrument(skip(self), level = "trace")]
    pub fn close(self) -> (Result<(), Error>, HashMap<RawFd, Data<T>>) {
        info!(self.api.fd);

        let ret = unsafe { libc::close(self.as_raw_fd()) };
        let result = if ret < 0 { Err(ret.into()) } else { Ok(()) };

        (result, self.api.datas)
    }

    /// Safe wrapper for `libc::epoll_wait`
    /// The time_out argument is in milliseconds
    ///
    /// ## Notes
    ///
    /// * If `time_out` is negative, it will block until an event is received.
    #[instrument(skip(self, time_out), level = "trace")]
    pub fn wait<N: Into<TimeOut>>(
        &mut self,
        time_out: N,
    ) -> Result<Wait<T>, Error>
    where
        N: Into<TimeOut>,
    {
        let time_out = time_out.into();
        info!(self.api.fd, ?time_out);
        let time_out = time_out.into();

        unsafe {
            let ret = libc::epoll_wait(
                self.as_raw_fd(),
                self.buffer.as_mut_ptr() as *mut libc::epoll_event,
                self.buffer.capacity() as libc::c_int,
                time_out,
            );

            if ret < 0 {
                let e = ret.into();

                Err(e)
            } else {
                let num_events = ret as usize;
                self.buffer.set_len(num_events);

                // https://doc.rust-lang.org/std/mem/union.MaybeUninit.html#method.slice_assume_init_ref
                let buffer = &mut *(self.buffer.as_mut_slice() as *mut _ as *mut [Event<T>]);

                let wait = Wait::new(&mut self.api, buffer);
                Ok(wait)
            }
        }
    }

    /// This resize the buffer used to recieve event
    #[instrument(skip(self, max_events), level = "trace")]
    pub fn resize_buffer<N: Into<MaxEvents>>(
        &mut self,
        max_events: N,
    ) {
        let max_events = max_events.into();
        info!(self.api.fd, ?max_events);
        let max_events = max_events.into();

        self.buffer.resize_with(max_events, MaybeUninit::uninit);
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
    #[ignore = "Still ignored taupaulin doesn't like it too"]
    fn create_with_max() {
        use nix::{
            sys::{
                signal::Signal,
                wait::{waitpid, WaitStatus},
            },
            unistd::{fork, ForkResult},
        };
        use std::panic;
        use std::process::abort;

        match unsafe { fork() } {
            Ok(ForkResult::Parent { child }) => match waitpid(child, None) {
                Ok(WaitStatus::Signaled(_, s, _)) => {
                    if s != Signal::SIGABRT {
                        panic!("Didn't abort")
                    }
                }
                o => panic!("Didn't expect: {:?}", o),
            },
            Ok(ForkResult::Child) => {
                let result = panic::catch_unwind(|| {
                    create::<DataU32>(false, usize::MAX);
                });

                if let Err(_) = result {
                    abort();
                }
            }
            Err(_) => panic!("Fork failed"),
        }
    }
}
