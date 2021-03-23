use epoll_api::{
    data_kind::Data,
    utils::{read_until_wouldblock, set_non_blocking, State},
    EPoll, EPollApi, Event, Flags, TimeOut,
};

use std::{
    io::{self, ErrorKind, Write},
    net::{Ipv6Addr, TcpStream},
    os::unix::io::AsRawFd,
};

use tracing_subscriber::{filter::LevelFilter, fmt::format::FmtSpan};

use tracing::{error, info, instrument};

enum Kind {
    Server(Server),
    Stdin(io::Stdin),
}

struct Server {
    stream: TcpStream,
    buf_read: Vec<u8>,
    buf_write: Vec<u8>,
    flags: Flags,
}

impl Server {
    #[instrument(skip(self), level = "trace")]
    fn write_buffer(&mut self) -> State {
        let mut total = 0;
        while !self.buf_write.is_empty() {
            match self.stream.write(&self.buf_write) {
                Ok(octet_write) => {
                    info!(octet_write);
                    self.buf_write.drain(..octet_write);
                    total += octet_write;
                }
                Err(e) => {
                    if e.kind() == ErrorKind::WouldBlock {
                        return State::WouldBlock(total);
                    } else {
                        return State::Error(e);
                    }
                }
            }
        }

        State::EndOfFile(total)
    }

    fn use_buffer(&mut self) {
        let valid = match std::str::from_utf8(&self.buf_read) {
            Ok(valid) => valid,
            Err(error) => {
                let (valid, after_valid) = self.buf_read.split_at(error.valid_up_to());

                if after_valid.len() > 42 {
                    panic!("server lie to us we don't want to blow our memory");
                }

                unsafe { std::str::from_utf8_unchecked(valid) }
            }
        };

        print!("{}", valid);

        let to_drain = ..valid.len();
        self.buf_read.drain(to_drain);
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(LevelFilter::TRACE)
        .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
        .init();

    let args: Vec<_> = std::env::args().collect();

    let mut epoll = EPoll::new(true, 42).unwrap();
    let port = args[1].parse::<u16>().unwrap();

    let stream = TcpStream::connect((Ipv6Addr::UNSPECIFIED, port)).unwrap();
    stream.set_nonblocking(true).unwrap();

    let local_addr = stream.local_addr().unwrap();
    println!("Connect to {}", local_addr);

    let fd = stream.as_raw_fd();
    let flags = Flags::EPOLLIN | Flags::EPOLLET;
    let event = Event::new(
        flags,
        Data::new_box(Kind::Server(Server {
            stream,
            buf_write: Default::default(),
            buf_read: Default::default(),
            flags,
        })),
    );

    epoll.add(fd, event).unwrap();

    {
        let stdin = io::stdin();
        let fd = stdin.as_raw_fd();
        set_non_blocking(fd).unwrap();
        let event = Event::new(
            Flags::EPOLLIN | Flags::EPOLLET,
            Data::new_box(Kind::Stdin(stdin)),
        );
        epoll.add(fd, event).unwrap();
    }

    'run: loop {
        let wait = epoll.wait(TimeOut::INFINITE).unwrap();
        for event in wait.events {
            let flags = event.flags();
            let kind = event.data_mut().as_mut();

            match kind {
                Kind::Server(server) => {
                    if flags.contains(Flags::EPOLLIN) {
                        match read_until_wouldblock(&server.stream, &mut server.buf_read, 4096) {
                            State::EndOfFile(_) => {
                                server.use_buffer();
                                info!("Server disconnect");
                                break 'run;
                            }
                            State::WouldBlock(_) => {
                                server.use_buffer();
                            }
                            State::Error(e) => {
                                error!("{}", e);
                                break 'run;
                            }
                        }
                    }
                    if flags.contains(Flags::EPOLLOUT) {
                        match server.write_buffer() {
                            State::WouldBlock(_) => {
                                if !server.flags.contains(Flags::EPOLLOUT) {
                                    info!("Register for write");
                                    let flags = server.flags | Flags::EPOLLOUT;
                                    wait.api
                                        .mod_flags(server.stream.as_raw_fd(), flags)
                                        .unwrap();
                                    server.flags = flags;
                                }
                            }
                            State::EndOfFile(_) => {
                                if server.flags.contains(Flags::EPOLLOUT) {
                                    info!("Unregister for write");
                                    let flags = server.flags ^ Flags::EPOLLOUT;
                                    wait.api
                                        .mod_flags(server.stream.as_raw_fd(), flags)
                                        .unwrap();
                                    server.flags = flags;
                                }
                            }
                            State::Error(e) => {
                                error!("{}", e);
                                break 'run;
                            }
                        }
                    }
                }
                Kind::Stdin(stdin) => {
                    if flags.contains(Flags::EPOLLIN) {
                        let server = wait.api.get_data_mut(fd).unwrap().as_mut();
                        let server = match server {
                            Kind::Server(server) => server,
                            Kind::Stdin(_) => unreachable!(),
                        };

                        match read_until_wouldblock(stdin, &mut server.buf_write, 4096) {
                            State::EndOfFile(_) => match server.write_buffer() {
                                State::WouldBlock(_) => {
                                    if !server.flags.contains(Flags::EPOLLOUT) {
                                        info!("Register for write");
                                        let fd = server.stream.as_raw_fd();
                                        server.flags = server.flags | Flags::EPOLLOUT;
                                        let flags = server.flags;
                                        wait.api.mod_flags(fd, flags).unwrap();
                                    }
                                    break 'run;
                                }
                                State::EndOfFile(_) => {
                                    break 'run;
                                }
                                State::Error(e) => {
                                    error!("{}", e);
                                    break 'run;
                                }
                            },
                            State::WouldBlock(_) => match server.write_buffer() {
                                State::WouldBlock(_) => {
                                    if !server.flags.contains(Flags::EPOLLOUT) {
                                        info!("Register for write");
                                        let fd = server.stream.as_raw_fd();
                                        server.flags = server.flags | Flags::EPOLLOUT;
                                        let flags = server.flags;
                                        wait.api.mod_flags(fd, flags).unwrap();
                                    }
                                }
                                State::EndOfFile(_) => {}
                                State::Error(e) => {
                                    error!("{}", e);
                                    break 'run;
                                }
                            },
                            State::Error(e) => {
                                error!("{}", e);
                                if e.kind() != ErrorKind::WouldBlock {
                                    break 'run;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    epoll.drop();
}
