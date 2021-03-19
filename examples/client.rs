use epoll_api::{Data, EPoll, EPollApi, Event, Flags, MaxEvents, TimeOut};

use std::{
    io::{self, ErrorKind, Read, Write},
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
    fn write_buffer(&mut self) -> io::Result<usize> {
        let n = self.stream.write(&self.buf_write)?;
        self.buf_write.drain(..n);
        Ok(n)
    }

    fn read_buffer(&mut self) -> io::Result<usize> {
        let n = self.stream.read_to_end(&mut self.buf_read)?;

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

        println!("{}", valid);

        let to_drain = ..valid.len();
        self.buf_read.drain(to_drain);

        Ok(n)
    }
}

fn main() {
    let args: Vec<_> = std::env::args().collect();

    let max_events = MaxEvents::new(42).unwrap();
    let mut epoll = EPoll::new(true, max_events).unwrap();
    let port = args[1].parse::<u16>().unwrap();

    let stream = TcpStream::connect((Ipv6Addr::UNSPECIFIED, port)).unwrap();
    stream.set_nonblocking(true).unwrap();

    let local_addr = stream.local_addr().unwrap();
    println!("Connect to {}", local_addr);

    let fd = stream.as_raw_fd();
    let event = Event::new(
        Flags::EPOLLIN | Flags::EPOLLOUT | Flags::EPOLLET,
        Data::new_ptr(Kind::Server(Server {
            stream,
            buf_write: Default::default(),
            buf_read: Default::default(),
        })),
    );

    epoll.add(fd, event).unwrap();

    {
        let stdin = io::stdin();
        let fd = stdin.as_raw_fd();
        let event = Event::new(
            Flags::EPOLLIN | Flags::EPOLLET,
            Data::new_ptr(Kind::Stdin(stdin)),
        );
        epoll.add(fd, event).unwrap();
    }

    'run: loop {
        let wait = epoll.wait(TimeOut::INFINITE).unwrap();
        for event in wait.events {
            let flags = event.flags();
            let kind = event.data_mut().ptr_mut();

            match kind {
                Kind::Server(server) => {
                    if flags.contains(Flags::EPOLLIN) {
                        match server.read_buffer() {
                            Ok(_) => {}
                            Err(e) => {
                                if e.kind() != ErrorKind::WouldBlock {
                                    break 'run;
                                }
                            }
                        }
                    }
                    if flags.contains(Flags::EPOLLOUT) {
                        if let Err(e) = server.write_buffer() {
                            eprint!("{}", e);
                            break 'run;
                        }
                    }
                }
                Kind::Stdin(stdin) => {
                    if flags.contains(Flags::EPOLLIN) {
                        let server = wait.api.get_data_mut(fd).unwrap().ptr_mut();
                        let server = match server {
                            Kind::Server(server) => server,
                            Kind::Stdin(_) => unreachable!(),
                        };

                        match stdin.read_to_end(&mut server.buf_write) {
                            Ok(_) => {}
                            Err(e) => {
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
