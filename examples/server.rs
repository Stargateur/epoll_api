use epoll_api::{
    data_kind::Data, utils::read_until_wouldblock, EPoll, EPollApi, Event, Flags, TimeOut,
};

use std::{
    collections::VecDeque,
    io::{self, ErrorKind, Write},
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
    fn write_buffer(&mut self) -> io::Result<()> {
        log::trace!("=> write");
        if !self.buffer.is_empty() {
            let n = self.stream.write(&self.buffer)?;
            log::trace!("writen: {}", n);
            self.buffer.drain(..n);
        }
        log::trace!("<= write");

        Ok(())
    }
}

fn main() {
    pretty_env_logger::init();

    let mut epoll = EPoll::new(true, 42).unwrap();

    let listener = TcpListener::bind((Ipv6Addr::UNSPECIFIED, 0)).unwrap();
    listener.set_nonblocking(true).unwrap();

    let local_addr = listener.local_addr().unwrap();
    println!("Server listen on {}", local_addr);

    {
        let fd = listener.as_raw_fd();
        let event = Event::new(
            Flags::EPOLLIN | Flags::EPOLLET,
            Data::new_box(Kind::Server(listener)),
        );

        epoll.add(fd, event).unwrap();
    }

    let mut dels = VecDeque::new();

    'run: loop {
        let wait = epoll.wait(TimeOut::INFINITE).unwrap();
        for event in wait.events {
            let flags = event.flags();
            match event.data_mut().as_mut() {
                Kind::Server(listener) => {
                    if flags.contains(Flags::EPOLLIN) {
                        loop {
                            match listener.accept() {
                                Ok((stream, addr)) => {
                                    println!("New client: {}", addr);

                                    let fd = stream.as_raw_fd();
                                    stream.set_nonblocking(true).unwrap();
                                    let event = Event::new(
                                        Flags::EPOLLIN | Flags::EPOLLOUT | Flags::EPOLLET,
                                        Data::new_box(Kind::Client(Client {
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
                        match read_until_wouldblock(&client.stream, &mut client.buffer, 4096) {
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
            let data = epoll.del(x).unwrap().into_box();

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
