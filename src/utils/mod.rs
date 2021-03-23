mod read_size;

use std::{
    io::{self, ErrorKind, Read},
    os::unix::io::AsRawFd,
};

pub use read_size::ReadSize;

use tracing::{debug, info, instrument};

#[must_use]
pub enum State {
    WouldBlock(usize),
    EndOfFile(usize),
    Error(io::Error),
}

/// This function assume the Read implementation don't do anything stupid sue me
#[instrument(skip(reader, output, read_size), level = "trace")]
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
    info!(?read_size);
    let read_size = read_size.into();

    let mut total = 0;
    let ret = loop {
        let available = output.capacity();
        debug!(available);
        if available < read_size {
            let to_reserve = read_size - available;
            debug!(to_reserve);
            output.reserve(read_size - available);
        }
        let buffer = unsafe {
            std::slice::from_raw_parts_mut(output.as_mut_ptr().add(output.len()), read_size)
        };
        debug!(buffer = ?buffer.as_mut_ptr(), ptr = ?output.as_mut_ptr(), len = output.len(), cap = output.capacity(), read_size);

        match reader.read(buffer) {
            Ok(octet_read) => {
                if octet_read == 0 {
                    break State::EndOfFile(total);
                }
                info!(octet_read);
                total += octet_read;

                unsafe { output.set_len(output.len() + octet_read) }
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

#[instrument(skip(fd), level = "trace")]
pub fn set_non_blocking<Fd: AsRawFd>(fd: Fd) -> io::Result<()> {
    let fd = fd.as_raw_fd();
    info!(fd);

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
