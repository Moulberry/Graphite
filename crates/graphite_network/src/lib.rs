use std::cell::RefCell;
use std::collections::HashMap;
use std::error::Error;
use std::io::{self, Read};
use std::net::ToSocketAddrs;
use std::ops::DerefMut;
use std::rc::Rc;

use mio::event::Event;
use mio::net::{TcpListener, TcpStream};
use mio::{Poll, Events, Token, Interest, Registry};

mod write_buffer;
pub use write_buffer::WriteBuffer;

const SERVER: Token = Token(0);
const BUFFER_SIZE: usize = 2097148;

pub trait FramedPacketHandler<N: NetworkHandlerService> {
    fn handle(&mut self, service: &mut N, data: &[u8]);
    fn disconnected(&mut self, service: &mut N);
}

pub struct Connection<N: NetworkHandlerService> {
    sent_to_handler: bool,
    stream: TcpStream,
    read_buffer: Vec<u8>,
    handler: Option<Rc<RefCell<dyn FramedPacketHandler<N>>>>
}

impl <N: NetworkHandlerService> Connection<N> {
    pub fn set_handler<T: FramedPacketHandler<N> + 'static>(&mut self, handler: Rc<RefCell<T>>) {
        if self.handler.is_some() {
            panic!("Handler set twice!");
        }

        let t = handler.clone() as Rc<RefCell<dyn FramedPacketHandler<N>>>;
        self.handler = Some(t);
    }
}

pub trait NetworkHandlerService: Sized {
    fn accept_new_connection(&mut self, connection: Rc<RefCell<Connection<Self>>>);
    // game loop
}

pub struct NetworkHandler<T: NetworkHandlerService> {
    service: T
}

impl <T: NetworkHandlerService> NetworkHandler<T> {
    pub fn new(service: T) -> NetworkHandler<T> {
        Self {
            service
        }
    }

    pub fn listen(&mut self, addr: impl ToSocketAddrs) -> Result<(), Box<dyn Error>> {
        // Create a poll instance.
        let mut poll = Poll::new()?;
        // Create storage for events.
        let mut events = Events::with_capacity(128);
    
        // Setup the server socket.
        let mut server = TcpListener::bind(addr.to_socket_addrs().unwrap().next().unwrap())?;
        // Start listening for incoming connections.
        poll.registry()
            .register(&mut server, SERVER, Interest::READABLE)?;
    
        // todo: use slab instead of hashmap
        let mut connections = HashMap::new();
        let mut unique_token = Token(SERVER.0 + 1);
    
        loop {
            // Poll Mio for events, blocking until we get an event.
            if let Err(err) = poll.poll(&mut events, None) {
                if interrupted(&err) {
                    continue;
                }
                return Err(err.into());
            }
    
            for event in events.iter() {
                match event.token() {
                    SERVER => loop {
                        let (mut stream, address) = match server.accept() {
                            Ok((stream, address)) => (stream, address),
                            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                                // Break out of accept loop, continue processing events
                                break;
                            }
                            Err(e) => {
                                return Err(e.into());
                            }
                        };
    
                        println!("Accepted connection from: {}", address);
    
                        let token_id = unique_token.0;
                        unique_token.0 += 1;
                        let token = Token(token_id);
    
                        poll.registry().register(
                            &mut stream,
                            token,
                            Interest::READABLE.add(Interest::WRITABLE),
                        )?;
    
                        let connection = Rc::new(RefCell::new(Connection {
                            sent_to_handler: false,
                            stream,
                            read_buffer: Vec::new(),
                            handler: None
                        }));
    
                        connections.insert(token, connection);
                    }
                    token => {
                        // Maybe received an event for a TCP connection.
                        let disconnect = if let Some(connection) = connections.get_mut(&token) {
                            self.handle_connection_event(poll.registry(), connection, event)?
                        } else {
                            // Sporadic events happen, we can safely ignore them.
                            false
                        };
                        if disconnect {
                            if let Some(connection) = connections.remove(&token) {
                                let mut connection_ref = connection.borrow_mut();
                                poll.registry().deregister(&mut connection_ref.stream)?;
                                if let Some(handler) = connection_ref.handler.take() {
                                    handler.borrow_mut().disconnected(&mut self.service);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    fn handle_connection_event(
        &mut self,
        registry: &Registry,
        connection: &mut Rc<RefCell<Connection<T>>>,
        event: &Event,
    ) -> io::Result<bool> {
        if event.is_writable() {
            let mut connection_ref = connection.borrow_mut();

            if !connection_ref.sent_to_handler {
                connection_ref.sent_to_handler = true;

                registry.reregister(&mut connection_ref.stream, event.token(), Interest::READABLE)?;

                drop(connection_ref);
                self.service.accept_new_connection(Rc::clone(connection));
                connection_ref = connection.borrow_mut();

                // No handler? Disconnect
                // if connection_ref.handler.is_none() {
                //     return Ok(true);
                // }
            } else {
                registry.reregister(&mut connection_ref.stream, event.token(), Interest::READABLE)?;
            }
        }

        // if event.is_writable() {
        //     // We can (maybe) write to the connection.
        //     match connection.write(DATA) {
        //         // We want to write the entire `DATA` buffer in a single go. If we
        //         // write less we'll return a short write error (same as
        //         // `io::Write::write_all` does).
        //         Ok(n) if n < DATA.len() => return Err(io::ErrorKind::WriteZero.into()),
        //         Ok(_) => {
        //             // After we've written something we'll reregister the connection
        //             // to only respond to readable events.
                    
        //         }
        //         // Would block "errors" are the OS's way of saying that the
        //         // connection is not actually ready to perform this I/O operation.
        //         Err(ref err) if would_block(err) => {}
        //         // Got interrupted (how rude!), we'll try again.
        //         Err(ref err) if interrupted(err) => {
        //             return handle_connection_event(registry, connection, event)
        //         }
        //         // Other errors we'll consider fatal.
        //         Err(err) => return Err(err),
        //     }
        // }
    
        if event.is_readable() {
            let mut connection_ref = connection.borrow_mut();
            let connection_ref = connection_ref.deref_mut();
            
            let mut connection_closed = false;

            // We can (maybe) read from the connection.
            loop {
                // todo: cap at maximum packet size
                let read_buffer = &mut connection_ref.read_buffer;
                let stream = &mut connection_ref.stream;

                read_buffer.reserve(65536);

                let read_into = unsafe {
                    let ptr = read_buffer.as_mut_ptr().add(read_buffer.len());
                    std::slice::from_raw_parts_mut(ptr, read_buffer.capacity() - read_buffer.len())
                };

                match stream.read(read_into) {
                    Ok(0) => {
                        // Reading 0 bytes means the other side has closed the
                        // connection or is done writing, then so are we.
                        connection_closed = true;
                        break;
                    }
                    Ok(n) => {
                        unsafe { read_buffer.set_len(read_buffer.len() + n); }
                    }
                    // Would block "errors" are the OS's way of saying that the
                    // connection is not actually ready to perform this I/O operation.
                    Err(ref err) if would_block(err) => break,
                    Err(ref err) if interrupted(err) => continue,
                    // Other errors we'll consider fatal.
                    Err(err) => return Err(err),
                }
            }
    
            if !connection_ref.read_buffer.is_empty() {
                if let Some(handler) = &mut connection_ref.handler {
                    handler.borrow_mut().handle(&mut self.service, &connection_ref.read_buffer);
                }
            }
    
            if connection_closed {
                println!("Connection closed");
                return Ok(true);
            }
        }
    
        Ok(false)
    }
}

fn would_block(err: &io::Error) -> bool {
    err.kind() == io::ErrorKind::WouldBlock
}

fn interrupted(err: &io::Error) -> bool {
    err.kind() == io::ErrorKind::Interrupted
}