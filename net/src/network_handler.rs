use std::collections::VecDeque;
use std::io;
use std::net::TcpListener;
use std::os::unix::io::{AsRawFd, RawFd};
use std::ptr;
use std::time::Duration;

use io_uring::types::Timespec;
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
        write_buffer_index: u16,
    },
    #[allow(dead_code)] // Just used to ensure the enum takes up 64 bits
    Unused {
        _1: u8,
        _2: u16,
        _3: u32,
    },
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
        // Push a new accept up to `self.count` times
        while self.count > 0 {
            unsafe {
                match sq.push_unbuffered(&self.entry) {
                    Ok(_) => self.count -= 1,
                    Err(_) => break,
                }
            }
        }
    }
}

pub struct BackloggedSubmissionQueue<'a> {
    ring_squeue: SubmissionQueue<'a>,
    backlog: &'a mut VecDeque<io_uring::squeue::Entry>,
}

impl<'a> BackloggedSubmissionQueue<'a> {
    fn clean_backlog(&mut self, submitter: &Submitter) -> anyhow::Result<()> {
        loop {
            // Submit the submission queue to make room
            if self.ring_squeue.is_full() {
                match submitter.submit() {
                    Ok(_) => (),
                    Err(ref err) if err.raw_os_error() == Some(libc::EBUSY) => break,
                    Err(err) => return Err(err.into()),
                }
                self.ring_squeue.sync();
            }

            // Move from backlog into submission queue
            match self.backlog.pop_front() {
                Some(sqe) => unsafe {
                    let _ = self.ring_squeue.push(&sqe);
                },
                None => break, // Nothing in the submission queue, break out
            }
        }

        Ok(())
    }

    fn sync(&mut self) {
        self.ring_squeue.sync()
    }

    unsafe fn push_unbuffered(
        &mut self,
        entry: &io_uring::squeue::Entry,
    ) -> Result<(), io_uring::squeue::PushError> {
        self.ring_squeue.push(entry)
    }

    unsafe fn push(&mut self, entry: io_uring::squeue::Entry) {
        if self.ring_squeue.push(&entry).is_err() {
            self.backlog.push_back(entry);
        }
    }
}

pub struct ByteSender<'a, 'b> {
    write_buffers: &'a mut Slab<Box<[u8]>>,
    self_connection_index: u16,
    fd: RawFd,
    ring_squeue: &'a mut BackloggedSubmissionQueue<'b>,
}

impl<'a, 'b> ByteSender<'a, 'b> {
    pub fn send(&mut self, bytes: Box<[u8]>) {
        // get length and pointer for ffi
        let bytes_len = bytes.len();
        let bytes_pointer = &*bytes as *const _ as *const u8;

        // put bytes into write_buffers, get an index
        let write_index = self.write_buffers.insert(bytes);

        // create user data for write operation, needed in order to drop the write_buffer once finished
        let write_user_data = UserData::Write {
            connection_index: self.self_connection_index,
            write_buffer_index: write_index as u16,
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

pub struct Connection<C: ?Sized> {
    fd: RawFd,
    write_buffers: Slab<Box<[u8]>>,

    rbuff_data_offset: usize,
    rbuff_write_offset: usize,
    read_buffer: Vec<u8>,
    
    pub service: C,
}

pub trait ConnectionService<T: ?Sized + NetworkManagerService<Self>> {
    const BUFFER_SIZE: u32 = 4_194_304;

    fn receive(&mut self, service: &mut T, bytes: &mut &[u8], byte_sender: &mut ByteSender) -> anyhow::Result<()>;
}

pub trait NetworkManagerService<C: ?Sized + ConnectionService<Self>> {
    const TICK_RATE: Option<Duration>;

    fn new_connection_service(&mut self) -> C;
    fn tick(&mut self) {}
}

pub struct NetworkManager<T: ?Sized + NetworkManagerService<C>, C: ConnectionService<T>> {
    ring: IoUring,
    backlog: VecDeque<io_uring::squeue::Entry>,
    connections: Slab<Connection<C>>,

    tv_sec: u64,
    tv_nsec: u32,
    current_timespec: Timespec,

    service: T,
}

// Publically exposed start method
pub fn start<T: NetworkManagerService<C>, C: ConnectionService<T>>(
    service: T,
    addr: &str,
) -> anyhow::Result<()> {
    let mut network_manager = NetworkManager::new(service)?;
    network_manager.start(addr)?;
    Ok(())
}

impl<T: NetworkManagerService<C>, C: ConnectionService<T>> NetworkManager<T, C> {
    fn new(service: T) -> anyhow::Result<Self> {
        // todo: maybe use IORING_SETUP_SQPOLL?

        let ring = IoUring::new(128)?; // 128? too large? too small?
        let connection_alloc = Slab::with_capacity(64);
        let backlog = VecDeque::new();

        Ok(Self {
            ring,
            backlog,
            connections: connection_alloc,

            tv_sec: 0,
            tv_nsec: 0,
            current_timespec: Timespec::new(),

            service,
        })
    }

    fn start(&mut self, addr: &str) -> anyhow::Result<()> {
        // Start listening on the address `addr`
        let listener = TcpListener::bind(addr)?;
        let mut accept = AcceptCount::new(listener.as_raw_fd(), 3);
        println!("listen {}", listener.local_addr()?);

        // Split the ring into the queues, create BackloggedSubmissionQueue
        let (ring_submitter, raw_ring_squeue, mut ring_cqueue) = self.ring.split();
        let mut ring_squeue = BackloggedSubmissionQueue {
            ring_squeue: raw_ring_squeue,
            backlog: &mut self.backlog,
        };

        // Submit initial tick via Timeout opcode
        let tick_duration = T::TICK_RATE.unwrap_or(Duration::from_secs(0));
        let tick_s = tick_duration.as_secs();
        let tick_ns = tick_duration.subsec_nanos();

        if let Some(_) = T::TICK_RATE {
            let timespec = nix::time::clock_gettime(nix::time::ClockId::CLOCK_MONOTONIC)?;
            self.tv_sec = timespec.tv_sec() as u64 + tick_s;
            self.tv_nsec = timespec.tv_nsec() as u32 + tick_ns;
            self.current_timespec = self.current_timespec.sec(self.tv_sec).nsec(self.tv_nsec);

            NetworkManager::<T, C>::push_tick_timeout_event(
                &self.current_timespec,
                &mut ring_squeue,
            );
        }

        loop {
            // Ensure there are enough accept events in the squeue
            accept.push_to(&mut ring_squeue);
            ring_squeue.sync();

            // Submit from submission queue and wait for some event
            match ring_submitter.submit_and_wait(1) {
                Ok(_) => (),
                Err(ref err) if err.raw_os_error() == Some(libc::EBUSY) => (),
                Err(err) => return Err(err.into()),
            }

            // Clean backlog
            ring_squeue.clean_backlog(&ring_submitter)?;

            // Sync submittion and completion queues
            ring_squeue.sync();
            ring_cqueue.sync();

            // Read all the entries in the completion queue
            for cqe in &mut ring_cqueue {
                let result = cqe.result();
                let user_data = UserData::from(cqe.user_data());

                // Handle cqe error
                if result < 0 {
                    match -result {
                        libc::ETIME => (),
                        err => {
                            eprintln!(
                                "userdata: {:?} got error: {:?}",
                                user_data,
                                io::Error::from_raw_os_error(err)
                            );
                            continue;
                        }
                    }
                }

                match user_data {
                    UserData::Unused { _1, _2, _3 } => {
                        panic!("unused")
                    }
                    UserData::Write {
                        connection_index,
                        write_buffer_index,
                    } => {
                        // Write has completed, we can drop the associated buffer
                        if let Some(connection) =
                            self.connections.get_mut(connection_index as usize)
                        {
                            connection
                                .write_buffers
                                .try_remove(write_buffer_index as usize);
                        }
                    }
                    UserData::TickTimeout => {
                        // Finished waiting for our tick
                        let timespec =
                            nix::time::clock_gettime(nix::time::ClockId::CLOCK_MONOTONIC)?;

                        // Update `current_timespec`
                        if self.tv_sec + 5 < timespec.tv_sec() as u64 {
                            // Major lag spike, 5 seconds behind, we can skip processing some ticks

                            let skipped_ms = (timespec.tv_sec() as i64 - self.tv_sec as i64) * 1000
                                + (timespec.tv_nsec() as i64 - self.tv_nsec as i64) / 1_000_000;
                            eprintln!("Can't keep up! Did the system time change, or is the server overloaded? Running {}ms behind, skipping {} tick(s)",
                                skipped_ms, skipped_ms/(tick_duration.as_millis() as i64));

                            self.tv_sec = timespec.tv_sec() as u64 + tick_s;
                            self.tv_nsec = timespec.tv_nsec() as u32 + tick_ns;
                        } else {
                            // Increment `current_timespec` by tick duration
                            self.tv_sec += tick_s;
                            self.tv_nsec += tick_ns;
                            if self.tv_nsec > 1_000_000_000 {
                                self.tv_sec += 1;
                                self.tv_nsec -= 1_000_000_000;
                            }
                        }
                        self.current_timespec = self.current_timespec.sec(self.tv_sec).nsec(self.tv_nsec);

                        // Push a timeout event using `current_timespec`
                        NetworkManager::<T, C>::push_tick_timeout_event(
                            &self.current_timespec,
                            &mut ring_squeue,
                        );

                        // Call the service-defined tick method
                        self.service.tick();
                    }
                    UserData::Accept => {
                        // New TCP connection has been accepted
                        let fd = result;

                        // Check for connection limit (u16::MAX)
                        if self.connections.len() > u16::MAX as usize {
                            unsafe {
                                libc::close(fd);
                            }
                            break;
                        }

                        // Allocate a new connection struct
                        let mut read_buffer = vec![0_u8; C::BUFFER_SIZE as usize];
                        let read_buffer_ptr = read_buffer.as_mut_ptr();
                        let connection_index = self.connections.insert(Connection {
                            fd,
                            rbuff_data_offset: 0,
                            rbuff_write_offset: 0,
                            read_buffer,
                            write_buffers: Slab::new(),
                            service: self.service.new_connection_service(),
                        });

                        // Increase accept count, so we can accept another connection
                        accept.count += 1;

                        // Kickstart the recv event
                        NetworkManager::<T, C>::push_recv_event(
                            &mut ring_squeue,
                            result,
                            connection_index as u16,
                            read_buffer_ptr,
                            C::BUFFER_SIZE,
                        );
                    }
                    UserData::Read { connection_index } => {
                        // Received some data over the TCP connection

                        if result <= 0 {
                            // Connection closed by remote, clean up
                            NetworkManager::<T, C>::try_close_connection_by_index(&mut self.connections, connection_index);
                        } else if let Some(connection) = self.connections.get_mut(connection_index as _) {
                            // Some data has been read

                            let bytes_end = connection.rbuff_write_offset + result as usize;
                            if bytes_end >= C::BUFFER_SIZE as usize {
                                // Exceeded buffer size... crap...
                                NetworkManager::<T, C>::try_close_connection_by_index(&mut self.connections, connection_index);
                                break;
                            }

                            // Create ByteSender wrapper that contains all the needed information for writing
                            // Unfortunately just passing around the connection isn't enough as the `ring_squeue`
                            // is needed to actually push the event. Theoretically we could refactor some stuff and
                            // use a channel, but the overhead probably isn't worth it
                            let mut byte_sender = ByteSender {
                                write_buffers: &mut connection.write_buffers,
                                self_connection_index: connection_index as _,
                                fd: connection.fd,
                                ring_squeue: &mut ring_squeue,
                            };

                            // Call the service-defined receive method
                            let mut bytes = &connection.read_buffer[connection.rbuff_data_offset..bytes_end];
                            let num_bytes_received = bytes.len();

                            let receive_result = connection.service.receive(&mut self.service, &mut bytes, &mut byte_sender);

                            if receive_result.is_err() {
                                // An error occurred with this connection, forcibly close it
                                // eprintln!("error: {:?}", err);
                                NetworkManager::<T, C>::try_close_connection_by_index(&mut self.connections, connection_index);
                            } else {
                                if bytes.len() == 0 {
                                    // Fully read

                                    // Since we fully read, we can start receving again from the start of the buffer
                                    connection.rbuff_data_offset = 0;
                                    connection.rbuff_write_offset = 0;
                                } else {
                                    // Partial read

                                    // Set the `data_offset` to the start of the partially-unread data
                                    let bytes_consumed = num_bytes_received - bytes.len();
                                    connection.rbuff_data_offset += bytes_consumed;

                                    // Set the `write_offset` to the end of the byte stream,
                                    // ready to receive more bytes to complete the partial read
                                    connection.rbuff_write_offset = bytes_end;
                                }

                                // Re-queue the recv event
                                let read_buffer_ptr = unsafe { connection.read_buffer.as_mut_ptr().offset(connection.rbuff_write_offset as isize) };
                                NetworkManager::<T, C>::push_recv_event(
                                    &mut ring_squeue,
                                    connection.fd,
                                    connection_index as _,
                                    read_buffer_ptr,
                                    C::BUFFER_SIZE - connection.rbuff_write_offset as u32,
                                ); 
                            }
                        }
                    }
                }
            }
        }
    }

    fn try_close_connection_by_index(connections: &mut Slab<Connection<C>>, connection_index: u16) {
        // Remove the connection from the pool and close the corresponding file descriptor
        if let Some(conn) = connections.try_remove(connection_index as _) {
            unsafe {
                libc::close(conn.fd);
            }

            // The kernel should refuse to operate on the connection after it is closed
            // Therefore, it should be safe to drop `connection` (and it's `write_buffers`) here
            std::mem::drop(conn);
        }
    }

    fn push_tick_timeout_event(timespec: &Timespec, ring_squeue: &mut BackloggedSubmissionQueue) {
        let op = opcode::Timeout::new(timespec)
            .flags(types::TimeoutFlags::ABS)
            .build()
            .user_data(UserData::TickTimeout.into());

        unsafe {
            ring_squeue.push(op);
        }
    }

    fn push_recv_event(
        squeue: &mut BackloggedSubmissionQueue<'_>,
        fd: RawFd,
        connection_index: u16,
        read_buffer_ptr: *mut u8,
        buffer_size: u32,
    ) {
        let read_e = opcode::Recv::new(types::Fd(fd), read_buffer_ptr, buffer_size)
            .build()
            .user_data(UserData::Read { connection_index }.into());

        unsafe {
            squeue.push(read_e);
        }
    }
}
