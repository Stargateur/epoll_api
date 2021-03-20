use std::{
    io::{self, ErrorKind, Read},
    os::unix::io::AsRawFd,
};

#[must_use]
pub enum State {
    WouldBlock(usize),
    EOF(usize),
    Error(io::Error),
}

/// This function assume the Read implementation don't do anything stupid sue me
pub fn read_until_wouldblock<R: Read>(
    mut reader: R,
    output: &mut Vec<u8>,
    read_size: usize,
) -> State {
    log::trace!("=> read_until_wouldblock");
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
                    break State::EOF(total);
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
    log::trace!("<= read_until_wouldblock");

    ret
}

pub fn set_non_blocking<DataFd: AsRawFd>(fd: DataFd) -> io::Result<()> {
    let fd = fd.as_raw_fd();
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
