use epoll_api::{
    data_kind::{Data, DataBox},
    utils::{read_until_wouldblock, State},
    Api, EPoll, EPollApi, Event, Flags, TimeOut,
};

use std::{
    collections::VecDeque,
    io::{self, ErrorKind, Write},
    net::{Ipv6Addr, TcpListener, TcpStream},
    os::unix::io::AsRawFd,
};

use tracing_subscriber::{filter::LevelFilter, fmt::format::FmtSpan};

use tracing::{error, info, instrument};

enum Kind {
    Server(TcpListener),
    Client(Client),
}

struct Client {
    stream: TcpStream,
    buffer: Vec<u8>,
    flags: Flags,
}

impl Client {
    #[instrument(skip(self, api), level = "trace")]
    fn write_buffer(
        &mut self,
        api: &mut Api<DataBox<Kind>>,
    ) -> io::Result<()> {
        while !self.buffer.is_empty() {
            let n = match self.stream.write(&self.buffer) {
                Ok(n) => n,
                Err(e) => {
                    if e.kind() == ErrorKind::WouldBlock {
                        if !self.flags.contains(Flags::EPOLLOUT) {
                            info!("Register for write");
                            let flags = self.flags | Flags::EPOLLOUT;
                            api.mod_flags(self.stream.as_raw_fd(), flags)?;
                            self.flags = flags;
                        }
                        return Ok(());
                    } else {
                        return Err(e);
                    }
                }
            };
            info!("writen: {}", n);
            self.buffer.drain(..n);
        }

        if self.flags.contains(Flags::EPOLLOUT) {
            info!("Unregister for write");
            let flags = self.flags ^ Flags::EPOLLOUT;
            api.mod_flags(self.stream.as_raw_fd(), flags)?;
            self.flags = flags;
        }

        Ok(())
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(LevelFilter::TRACE)
        .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
        .init();

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
                                    let flags = Flags::EPOLLIN | Flags::EPOLLET;
                                    let event = Event::new(
                                        flags,
                                        Data::new_box(Kind::Client(Client {
                                            stream,
                                            buffer: Default::default(),
                                            flags,
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
                            State::WouldBlock(_) => {
                                if let Err(e) = client.write_buffer(wait.api) {
                                    eprint!("{}", e);
                                    dels.push_back(client.stream.as_raw_fd());
                                }
                            }
                            State::EndOfFile(_) => {
                                if let Err(e) = client.write_buffer(wait.api) {
                                    eprint!("{}", e);
                                }
                                dels.push_back(client.stream.as_raw_fd());
                            }
                            State::Error(e) => {
                                error!("{}", e);
                                dels.push_back(client.stream.as_raw_fd());
                            }
                        }
                    }
                    if flags.contains(Flags::EPOLLOUT) {
                        if let Err(e) = client.write_buffer(wait.api) {
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
