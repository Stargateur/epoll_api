use epoll_api::{Data, EPoll, EPollApi, Event, Flags, MaxEvents, TimeOut};
use libc::EINVAL;

use std::{
    collections::VecDeque,
    io::{BufWriter, ErrorKind},
    net::{Ipv6Addr, TcpListener, TcpStream},
    os::unix::io::AsRawFd,
};

enum Kind {
    Server(TcpListener),
    Client(Client),
}

struct Client {
    stream: TcpStream,
    buf_write: Vec<u8>,
}

fn main() {
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
            match event.data().ptr() {
                Kind::Server(listener) => {
                    if event.flags().contains(Flags::EPOLLIN) {
                        loop {
                            match listener.accept() {
                                Ok((stream, addr)) => {
                                    println!("New client: {}", addr);

                                    let fd = stream.as_raw_fd();
                                    let event = Event::new(
                                        Flags::EPOLLIN | Flags::EPOLLET,
                                        Data::new_ptr(Kind::Client(Client {
                                            stream,
                                            buf_write: Vec::new(),
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
                Kind::Client(client) => {}
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
    // for stream in listener.incoming() {
    //     match stream {
    //         Ok(stream) => {
    //             println!("new client!");
    //         }
    //         Err(e) => { /* connection failed */ }
    //     }
    // }
}

//     for (n = 0; n < nfds; ++n) {
//         if (events[n].data.fd == listen_sock) {
//             conn_sock = accept(listen_sock,
//                                (struct sockaddr *) &addr, &addrlen);
//             if (conn_sock == -1) {
//                 perror("accept");
//                 exit(EXIT_FAILURE);
//             }
//             setnonblocking(conn_sock);
//             ev.events = EPOLLIN | EPOLLET;
//             ev.data.fd = conn_sock;
//             if (epoll_ctl(epollfd, EPOLL_CTL_ADD, conn_sock,
//                         &ev) == -1) {
//                 perror("epoll_ctl: conn_sock");
//                 exit(EXIT_FAILURE);
//             }
//         } else {
//             do_use_fd(events[n].data.fd);
//         }
//     }
// }
