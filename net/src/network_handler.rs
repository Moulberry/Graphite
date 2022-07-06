use std::collections::VecDeque;
use std::marker::PhantomData;
use std::net::TcpListener;
use std::os::unix::io::{AsRawFd, RawFd};
use std::ptr;

use io_uring::{opcode, squeue, types, IoUring, SubmissionQueue, Submitter};
use slab::Slab;

#[derive(Debug)]
#[repr(u8)]
enum UserData {
    Accept,
    Read {
        connection_index: u16
    },
    TickTimeout,
    Write {
        connection_index: u16,
        write_buffer_index: u16
    },
    #[allow(dead_code)] // Just used to ensure the enum takes up 64 bits
    Unused {
        _1: u8,
        _2: u16,
        _3: u32
    }
}

impl From<u64> for UserData {
    fn from(data: u64) -> Self {
        unsafe { std::mem::transmute(data) }
    }
}

impl Into<u64> for UserData {
    fn into(self) -> u64 {
        unsafe { std::mem::transmute(self) }
    }
}

struct AcceptCount {
    entry: squeue::Entry,
    count: usize,
}

impl AcceptCount {
    fn new(fd: RawFd, count: usize) -> AcceptCount {
        AcceptCount {
            entry: opcode::Accept::new(types::Fd(fd), ptr::null_mut(), ptr::null_mut())
                .build()
                .user_data(UserData::Accept.into()),
            count,
        }
    }

    fn push_to(&mut self, sq: &mut BackloggedSubmissionQueue<'_>) {
        while self.count > 0 {
            unsafe {
                match sq.push_unbuffered(&self.entry) {
                    Ok(_) => self.count -= 1,
                    Err(_) => break,
                }
            }
        }

        sq.sync();
    }
}

pub struct BackloggedSubmissionQueue<'a> {
    ring_squeue: SubmissionQueue<'a>,
    backlog: &'a mut VecDeque<io_uring::squeue::Entry>,
}

impl <'a> BackloggedSubmissionQueue<'a> {
    fn clean_backlog(&mut self, submitter: &Submitter) -> anyhow::Result<()> {
        loop {
            if self.ring_squeue.is_full() {
                println!("sq full!");
                match submitter.submit() {
                    Ok(_) => (),
                    Err(ref err) if err.raw_os_error() == Some(libc::EBUSY) => break,
                    Err(err) => return Err(err.into()),
                }
            }
            self.ring_squeue.sync();

            match self.backlog.pop_front() {
                Some(sqe) => unsafe {
                    println!("had something in backlog!");
                    let _ = self.ring_squeue.push(&sqe);
                },
                None => break,
            }
        }

        Ok(())
    }

    fn sync(&mut self) {
        self.ring_squeue.sync()
    }

    unsafe fn push_unbuffered(&mut self, entry: &io_uring::squeue::Entry) -> Result<(), io_uring::squeue::PushError> {
        self.ring_squeue.push(entry)
    }

    unsafe fn push(&mut self, entry: io_uring::squeue::Entry) {
        if self.ring_squeue.push(&entry).is_err() {
            self.backlog.push_back(entry);
        }
    }
}

struct TheByteSender<'a, 'b> {
    write_buffers: &'a mut Slab<Box<[u8]>>,
    self_connection_index: u16,
    fd: RawFd,
    ring_squeue: &'a mut BackloggedSubmissionQueue<'b>
}

impl <'a, 'b> ByteSender for TheByteSender<'a, 'b> {
    fn send(&mut self, bytes: Box<[u8]>) {
        // get length and pointer for ffi
        println!("trying to write: {}", bytes.len());
        let bytes_len = bytes.len();
        let bytes_pointer = &*bytes as *const _ as *const u8;

        // put bytes into write_buffers, get an index
        let write_index = self.write_buffers.insert(bytes);

        // create user data for write operation, needed in order to drop the write_buffer once finished
        let write_user_data = UserData::Write {
            connection_index: self.self_connection_index,
            write_buffer_index: write_index as u16
        };

        // submit the write operation to io_uring
        let write_e = opcode::Send::new(types::Fd(self.fd), bytes_pointer, bytes_len as u32)
            .build()
            .user_data(write_user_data.into());
        unsafe {
            self.ring_squeue.push(write_e);
        }
    }
}

pub trait ByteSender {
    fn send(&mut self, bytes: Box<[u8]>);
}

pub struct Connection<T: ?Sized, C: ?Sized> {
    _phantom: PhantomData<T>,
    fd: RawFd,
    write_buffers: Slab<Box<[u8]>>,
    pub read_buffer: Vec<u8>,
    pub service: C
}

pub trait ConnectionService<T: ?Sized + NetworkManagerService<Self>> {
    const BUFFER_SIZE: u32 = 4_194_304;

    fn receive(&mut self, service: &mut T, bytes: &[u8], byte_sender: &mut impl ByteSender);
}

pub trait NetworkManagerService<C: ?Sized + ConnectionService<Self>> {
    const SHOULD_TICK: bool;

    fn new_connection_service<'a>(&'a self) -> C;
    fn tick() {}
}

pub struct NetworkManager<T: ?Sized + NetworkManagerService<C>, C: ConnectionService<T>> {
    ring: IoUring,
    backlog: VecDeque<io_uring::squeue::Entry>,
    connection_alloc: Slab<Connection<T, C>>,
    service: T,
}

pub fn start<T: NetworkManagerService<C>, C: ConnectionService<T>>(service: T, addr: &str) -> anyhow::Result<()> {
    let mut network_manager = NetworkManager::new(service)?;

    network_manager.start(addr)?;

    Ok(())
}

impl <T: NetworkManagerService<C>, C: ConnectionService<T>> NetworkManager<T, C> {
    fn new(service: T) -> anyhow::Result<Self> {
        // todo: maybe use IORING_SETUP_SQPOLL?

        let ring = IoUring::new(128)?; // 128? too large? too small?
        let connection_alloc = Slab::with_capacity(64);
        let backlog = VecDeque::new();

        // println!("has fast poll: {}", ring.params().is_feature_fast_poll());

        Ok(Self {
            ring,
            backlog,
            connection_alloc,
            service
        })
    }

    fn start(&mut self, addr: &str) -> anyhow::Result<()> {
        const TICK_TIMESPEC: types::Timespec = types::Timespec::new().nsec(50_000_000);
        

        let listener = TcpListener::bind(addr)?;
        println!("listen {}", listener.local_addr()?);

        let mut accept = AcceptCount::new(listener.as_raw_fd(), 3);

        let (ring_submitter, raw_ring_squeue, mut ring_cqueue) = self.ring.split();
        let mut ring_squeue = BackloggedSubmissionQueue { ring_squeue: raw_ring_squeue, backlog: &mut self.backlog };

        if T::SHOULD_TICK {
            let op = opcode::Timeout::new(&TICK_TIMESPEC)
                    .build()
                    .user_data(UserData::TickTimeout.into());

            unsafe {
                ring_squeue.push(op);
            }
        }

        accept.push_to(&mut ring_squeue);

        loop {
            match ring_submitter.submit_and_wait(1) {
                Ok(_) => (),
                Err(ref err) if err.raw_os_error() == Some(libc::EBUSY) => println!("ebusy"),
                Err(err) => return Err(err.into()),
            }
            ring_cqueue.sync();

            // clean backlog
            ring_squeue.clean_backlog(&ring_submitter)?;

            accept.push_to(&mut ring_squeue);

            for cqe in &mut ring_cqueue {
                let result = cqe.result();
                
                let user_data = UserData::from(cqe.user_data());

                match user_data {
                    UserData::Unused { _1, _2, _3 } => { panic!("unused") }
                    UserData::Write { connection_index, write_buffer_index } => {
                        println!("wrote: {}", result);

                        if let Some(connection) = self.connection_alloc.get_mut(connection_index as usize) {
                            connection.write_buffers.try_remove(write_buffer_index as usize);
                        }
                    }
                    UserData::TickTimeout => {
                        let op = opcode::Timeout::new(&TICK_TIMESPEC)
                                .build()
                                .user_data(UserData::TickTimeout.into());

                        unsafe {
                            ring_squeue.push(op);
                        }

                        T::tick();
                    }
                    UserData::Accept => {
                        println!("accept!");

                        let mut read_buffer = vec!(0_u8; C::BUFFER_SIZE as usize);
                        let read_buffer_ptr = read_buffer.as_mut_ptr();

                        let connection_index = self.connection_alloc.insert(Connection {
                            _phantom: PhantomData,
                            fd: result,
                            read_buffer,
                            write_buffers: Slab::new(),
                            service: self.service.new_connection_service()
                        });

                        NetworkManager::<T, C>::recv(&mut ring_squeue, result, connection_index, read_buffer_ptr, C::BUFFER_SIZE);
                    }
                    UserData::Read { connection_index } => {
                        println!("read bytes: {}", result);

                        let connection_index = connection_index as usize;

                        if result == 0 {
                            // Connection closed

                            if let Some(connection) = self.connection_alloc.try_remove(connection_index) {
                                println!("Connection closed!");
                                unsafe {
                                    libc::close(connection.fd);
                                }
                            }
                        } else if let Some(connection) = self.connection_alloc.get_mut(connection_index) {
                            // Data has been read

                            //let data = &connection.read_buffer[0..result as usize];

                            let mut byte_sender = TheByteSender {
                                write_buffers: &mut connection.write_buffers,
                                self_connection_index: connection_index as u16,
                                fd: connection.fd,
                                ring_squeue: &mut ring_squeue,
                            };

                            connection.service.receive(&mut self.service, &connection.read_buffer[..result as usize], &mut byte_sender);

                            let read_buffer_ptr = connection.read_buffer.as_mut_ptr();

                            NetworkManager::<T, C>::recv(&mut ring_squeue, connection.fd, connection_index, read_buffer_ptr, C::BUFFER_SIZE);
                        }
                    }
                }
            }
        }
    }

    fn recv(squeue: &mut BackloggedSubmissionQueue<'_>, fd: RawFd, connection_index: usize, read_buffer_ptr: *mut u8, buffer_size: u32) {
        let read_e = opcode::Recv::new(types::Fd(fd), read_buffer_ptr, buffer_size)
            .build()
            .user_data(UserData::Read {
                connection_index: connection_index as u16
            }.into());
    
        unsafe {
            squeue.push(read_e);
        }
    }
}

/*if ret < 0 {
    match -ret {
        libc::ETIME => (),
        err => {
            eprintln!(
                "token {:?} error: {:?}",
                token_alloc.get(token_index),
                io::Error::from_raw_os_error(err)
            );
            continue;
        }
    }
}

let token = &mut token_alloc[token_index];
match token.clone() {
    Token::Timeout => {
        println!("tick!");

        /*let op = opcode::Timeout::new(&TIMESPEC)
                .build()
                .user_data(token_index as _);

        unsafe {
            if sq.push(&op).is_err() {
                backlog.push_back(op);
            }
        }     */
    }
    Token::Accept => {
        println!("accept");

        accept.count += 1;

        let fd = ret;
        /*let poll_token = token_alloc.insert(Token::Poll { fd });

        let poll_e = opcode::PollAdd::new(types::Fd(fd), libc::POLLIN as _)
            .build()
            .user_data(poll_token as _);*/

        /*let read_token = token_alloc.insert(Token::Read { fd, buf_index: 0 });

        let read_e = opcode::Read::new(types::Fd(fd), ptr::null_mut(), 0)
            .build()
            .user_data(read_token as _);

        unsafe {
            if sq.push(&read_e).is_err() {
                backlog.push_back(read_e);
            }
        }*/
    }
    /*Token::Poll { fd } => {
        println!("poll!");

        let (buf_index, buf) = match bufpool.pop() {
            Some(buf_index) => (buf_index, &mut buf_alloc[buf_index]),
            None => {
                let buf = vec![0u8; 2048].into_boxed_slice();
                let buf_entry = buf_alloc.vacant_entry();
                let buf_index = buf_entry.key();
                (buf_index, buf_entry.insert(buf))
            }
        };

        *token = Token::Read { fd, buf_index };

        let read_e = opcode::Recv::new(types::Fd(fd), buf.as_mut_ptr(), buf.len() as _)
            .build()
            .user_data(token_index as _);

        unsafe {
            if sq.push(&read_e).is_err() {
                backlog.push_back(read_e);
            }
        }
    }*/
    Token::Read { fd, buf_index } => {
        println!("read: {}", ret);
        /*println!("flags: {}", cqe.flags());
        let buffer_index = io_uring::cqueue::buffer_select(cqe.flags());
        println!("buffer select?: {:?}", buffer_index);
        
        if let Some(buffer_index) = buffer_index {
            let start = unsafe { addr.add(len*buffer_index as usize) };
            println!("read?: {}", unsafe { *start });
        }*/

        /*if ret == 0 {
            bufpool.push(buf_index);
            token_alloc.remove(token_index);

            println!("shutdown");

            unsafe {
                libc::close(fd);
            }
        } else {
            let len = ret as usize;
            let buf = &buf_alloc[buf_index];

            *token = Token::Write {
                fd,
                buf_index,
                len,
                offset: 0,
            };

            let write_e = opcode::Send::new(types::Fd(fd), buf.as_ptr(), len as _)
                .build()
                .user_data(token_index as _);

            unsafe {
                if sq.push(&write_e).is_err() {
                    backlog.push_back(write_e);
                }
            }
        }*/
    }
    Token::Write {
        fd,
        buf_index,
        offset,
        len,
    } => {
        /*let write_len = ret as usize;

        let entry = if offset + write_len >= len {
            bufpool.push(buf_index);

            *token = Token::Poll { fd };

            opcode::PollAdd::new(types::Fd(fd), libc::POLLIN as _)
                .build()
                .user_data(token_index as _)
        } else {
            let offset = offset + write_len;
            let len = len - offset;

            let buf = &buf_alloc[buf_index][offset..];

            *token = Token::Write {
                fd,
                buf_index,
                offset,
                len,
            };

            opcode::Write::new(types::Fd(fd), buf.as_ptr(), len as _)
                .build()
                .user_data(token_index as _)
        };

        unsafe {
            if sq.push(&entry).is_err() {
                backlog.push_back(entry);
            }
        }*/
    }
}*/