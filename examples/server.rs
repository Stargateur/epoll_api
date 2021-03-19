use epoll_api::{Data, EPoll, EPollApi, Event, Flags, MaxEvents, TimeOut};

use std::{
    collections::VecDeque,
    io::{self, ErrorKind, Read, Write},
    net::{Ipv6Addr, TcpListener, TcpStream},
    os::unix::io::AsRawFd,
};

enum Kind {
    Server(TcpListener),
    Client(Client),
}

struct Client {
    stream: TcpStream,
    buffer: Vec<u8>,
}

impl Client {
    fn write_buffer(&mut self) -> io::Result<usize> {
        log::trace!("write");
        let n = self.stream.write(&self.buffer)?;
        log::trace!("write");
        self.buffer.drain(..n);
        Ok(n)
    }

    fn read_buffer(&mut self) -> io::Result<()> {
        log::trace!("start read");
        let mut buffer = [0; 1024];
        loop {
            match self.stream.read(&mut buffer) {
                Ok(n) => {
                    if n == 0 {
                        return Err(ErrorKind::ConnectionAborted.into());
                    }

                    self.buffer.copy_from_slice(&buffer[..n]);
                }
                Err(e) => {
                    if e.kind() == ErrorKind::WouldBlock {
                        break;
                    } else {
                        return Err(e);
                    }
                }
            }
        }
        log::trace!("end read");

        Ok(())
    }
}

fn main() {
    pretty_env_logger::init();

    let max_events = MaxEvents::new(42).unwrap();
    let mut epoll = EPoll::new(true, max_events).unwrap();

    let listener = TcpListener::bind((Ipv6Addr::UNSPECIFIED, 0)).unwrap();
    listener.set_nonblocking(true).unwrap();

    let local_addr = listener.local_addr().unwrap();
    println!("Server listen on {}", local_addr);

    {
        let fd = listener.as_raw_fd();
        let event = Event::new(
            Flags::EPOLLIN | Flags::EPOLLET,
            Data::new_ptr(Kind::Server(listener)),
        );

        epoll.add(fd, event).unwrap();
    }

    let mut dels = VecDeque::new();

    'run: loop {
        let wait = epoll.wait(TimeOut::INFINITE).unwrap();
        for event in wait.events {
            let flags = event.flags();
            match event.data_mut().ptr_mut() {
                Kind::Server(listener) => {
                    if flags.contains(Flags::EPOLLIN) {
                        loop {
                            match listener.accept() {
                                Ok((stream, addr)) => {
                                    println!("New client: {}", addr);

                                    let fd = stream.as_raw_fd();
                                    let event = Event::new(
                                        Flags::EPOLLIN | Flags::EPOLLOUT | Flags::EPOLLET,
                                        Data::new_ptr(Kind::Client(Client {
                                            stream,
                                            buffer: Default::default(),
                                        })),
                                    );

                                    wait.api.add(fd, event).unwrap();
                                }
                                Err(e) => {
                                    if e.kind() != ErrorKind::WouldBlock {
                                        dels.push_back(listener.as_raw_fd());
                                    }
                                    break;
                                }
                            }
                        }
                    }
                }
                Kind::Client(client) => {
                    if flags.contains(Flags::EPOLLIN) {
                        match client.read_buffer() {
                            Ok(_) => {
                                if let Err(e) = client.write_buffer() {
                                    eprint!("{}", e);
                                    dels.push_back(client.stream.as_raw_fd());
                                }
                            }
                            Err(e) => {
                                if e.kind() != ErrorKind::WouldBlock {
                                    dels.push_back(client.stream.as_raw_fd());
                                }
                            }
                        }
                    }
                    if flags.contains(Flags::EPOLLOUT) {
                        if let Err(e) = client.write_buffer() {
                            eprint!("{}", e);
                            dels.push_back(client.stream.as_raw_fd());
                        }
                    }
                }
            }
        }

        while let Some(x) = dels.pop_front() {
            let data = epoll.del(x).unwrap().into_inner();

            match *data {
                Kind::Server(_) => {
                    break 'run;
                }
                Kind::Client(client) => {
                    println!("Bye: {}", client.stream.local_addr().unwrap());
                }
            }
        }
    }

    epoll.drop();
}
