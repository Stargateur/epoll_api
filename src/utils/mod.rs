mod read_size;

use std::{
    io::{self, ErrorKind, Read},
    os::unix::io::AsRawFd,
};

pub use read_size::ReadSize;

#[cfg(feature = "tracing")]
use tracing::trace_span;

#[must_use]
pub enum State {
    WouldBlock(usize),
    EndOfFile(usize),
    Error(io::Error),
}

/// This function assume the Read implementation don't do anything stupid sue me

pub fn read_until_wouldblock<R, S>(
    mut reader: R,
    output: &mut Vec<u8>,
    read_size: S,
) -> State
where
    R: Read,
    S: Into<ReadSize>,
{
    let read_size: ReadSize = read_size.into();

    #[cfg(feature = "tracing")]
    let _span = trace_span!("read_until_wouldblock", ?read_size).entered();

    let read_size = read_size.into();

    let mut total = 0;
    let ret = loop {
        let available = output.capacity() - output.len();
        if available < read_size {
            output.reserve(read_size - available);
        }
        let buffer = unsafe {
            std::slice::from_raw_parts_mut(output.as_mut_ptr().add(output.len()), read_size)
        };
        match reader.read(buffer) {
            Ok(n) => {
                if n == 0 {
                    break State::EndOfFile(total);
                }
                total += n;

                unsafe { output.set_len(output.len() + n) }
            }
            Err(e) => {
                break if e.kind() == ErrorKind::WouldBlock {
                    State::WouldBlock(total)
                } else {
                    State::Error(e)
                };
            }
        }
    };

    ret
}

pub fn set_non_blocking<DataFd: AsRawFd>(fd: DataFd) -> io::Result<()> {
    let fd = fd.as_raw_fd();

    #[cfg(feature = "tracing")]
    let _span = trace_span!("set_non_blocking", fd).entered();

    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL);
        if flags == -1 {
            Err(io::Error::last_os_error())
        } else if flags & libc::O_NONBLOCK != libc::O_NONBLOCK {
            if libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) == -1 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        } else {
            Ok(())
        }
    }
}
