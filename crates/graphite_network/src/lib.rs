use std::any::{TypeId, Any};
use std::cell::{UnsafeCell, RefCell};
use std::collections::HashMap;
use std::error::Error;
use std::io::{self, Read, Write};
use std::net::{ToSocketAddrs, SocketAddr};
use std::rc::Rc;
use std::sync::Arc;

use graphite_binary::varint;
use mio::event::Event;
use mio::net::{TcpListener, TcpStream};
use mio::{Poll, Events, Token, Interest};

mod packet_buffer;
pub use packet_buffer::PacketBuffer;

const NEW_CONNECTION_TOKEN: Token = Token(0);

pub enum HandleAction {
    Continue,
    Disconnect,
    Transfer(Box<dyn FnOnce(TcpStream)>)
}

pub trait FramedPacketHandler {
    fn handle(&mut self, data: &[u8]) -> HandleAction;
    fn disconnected(&mut self);
}

pub struct Connection {
    shutdown: bool,

    stream: TcpStream,
    unfinished_buffer: Vec<u8>,
    handler: Option<Rc<UnsafeCell<dyn FramedPacketHandler>>>
}

impl Connection {
    pub fn set_handler<T: FramedPacketHandler + 'static>(&mut self, handler: Rc<UnsafeCell<T>>) {
        if self.shutdown {
            return;
        }

        if self.handler.is_some() {
            panic!("Handler set twice!");
        }

        let t = handler.clone() as Rc<UnsafeCell<dyn FramedPacketHandler>>;
        self.handler = Some(t);
    }

    pub fn send(&mut self, mut bytes: &[u8]) {
        if self.shutdown {
            return;
        }

        loop {
            match self.stream.write(bytes) {
                // Partial write
                Ok(n) if n < bytes.len() => {
                    bytes = &bytes[n..];
                    continue;
                }
                // Success
                Ok(_) => {
                    break;
                }
                // WouldBlock or Interrupted... try again
                Err(ref err) if would_block(err) || interrupted(err) => {
                    continue;
                }
                // Other errors we'll consider fatal.
                Err(err) => {
                    panic!("err: {}", err); // todo: disconnect instead of panicing
                    // return Err(err)
                },
            }
        }
    }

    pub fn is_shutdown(&self) -> bool {
        self.shutdown
    }

    pub fn disconnect(&mut self) {
        if !self.shutdown {
            self.shutdown = true;
            let _ = self.stream.shutdown(std::net::Shutdown::Both);
        }
    }
}

pub trait NetworkHandlerService: Sized {
    const MAXIMUM_PACKET_SIZE: usize;
    type ExtraData: 'static + From<SocketAddr>;

    fn accept_new_connection(&mut self, extra_data: Self::ExtraData, connection: Rc<RefCell<Connection>>);
    // game loop
}

#[derive(Clone)]
pub struct TcpStreamSender<E> {
    inner: std::sync::mpsc::Sender<(TcpStream, E)>,
    waker: Arc<mio::Waker>
}

impl <E> TcpStreamSender<E> {
    pub fn send(&mut self, stream: TcpStream, extra_data: E) {
        self.inner.send((stream, extra_data)).unwrap();
        self.waker.wake().unwrap();
    }
}

pub enum ConnectionReceiver<E> {
    TcpListener(TcpListener),
    Channel(std::sync::mpsc::Receiver<(TcpStream, E)>)
}

pub struct NetworkHandler<T: NetworkHandlerService> {
    service: T,
    poll: Poll,
    receiver: ConnectionReceiver<T::ExtraData>,
    connections: HashMap<Token, (Option<T::ExtraData>, Rc<RefCell<Connection>>)>,

    read_buffer: Box<[u8]>,
    bytes_read: usize
}

impl <T: NetworkHandlerService<ExtraData = SocketAddr>> NetworkHandler<T> {
    pub fn new(service: T, addr: impl ToSocketAddrs) -> Result<NetworkHandler<T>, Box<dyn Error>> {
        let poll = Poll::new()?;

        let mut server = TcpListener::bind(addr.to_socket_addrs().unwrap().next().unwrap())?;

        poll.registry()
            .register(&mut server, NEW_CONNECTION_TOKEN, Interest::READABLE)?;

        let network_handler = Self {
            service,
            poll,
            receiver: ConnectionReceiver::TcpListener(server),
            connections: HashMap::new(),

            read_buffer: vec![0_u8; 4_194_304].into_boxed_slice(),
            bytes_read: 0
        };

        Ok(network_handler)
    }
}

impl <T: NetworkHandlerService> NetworkHandler<T> {
    pub fn new_channel(service: T) -> Result<(NetworkHandler<T>, TcpStreamSender<T::ExtraData>), Box<dyn Error>> {
        let poll = Poll::new()?;

        let waker = mio::Waker::new(poll.registry(), NEW_CONNECTION_TOKEN)?;

        let (tx, rx) = std::sync::mpsc::channel();

        let network_handler = Self {
            service,
            poll,
            receiver: ConnectionReceiver::Channel(rx),
            connections: HashMap::new(),

            read_buffer: vec![0_u8; 4_194_304].into_boxed_slice(),
            bytes_read: 0
        };
        let sender = TcpStreamSender {
            inner: tx,
            waker: Arc::new(waker)
        };

        Ok((network_handler, sender))
    }

    pub fn listen(&mut self) -> Result<(), Box<dyn Error>> {
        // Create storage for events.
        let mut events = Events::with_capacity(128);
    
        // todo: use slab instead of hashmap
        let mut unique_token = Token(NEW_CONNECTION_TOKEN.0 + 1);
    
        loop {
            // Poll Mio for events, blocking until we get an event.
            if let Err(err) = self.poll.poll(&mut events, None) {
                if interrupted(&err) {
                    continue;
                }
                return Err(err.into());
            }
    
            for event in events.iter() {
                match event.token() {
                    NEW_CONNECTION_TOKEN => loop {
                        let (mut stream, extra_data) = match &mut self.receiver {
                            ConnectionReceiver::Channel(receiver) => match receiver.try_recv() {
                                Ok((stream, extra_data)) => (stream, extra_data),
                                Err(_) => {
                                    // Break out of accept loop, continue processing events
                                    break;
                                },
                            },
                            ConnectionReceiver::TcpListener(tcp_listener) => match tcp_listener.accept() {
                                Ok((stream, address)) => {
                                    (stream, address.into())
                                },
                                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                                    // Break out of accept loop, continue processing events
                                    break;
                                }
                                Err(e) => {
                                    return Err(e.into());
                                }
                            },
                        };
    
                        let token_id = unique_token.0;
                        unique_token.0 += 1;
                        let token = Token(token_id);
    
                        self.poll.registry().register(
                            &mut stream,
                            token,
                            Interest::READABLE.add(Interest::WRITABLE),
                        )?;
    
                        let connection = Rc::new(RefCell::new(Connection {
                            shutdown: false,

                            stream,
                            unfinished_buffer: Vec::new(),
                            handler: None
                        }));
    
                        self.connections.insert(token, (Some(extra_data), connection));
                    }
                    token => {
                        // Maybe received an event for a TCP connection.
                        let action = self.handle_connection_event(token, event)?;
                        match action {
                            HandleAction::Continue => {},
                            HandleAction::Disconnect => {
                                if let Some(connection) = self.remove_connection(token) {
                                    drop(connection);
                                }
                            },
                            HandleAction::Transfer(callback) => {
                                if let Some(connection) = self.remove_connection(token) {
                                    callback(connection.stream);
                                }
                            },
                        }
                    }
                }
            }
        }
    }

    fn remove_connection(&mut self, token: Token) -> Option<Connection> {
        if let Some((_, connection)) = self.connections.remove(&token) {
            let mut connection_ref = connection.borrow_mut();
            let _ = self.poll.registry().deregister(&mut connection_ref.stream);

            // Fire disconnected
            if let Some(handler) = connection_ref.handler.take() {
                unsafe { handler.get().as_mut().unwrap() }.disconnected();
            }
            drop(connection_ref);

            Some(Rc::into_inner(connection)
                .expect("Connection object wasn't properly dropped - this likely would have led to a memory leak")
                .into_inner())
        } else {
            None
        }
    }
    
    fn handle_connection_event(
        &mut self,
        token: Token,
        event: &Event,
    ) -> io::Result<HandleAction> {
        let (extra_data, connection) = match self.connections.get_mut(&token) {
            Some((extra_data, connection)) => (extra_data, connection),
            None => {
                // Sporadic events happen, we can safely ignore them.
                return Ok(HandleAction::Continue);
            },
        };

        let mut connection_ref = connection.borrow_mut();

        if connection_ref.is_shutdown() {
            return Ok(HandleAction::Disconnect);
        }

        if event.is_writable() {
            connection_ref.stream.set_nodelay(true).unwrap();

            if let Some(extra_data) = extra_data.take() {
                self.poll.registry().reregister(&mut connection_ref.stream, event.token(), Interest::READABLE)?;

                drop(connection_ref);
                self.service.accept_new_connection(extra_data, Rc::clone(connection));
                connection_ref = connection.borrow_mut();

                // No handler? Disconnect
                if connection_ref.handler.is_none() {
                    return Ok(HandleAction::Disconnect);
                }
            } else {
                self.poll.registry().reregister(&mut connection_ref.stream, event.token(), Interest::READABLE)?;
            }
        }
    
        if event.is_readable() {
            let mut connection_closed = false;

            self.bytes_read = connection_ref.unfinished_buffer.len();
            self.read_buffer[0..self.bytes_read].copy_from_slice(&connection_ref.unfinished_buffer);
            connection_ref.unfinished_buffer.clear();

            // We can (maybe) read from the connection.
            loop {
                let stream = &mut connection_ref.stream;

                let read_into = unsafe {
                    let ptr = self.read_buffer.as_mut_ptr().add(self.bytes_read);
                    std::slice::from_raw_parts_mut(ptr, self.read_buffer.len() - self.bytes_read)
                };

                // Filled up the entire buffer, naughty client
                if read_into.len() == 0 {
                    return Ok(HandleAction::Disconnect);
                }

                match stream.read(read_into) {
                    Ok(0) => {
                        // Reading 0 bytes means the other side has closed the
                        // connection or is done writing, then so are we.
                        connection_closed = true;
                        break;
                    }
                    Ok(n) => {
                        self.bytes_read += n;
                    }
                    // Would block "errors" are the OS's way of saying that the
                    // connection is not actually ready to perform this I/O operation.
                    Err(ref err) if would_block(err) => break,
                    Err(ref err) if interrupted(err) => continue,
                    // Other errors we'll consider fatal.
                    Err(err) => return Err(err),
                }
            }
    
            if self.bytes_read > 0 {
                let mut slice = &self.read_buffer[0..self.bytes_read];

                if let Some(handler) = &mut connection_ref.handler {
                    let handler_ref = unsafe { handler.get().as_mut().unwrap() };
                    loop {
                        match try_read_packet(&mut slice, T::MAXIMUM_PACKET_SIZE) {
                            PacketReadResult::PacketTooLarge => {
                                connection_closed = true;
                                break;
                            },
                            PacketReadResult::Complete(packet) => {
                                drop(connection_ref);
                                match handler_ref.handle(packet) {
                                    HandleAction::Continue => {},
                                    HandleAction::Disconnect => {
                                        return Ok(HandleAction::Disconnect);
                                    },
                                    HandleAction::Transfer(callback) => {
                                        return Ok(HandleAction::Transfer(callback));
                                    },
                                }
                                connection_ref = connection.borrow_mut();

                                if connection_ref.is_shutdown() {
                                    return Ok(HandleAction::Disconnect);
                                }
                            },
                            PacketReadResult::Partial => {
                                let slice_len = slice.len();

                                connection_ref.unfinished_buffer.reserve(slice_len);
                                connection_ref.unfinished_buffer[0..slice_len].copy_from_slice(slice);
                                unsafe {
                                    connection_ref.unfinished_buffer.set_len(slice_len);
                                }
                                break;
                            },
                            PacketReadResult::Empty => {
                                break;
                            },
                        }
                        
                    }  
                } else {
                    return Ok(HandleAction::Disconnect);
                }
            }
    
            if connection_closed {
                println!("Connection closed");
                return Ok(HandleAction::Disconnect);
            }
        }
    
        Ok(HandleAction::Continue)
    }
}

pub enum PacketReadResult<'a> {
    PacketTooLarge,
    Complete(&'a [u8]),
    Partial,
    Empty,
}

fn try_read_packet<'a>(slice: &mut &'a [u8], maximum_packet_size: usize) -> PacketReadResult<'a> {
    let remaining = slice.len();

    if remaining == 0 {
        return PacketReadResult::Empty;
    } else if remaining >= 3 {
        // Packet must start with varint header specifying the amount of data
        let (packet_size, varint_header_bytes) = match varint::decode::u21(slice) {
            Ok(decoded) => decoded,
            Err(_) => return PacketReadResult::PacketTooLarge,
        };
        let packet_size = packet_size as usize;

        if packet_size > maximum_packet_size {
            return PacketReadResult::PacketTooLarge;
        }

        let remaining = remaining - varint_header_bytes;
        if remaining >= packet_size {
            // Enough bytes to fully read, consume varint header & emit fully read packet
            let ret = PacketReadResult::Complete(
                &slice[varint_header_bytes..varint_header_bytes + packet_size],
            );

            *slice = &slice[varint_header_bytes + packet_size..];

            return ret;
        }
    } else if remaining == 2 && slice[0] == 1 {
        // Special case for packet of size 1
        // Enough bytes (2) to fully read
        let ret = PacketReadResult::Complete(&slice[1..2]);

        *slice = &slice[2..];

        return ret;
    }

    // Not enough bytes to fully read, emit [varint header + remaining data] as partial read
    PacketReadResult::Partial
}

fn would_block(err: &io::Error) -> bool {
    err.kind() == io::ErrorKind::WouldBlock
}

fn interrupted(err: &io::Error) -> bool {
    err.kind() == io::ErrorKind::Interrupted
}