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

enum Kind {
    Server(Server),
    Stdin(io::Stdin),
}

struct Server {
    stream: TcpStream,
    buf_read: Vec<u8>,
    buf_write: Vec<u8>,
}

impl Server {
    fn write_buffer(&mut self) -> io::Result<()> {
        log::trace!("=> write");
        if !self.buf_write.is_empty() {
            let n = self.stream.write(&self.buf_write)?;
            log::trace!("writen: {}", n);
            self.buf_write.drain(..n);
        }
        log::trace!("<= write");

        Ok(())
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
    pretty_env_logger::init();

    let args: Vec<_> = std::env::args().collect();

    let mut epoll = EPoll::new(true, 42).unwrap();
    let port = args[1].parse::<u16>().unwrap();

    let stream = TcpStream::connect((Ipv6Addr::UNSPECIFIED, port)).unwrap();
    stream.set_nonblocking(true).unwrap();

    let local_addr = stream.local_addr().unwrap();
    println!("Connect to {}", local_addr);

    let fd = stream.as_raw_fd();
    let event = Event::new(
        Flags::EPOLLIN | Flags::EPOLLOUT | Flags::EPOLLET,
        Data::new_box(Kind::Server(Server {
            stream,
            buf_write: Default::default(),
            buf_read: Default::default(),
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
                            State::EOF(_) => {
                                server.use_buffer();
                                log::info!("Server disconnect");
                                break 'run;
                            }
                            State::WouldBlock(_) => {
                                server.use_buffer();
                            }
                            State::Error(e) => {
                                log::error!("{}", e);
                                break 'run;
                            }
                        }
                    }
                    if flags.contains(Flags::EPOLLOUT) {
                        if let Err(e) = server.write_buffer() {
                            log::error!("{}", e);
                            break 'run;
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
                            State::EOF(_) => {
                                if let Err(e) = server.write_buffer() {
                                    log::error!("{}", e);
                                }
                                // we should wait for server response
                                break 'run;
                            }
                            State::WouldBlock(_) => {
                                if let Err(e) = server.write_buffer() {
                                    log::error!("{}", e);
                                    break 'run;
                                }
                            }
                            State::Error(e) => {
                                log::error!("{}", e);
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
