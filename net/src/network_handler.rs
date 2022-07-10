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
        connection_index: u16,
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
    _listener: TcpListener,
    entry: squeue::Entry,
    count: usize,
}

impl AcceptCount {
    fn new(listener: TcpListener, count: usize) -> AcceptCount {
        println!("listen {}", listener.local_addr().unwrap());

        let fd = listener.as_raw_fd();

        AcceptCount {
            _listener: listener,
            entry: opcode::Accept::new(types::Fd(fd), ptr::null_mut(), ptr::null_mut())
                .build()
                .user_data(UserData::Accept.into()),
            count,
        }
    }

    fn push_to(&mut self, ring_squeue: &mut SubmissionQueue) {
        // Push a new accept up to `self.count` times
        while self.count > 0 {
            unsafe {
                match ring_squeue.push(&self.entry) {
                    Ok(_) => self.count -= 1,
                    Err(_) => break,
                }
            }
        }
    }
}

pub struct NewConnectionAccepter<N: NetworkManagerService> {
    network_manager: *const NetworkManager<N>,
    submission_backlog: *mut VecDeque<squeue::Entry>,
}

impl<N: NetworkManagerService> NewConnectionAccepter<N> {
    // todo: safe `accept` that doesn't return pointer

    /// SAFETY:
    /// Returned pointer is valid as long as the connection is part of the relevant NetworkManager
    /// If `request_redirect` is called on the connection, the returned pointer is no longer valid
    pub unsafe fn accept_and_get_ptr(
        &self,
        uninitialized_conn: UninitializedConnection,
        connection_service: N::ConnectionServiceType,
        connections: &mut Slab<(Connection<N>, Box<N::ConnectionServiceType>)>,
    ) -> Option<(*mut Connection<N>, *mut N::ConnectionServiceType)> {
        // Check for connection limit (u16::MAX)
        if connections.len() > u16::MAX as usize {
            std::mem::drop(uninitialized_conn);
            return None;
        }

        let vacant_entry = connections.vacant_entry();
        let connection_index = vacant_entry.key() as u16;

        let mut connection = Connection {
            network_manager: self.network_manager,
            submission_backlog: self.submission_backlog,
            self_index: connection_index,
            fd: uninitialized_conn.fd,
            write_buffers: Slab::new(),
            connection_redirect: None,
            rbuff_data_offset: uninitialized_conn.rbuff_data_offset,
            rbuff_write_offset: uninitialized_conn.rbuff_write_offset,
            read_buffer: uninitialized_conn.read_buffer,
        };

        // Kickstart the recv event
        let mut sq = self
            .network_manager
            .as_ref()
            .unwrap()
            .ring
            .submission_shared();
        let backlog = self.submission_backlog.as_mut().unwrap();

        let read_buffer_ptr = connection
            .read_buffer
            .as_mut_ptr()
            .offset(connection.rbuff_write_offset as isize);
        NetworkManager::<N>::push_recv_event(
            &mut sq,
            backlog,
            connection.fd.0,
            connection_index as _,
            read_buffer_ptr,
            N::ConnectionServiceType::BUFFER_SIZE - connection.rbuff_write_offset as u32,
        );

        println!(
            "putting connection service: {}",
            std::any::type_name::<N::ConnectionServiceType>()
        );

        let entry_ref = vacant_entry.insert((connection, Box::from(connection_service)));
        Some((&mut entry_ref.0, entry_ref.1.as_mut()))
    }
}

/*pub struct ByteSender<'a, 'b> {
    buffer: Vec<u8>,
    output_buffers: &'a mut Slab<Box<[u8]>>,
    connection_index: u16,
    fd: RawFd,
    ring_squeue: &'a mut SubmissionQueue<'b>,
    backlog: &'a mut VecDeque<squeue::Entry>
}

impl<'a, 'b> ByteSender<'a, 'b> {
    pub fn send(&mut self, bytes: &[u8]) {
        self.buffer.put_slice(bytes);
    }
}

impl <'a, 'b> Drop for ByteSender<'a, 'b> {
    fn drop(&mut self) {
        let bytes: Box<[u8]> = Box::from(&self.buffer[..]);

        // get length and pointer for ffi
        let bytes_len = bytes.len();
        let bytes_pointer = &*bytes as *const _ as *const u8;

        // put bytes into output_buffers, get an index
        let write_index = self.output_buffers.insert(bytes);

        // create user data for write operation, needed in order to drop the write_buffer once finished
        let write_user_data = UserData::Write {
            connection_index: self.connection_index,
            write_buffer_index: write_index as u16,
        };

        // submit the write operation to io_uring
        let write_e = opcode::Send::new(types::Fd(self.fd), bytes_pointer, bytes_len as u32)
            .build()
            .user_data(write_user_data.into());
        unsafe {
            if self.ring_squeue.push(&write_e).is_err() {
                self.backlog.push_back(write_e);
            }
        }
    }
}*/

pub struct AutoclosingFd(RawFd);
impl Drop for AutoclosingFd {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.0);
        }
    }
}

pub struct UninitializedConnection {
    fd: AutoclosingFd,
    rbuff_data_offset: usize,
    rbuff_write_offset: usize,
    read_buffer: Vec<u8>,
}

pub struct Connection<N: NetworkManagerService> {
    network_manager: *const NetworkManager<N>,
    submission_backlog: *mut VecDeque<squeue::Entry>,

    self_index: u16,
    fd: AutoclosingFd,
    write_buffers: Slab<Box<[u8]>>,

    connection_redirect:
        Option<Box<dyn FnMut(&mut N, UninitializedConnection, &N::ConnectionServiceType)>>,

    rbuff_data_offset: usize,
    rbuff_write_offset: usize,
    read_buffer: Vec<u8>,
}

impl<N: NetworkManagerService> Connection<N> {
    pub fn get_network_manager(&self) -> &NetworkManager<N> {
        unsafe { self.network_manager.as_ref().unwrap() }
    }

    pub fn read_bytes(&self) -> &[u8] {
        &self.read_buffer[self.rbuff_data_offset..self.rbuff_write_offset as usize]
    }

    pub fn request_redirect(
        &mut self,
        func: impl FnMut(&mut N, UninitializedConnection, &N::ConnectionServiceType) + 'static,
    ) {
        self.connection_redirect = Some(Box::from(func));
    }

    pub fn write(&mut self, bytes: &[u8]) {
        let bytes: Box<[u8]> = Box::from(bytes);

        // get length and pointer for ffi
        let bytes_len = bytes.len();
        let bytes_pointer = &*bytes as *const _ as *const u8;

        // put bytes into output_buffers, get an index
        let write_index = self.write_buffers.insert(bytes);

        // create user data for write operation, needed in order to drop the write_buffer once finished
        let write_user_data = UserData::Write {
            connection_index: self.self_index,
            write_buffer_index: write_index as u16,
        };

        // submit the write operation to io_uring
        let write_e = opcode::Send::new(types::Fd(self.fd.0), bytes_pointer, bytes_len as u32)
            .build()
            .user_data(write_user_data.into());
        unsafe {
            let network_manager = self.network_manager.as_ref().unwrap();
            let mut ring_squeue = network_manager.ring.submission_shared();

            if ring_squeue.push(&write_e).is_err() {
                let backlog = self.submission_backlog.as_mut().unwrap();
                backlog.push_back(write_e);
            }
        }
    }

    fn redirect(mut self, network_service: &mut N, connection_service: &N::ConnectionServiceType) {
        debug_assert!(self.connection_redirect.is_some());
        debug_assert!(self.write_buffers.is_empty());

        let unintialized = UninitializedConnection {
            fd: self.fd,
            rbuff_data_offset: self.rbuff_data_offset,
            rbuff_write_offset: self.rbuff_write_offset,
            read_buffer: self.read_buffer,
        };

        self.connection_redirect.take().unwrap()(network_service, unintialized, connection_service);
    }
}

pub trait ConnectionService
where
    Self: Sized,
{
    const BUFFER_SIZE: u32 = 4_194_304;
    type NetworkManagerServiceType: NetworkManagerService<ConnectionServiceType = Self>;

    fn on_receive(
        &mut self,
        connection: &mut Connection<Self::NetworkManagerServiceType>,
    ) -> anyhow::Result<u32>;
    fn delete(boxed: Box<Self>) {}
}

pub trait NetworkManagerService
where
    Self: Sized,
{
    const TICK_RATE: Option<Duration>;

    type ConnectionServiceType: ConnectionService<NetworkManagerServiceType = Self>;

    fn new_connection_service(&mut self) -> Self::ConnectionServiceType;
    fn tick(
        &mut self,
        connections: &mut Slab<(Connection<Self>, Box<Self::ConnectionServiceType>)>, // todo: move this field into accepter
        accepter: NewConnectionAccepter<Self>,
    );
}

pub struct NetworkManager<N: NetworkManagerService> {
    pub service: N,

    ring: IoUring,
    backlog: VecDeque<squeue::Entry>,
    connections: Slab<(Connection<N>, Box<N::ConnectionServiceType>)>,
    connections_waiting_for_redirect: Slab<u16>,

    tv_sec: u64,
    tv_nsec: u32,
    current_timespec: Timespec,
}

// Publically exposed start method
pub fn start<T: NetworkManagerService>(service: T, addr: Option<&str>) -> anyhow::Result<()> {
    let mut network_manager = NetworkManager::new(service)?;
    network_manager.start(addr)?;
    Ok(())
}

impl<N: NetworkManagerService> NetworkManager<N> {
    fn new(service: N) -> anyhow::Result<Self> {
        // todo: maybe use IORING_SETUP_SQPOLL?

        let ring = IoUring::new(128)?; // 128? too large? too small?
        let backlog = VecDeque::new();

        // let (ring_submitter, raw_ring_squeue, mut ring_cqueue) = ring.split();

        Ok(Self {
            ring,
            backlog,
            connections: Default::default(),
            connections_waiting_for_redirect: Default::default(),

            tv_sec: 0,
            tv_nsec: 0,
            current_timespec: Timespec::new(),

            service,
        })
    }

    fn start(&mut self, addr: Option<&str>) -> anyhow::Result<()> {
        // Start listening on the address `addr`
        let mut accept = None;
        if let Some(addr) = addr {
            accept = Some(AcceptCount::new(TcpListener::bind(addr)?, 3));
        }

        // Split the ring into submitter and completion queue
        let ring_submitter = self.ring.submitter();
        let mut ring_cqueue = unsafe { self.ring.completion_shared() };

        // Separate duration into seconds and nanoseconds
        let tick_duration = N::TICK_RATE.unwrap_or(Duration::from_secs(0));
        let tick_s = tick_duration.as_secs();
        let tick_ns = tick_duration.subsec_nanos();

        // Submit initial tick via Timeout opcode
        if N::TICK_RATE.is_some() {
            let timespec = nix::time::clock_gettime(nix::time::ClockId::CLOCK_MONOTONIC)?;
            self.tv_sec = timespec.tv_sec() as u64 + tick_s;
            self.tv_nsec = timespec.tv_nsec() as u32 + tick_ns;
            self.current_timespec = self.current_timespec.sec(self.tv_sec).nsec(self.tv_nsec);

            NetworkManager::<N>::push_tick_timeout_event(
                unsafe { self.ring.submission_shared() },
                &mut self.backlog,
                &self.current_timespec,
            );
        }

        loop {
            // Ensure there are enough accept events in the squeue
            let mut squeue = unsafe { self.ring.submission_shared() };
            if let Some(ref mut accept) = accept {
                accept.push_to(&mut squeue);
            }
            NetworkManager::<N>::clean_backlog(&mut self.backlog, &ring_submitter, squeue)?;

            self.connections_waiting_for_redirect
                .retain(|_, connection_index| {
                    let is_empty;

                    if let Some((connection, _)) = self.connections.get(*connection_index as _) {
                        is_empty = connection.write_buffers.is_empty();
                    } else {
                        return false; // remove from waitlist
                    }

                    if is_empty {
                        let (connection, service) =
                            self.connections.remove(*connection_index as _);
                        connection.redirect(&mut self.service, &service);

                        N::ConnectionServiceType::delete(service);

                        false // remove from waitlist
                    } else {
                        true // keep in waitlist
                    }
                });

            // Submit from submission queue and wait for some event on the completion queu
            match ring_submitter.submit_and_wait(1) {
                Ok(_) => (),
                Err(ref err) if err.raw_os_error() == Some(libc::EBUSY) => (),
                Err(err) => return Err(err.into()),
            }

            // Sync completion queue
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
                                "io_uring: completion query entry error:\n{:?}\nuserdata: {:?}",
                                io::Error::from_raw_os_error(err),
                                user_data
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
                        if let Some((connection, _)) =
                            self.connections.get_mut(connection_index as usize)
                        {
                            connection
                                .write_buffers
                                .try_remove(write_buffer_index as usize);
                            // println!("removing buffer: {:?}", a);
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
                        self.current_timespec =
                            self.current_timespec.sec(self.tv_sec).nsec(self.tv_nsec);

                        // Push a timeout event using `current_timespec`
                        NetworkManager::<N>::push_tick_timeout_event(
                            unsafe { self.ring.submission_shared() },
                            &mut self.backlog,
                            &self.current_timespec,
                        );

                        // Call the service-defined tick method
                        let accepter = NewConnectionAccepter {
                            network_manager: self,
                            submission_backlog: &mut self.backlog,
                        };
                        self.service.tick(&mut self.connections, accepter);
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
                        let mut read_buffer =
                            vec![0_u8; N::ConnectionServiceType::BUFFER_SIZE as usize];
                        let read_buffer_ptr = read_buffer.as_mut_ptr();

                        let self_ptr: *const NetworkManager<N> = self;

                        let vacant_entry = self.connections.vacant_entry();
                        let connection_index = vacant_entry.key();

                        vacant_entry.insert((
                            Connection {
                                fd: AutoclosingFd(fd),

                                network_manager: self_ptr,
                                submission_backlog: &mut self.backlog,
                                // connections_waiting_for_redirect: &mut self.connections_waiting_for_redirect,
                                self_index: connection_index as u16,

                                connection_redirect: None,

                                rbuff_data_offset: 0,
                                rbuff_write_offset: 0,
                                read_buffer,
                                write_buffers: Slab::new(),
                            },
                            Box::from(self.service.new_connection_service()),
                        ));

                        // todo: fire on_created

                        // Increase accept count, so we can accept another connection
                        if let Some(ref mut accept) = accept {
                            accept.count += 1;
                        }

                        // Kickstart the recv event
                        NetworkManager::<N>::push_recv_event(
                            &mut unsafe { self.ring.submission_shared() },
                            &mut self.backlog,
                            result,
                            connection_index as u16,
                            read_buffer_ptr,
                            N::ConnectionServiceType::BUFFER_SIZE,
                        );
                    }
                    UserData::Read { connection_index } => {
                        // Received some data over the TCP connection

                        if result <= 0 {
                            // Connection closed by remote, clean up
                            NetworkManager::<N>::try_close_connection_by_index(
                                &mut self.connections,
                                connection_index,
                            );
                            continue;
                        } else if let Some((connection, connection_service)) =
                            self.connections.get_mut(connection_index as _)
                        {
                            // Some data has been read

                            connection.rbuff_write_offset += result as usize;
                            if connection.rbuff_write_offset
                                >= N::ConnectionServiceType::BUFFER_SIZE as usize
                            {
                                // Exceeded buffer size... crap...
                                NetworkManager::<N>::try_close_connection_by_index(
                                    &mut self.connections,
                                    connection_index,
                                );
                                break;
                            }

                            // Call the service-defined receive method
                            let receive_result = connection_service.on_receive(connection);

                            match receive_result {
                                Err(err) => {
                                    eprintln!("error: {:?}", err);
                                    NetworkManager::<N>::try_close_connection_by_index(
                                        &mut self.connections,
                                        connection_index,
                                    );
                                    continue;
                                }
                                Ok(remaining_bytes) => {
                                    if remaining_bytes == 0 {
                                        // Fully read

                                        // Since we fully read, we can start receving again from the start of the buffer
                                        connection.rbuff_data_offset = 0;
                                        connection.rbuff_write_offset = 0;
                                    } else {
                                        // Partial read

                                        // Set the `data_offset` to the start of the partially-unread data
                                        connection.rbuff_data_offset = connection
                                            .rbuff_write_offset
                                            - remaining_bytes as usize;

                                        // Keep reading from the existing write offset
                                    }

                                    if connection.connection_redirect.is_some() {
                                        // Don't re-queue the recv event

                                        if !connection.write_buffers.is_empty() {
                                            // Pending write, add connection to the wait list
                                            self.connections_waiting_for_redirect
                                                .insert(connection_index);
                                            continue;
                                        } else {
                                            // No pending writes, we can redirect the connection immediately
                                            // Since we need to remove the connection, we have to do it below
                                        }
                                    } else {
                                        // Re-queue the recv event

                                        let read_buffer_ptr = unsafe {
                                            connection
                                                .read_buffer
                                                .as_mut_ptr()
                                                .offset(connection.rbuff_write_offset as isize)
                                        };
                                        NetworkManager::<N>::push_recv_event(
                                            &mut unsafe { self.ring.submission_shared() },
                                            &mut self.backlog,
                                            connection.fd.0,
                                            connection_index as _,
                                            read_buffer_ptr,
                                            N::ConnectionServiceType::BUFFER_SIZE
                                                - connection.rbuff_write_offset as u32,
                                        );
                                        continue;
                                    }
                                }
                            }
                        }

                        // No pending writes, we can redirect the connection immediately
                        let (connection, service) =
                            self.connections.remove(connection_index as _);
                        connection.redirect(&mut self.service, &service);

                        N::ConnectionServiceType::delete(service);
                    }
                }
            }
        }
    }

    fn clean_backlog(
        backlog: &mut VecDeque<squeue::Entry>,
        submitter: &Submitter,
        mut ring_squeue: SubmissionQueue,
    ) -> anyhow::Result<()> {
        loop {
            // Submit the submission queue to make room
            if ring_squeue.is_full() {
                match submitter.submit() {
                    Ok(_) => (),
                    Err(ref err) if err.raw_os_error() == Some(libc::EBUSY) => break,
                    Err(err) => return Err(err.into()),
                }
                ring_squeue.sync();
            }

            // Move from backlog into submission queue
            match backlog.pop_front() {
                Some(sqe) => unsafe {
                    let _ = ring_squeue.push(&sqe);
                },
                None => break, // Nothing in the submission queue, break out
            }
        }

        Ok(())
    }

    /*fn handle_receive(mut connection: Connection<C>, service: &mut T, connection_index: u16, ring_squeue: &mut BackloggedSubmissionQueue,
            bytes_end: usize) -> Option<(Connection<C>, usize)> {//(anyhow::Result<usize>, Option<Connection<C>>) {


        let remaining_bytes = bytes.len();
        Some((connection, remaining_bytes))
    }*/

    fn try_close_connection_by_index(
        connections: &mut Slab<(Connection<N>, Box<N::ConnectionServiceType>)>,
        connection_index: u16,
    ) {
        if let Some((connection, service)) = connections.try_remove(connection_index as _) {
            NetworkManager::<N>::close_connection(connection);
            N::ConnectionServiceType::delete(service);
        }
    }

    // todo: put method on Connection instead
    fn close_connection(connection: Connection<N>) {
        std::mem::drop(connection);
    }

    fn push_tick_timeout_event(
        mut sq: SubmissionQueue,
        backlog: &mut VecDeque<squeue::Entry>,
        timespec: &Timespec,
    ) {
        let timeout_e = opcode::Timeout::new(timespec)
            .flags(types::TimeoutFlags::ABS)
            .build()
            .user_data(UserData::TickTimeout.into());

        unsafe {
            if sq.push(&timeout_e).is_err() {
                backlog.push_back(timeout_e);
            }
        }
    }

    fn push_recv_event(
        sq: &mut SubmissionQueue,
        backlog: &mut VecDeque<squeue::Entry>,
        fd: RawFd,
        connection_index: u16,
        read_buffer_ptr: *mut u8,
        buffer_size: u32,
    ) {
        let read_e = opcode::Recv::new(types::Fd(fd), read_buffer_ptr, buffer_size)
            .build()
            .user_data(UserData::Read { connection_index }.into());

        unsafe {
            if sq.push(&read_e).is_err() {
                backlog.push_back(read_e);
            }
        }
    }
}
