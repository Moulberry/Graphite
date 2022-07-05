use std::collections::VecDeque;
use std::net::TcpListener;
use std::ops::Add;
use std::os::unix::io::{AsRawFd, RawFd};
use std::{io, ptr};

use io_uring::{opcode, squeue, types, IoUring, SubmissionQueue};
use slab::Slab;

#[derive(Clone, Debug)]
enum Token {
    Accept,
    /*Poll {
        fd: RawFd,
    },*/
    Read {
        fd: RawFd,
        buf_index: usize,
    },
    Write {
        fd: RawFd,
        buf_index: usize,
        offset: usize,
        len: usize,
    },
    Timeout,
}

pub struct AcceptCount {
    entry: squeue::Entry,
    count: usize,
}

impl AcceptCount {
    fn new(fd: RawFd, token: usize, count: usize) -> AcceptCount {
        AcceptCount {
            entry: opcode::Accept::new(types::Fd(fd), ptr::null_mut(), ptr::null_mut())
                .build()
                .user_data(token as _),
            count,
        }
    }

    pub fn push_to(&mut self, sq: &mut SubmissionQueue<'_>) {
        while self.count > 0 {
            unsafe {
                match sq.push(&self.entry) {
                    Ok(_) => self.count -= 1,
                    Err(_) => break,
                }
            }
        }

        sq.sync();
    }
}

pub fn main() -> anyhow::Result<()> {
    // todo: maybe use IORING_SETUP_SQPOLL?

    let mut ring = IoUring::new(256)?; // 256? too large?
    let listener = TcpListener::bind(("127.0.0.1", 3456))?;

    let mut backlog = VecDeque::new();
    //let mut bufpool = Vec::with_capacity(64);
    //let mut buf_alloc = Slab::with_capacity(64);
    let mut token_alloc = Slab::with_capacity(64);

    println!("listen {}", listener.local_addr()?);

    let (submitter, mut sq, mut cq) = ring.split();

    let mut accept = AcceptCount::new(listener.as_raw_fd(), token_alloc.insert(Token::Accept), 3);


    let nbufs = 4;
    let len = 2097152 * 4; // Room for 4 max-size packets, any more than this and a client is disconnected
    let addr = unsafe { std::alloc::alloc_zeroed(std::alloc::Layout::from_size_align(nbufs * len, 8).unwrap()) };

    println!("using addr: {:?}", addr);

    // num_buffers = num_cpus * 2
    unsafe {
        let op = opcode::ProvideBuffers::new(addr, len as i32, nbufs as u16, 1337, 0)
                    .build()
                    .user_data(1000);

        sq.push(&op).unwrap();
    }

    static TIMESPEC: types::Timespec = types::Timespec::new().nsec(50_000000);
    if false {
        let tick_token = token_alloc.insert(Token::Timeout );

        println!("tick token: {}", tick_token);

        let op = opcode::Timeout::new(&TIMESPEC)
                .build()
                .user_data(tick_token as _);

        unsafe {
            if sq.push(&op).is_err() {
                backlog.push_back(op);
            }
        }   
    }

    accept.push_to(&mut sq);

    loop {
        match submitter.submit_and_wait(1) {
            Ok(_) => (),
            Err(ref err) if err.raw_os_error() == Some(libc::EBUSY) => println!("ebusy"),
            Err(err) => return Err(err.into()),
        }
        cq.sync();

        // clean backlog
        loop {
            if sq.is_full() {
                println!("sq full!");
                match submitter.submit() {
                    Ok(_) => (),
                    Err(ref err) if err.raw_os_error() == Some(libc::EBUSY) => break,
                    Err(err) => return Err(err.into()),
                }
            }
            sq.sync();

            match backlog.pop_front() {
                Some(sqe) => unsafe {
                    println!("had something in backlog!");
                    let _ = sq.push(&sqe);
                },
                None => break,
            }
        }

        accept.push_to(&mut sq);

        for cqe in &mut cq {
            let ret = cqe.result();
            let token_index = cqe.user_data() as usize;

            if token_index >= 1000 {
                continue;
            }

            if ret < 0 {
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

                    let op = opcode::Timeout::new(&TIMESPEC)
                            .build()
                            .user_data(token_index as _);

                    unsafe {
                        if sq.push(&op).is_err() {
                            backlog.push_back(op);
                        }
                    }     
                }
                Token::Accept => {
                    println!("accept");

                    accept.count += 1;

                    let fd = ret;
                    /*let poll_token = token_alloc.insert(Token::Poll { fd });

                    let poll_e = opcode::PollAdd::new(types::Fd(fd), libc::POLLIN as _)
                        .build()
                        .user_data(poll_token as _);*/

                    let read_token = token_alloc.insert(Token::Read { fd, buf_index: 0 });

                    let read_e = opcode::Read::new(types::Fd(fd), ptr::null_mut(), 0)
                        .buf_group(1337)
                        .build()
                        .flags(io_uring::squeue::Flags::BUFFER_SELECT)
                        .user_data(read_token as _);

                    unsafe {
                        if sq.push(&read_e).is_err() {
                            backlog.push_back(read_e);
                        }
                    }
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
                    println!("flags: {}", cqe.flags());
                    let buffer_index = io_uring::cqueue::buffer_select(cqe.flags());
                    println!("buffer select?: {:?}", buffer_index);
                    
                    if let Some(buffer_index) = buffer_index {
                        let start = unsafe { addr.add(len*buffer_index as usize) };
                        println!("read?: {}", unsafe { *start });
                    }

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
            }
        }
    }
}
