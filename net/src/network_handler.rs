use std::collections::VecDeque;
use std::io;
use std::net::TcpListener;
use std::os::unix::io::{AsRawFd, RawFd};
use std::ptr;
use std::time::Duration;

use anyhow::bail;
use io_uring::types::Timespec;
use io_uring::{opcode, squeue, types, IoUring, SubmissionQueue, Submitter};
use slab::Slab;

#[derive(Debug, PartialEq, Copy, Clone)]
#[repr(u16)]
enum UserData {
    Accept,
    CancelRead,
    Read {
        // hack in order to make that top bits of the u64 not filled with garbage data
        connection_index: u16,
        _2: u16,
        _3: u16,
    },
    TickTimeout,
    Write {
        connection_index: u16,
        write_buffer_index: u16,
    },
}

impl UserData {
    fn create_read(connection_index: u16) -> UserData {
        UserData::Read {
            connection_index,
            _2: 0,
            _3: 0,
        }
    }

    fn is_write(&self) -> bool {
        const COMPARE_TO: UserData = UserData::Write {
            connection_index: 0,
            write_buffer_index: 0,
        };
        std::mem::discriminant(&COMPARE_TO) == std::mem::discriminant(self)
    }
}

impl From<u64> for UserData {
    fn from(data: u64) -> Self {
        unsafe { std::mem::transmute(data) }
    }
}

impl From<UserData> for u64 {
    fn from(data: UserData) -> Self {
        unsafe { std::mem::transmute(data) }
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

#[allow(type_alias_bounds)]
pub type ConnectionSlab<N: NetworkManagerService> = Slab<(Connection<N>, N::ConnectionServiceType)>;
#[allow(type_alias_bounds)]
type FnConnectionRedirect<N: NetworkManagerService> = Box<dyn FnMut(&mut N, UninitializedConnection, &N::ConnectionServiceType)>;

pub struct NewConnectionAccepter<N: NetworkManagerService> {
    network_manager: *const NetworkManager<N>,
    submission_backlog: *mut VecDeque<squeue::Entry>,
    connections_waiting_for_close: *mut Vec<u16>,
}

impl<N: NetworkManagerService> NewConnectionAccepter<N> {
    pub fn accept_and_get_index(
        &self,
        uninitialized_conn: UninitializedConnection,
        connection_service: N::ConnectionServiceType,
        connections: &mut ConnectionSlab<N>,
    ) -> anyhow::Result<u16> {
        // Check for connection limit (u16::MAX)
        if connections.len() > u16::MAX as usize {
            std::mem::drop(uninitialized_conn);
            bail!("connection limit has been reached");
        }

        let vacant_entry = connections.vacant_entry();
        let connection_index = vacant_entry.key() as u16;

        let mut connection = Connection {
            network_manager: self.network_manager,
            submission_backlog: self.submission_backlog,
            connections_waiting_for_close: self.connections_waiting_for_close,

            is_processing_read: false,
            close_requested: false,

            self_index: connection_index,
            fd: uninitialized_conn.fd,
            write_buffers: Slab::new(),
            connection_redirect: None,
            rbuff_data_offset: uninitialized_conn.rbuff_data_offset,
            rbuff_write_offset: uninitialized_conn.rbuff_write_offset,
            read_buffer: uninitialized_conn.read_buffer,
        };

        // Kickstart the recv event
        unsafe {
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
                .add(connection.rbuff_write_offset);
            NetworkManager::<N>::push_recv_event(
                &mut sq,
                backlog,
                connection.fd.0,
                connection_index as _,
                read_buffer_ptr,
                N::ConnectionServiceType::BUFFER_SIZE - connection.rbuff_write_offset as u32,
            );
        }

        vacant_entry.insert((connection, connection_service));
        Ok(connection_index)
    }
}

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
    connections_waiting_for_close: *mut Vec<u16>,

    self_index: u16,
    fd: AutoclosingFd,
    write_buffers: Slab<Box<[u8]>>,

    close_requested: bool,
    connection_redirect: Option<FnConnectionRedirect<N>>,

    is_processing_read: bool,

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
        if self.is_processing_read {
            self.connection_redirect = Some(Box::from(func));
        } else {
            unimplemented!();
            // put into waiting list somehow??
        }
    }

    pub fn request_close(&mut self) {
        self.close_requested = true;

        if self.is_processing_read {
            // handle in read process
        } else {
            // submit the cancel read operation
            let cancel_e = opcode::AsyncCancel::new(UserData::create_read(self.self_index).into())
                .build()
                .user_data(UserData::CancelRead.into());
            unsafe {
                let network_manager = self.network_manager.as_ref().unwrap();
                let mut ring_squeue = network_manager.ring.submission_shared();

                if ring_squeue.push(&cancel_e).is_err() {
                    let backlog = self.submission_backlog.as_mut().unwrap();
                    backlog.push_back(cancel_e);
                }

                // Submit into cancel list
                self.connections_waiting_for_close
                    .as_mut()
                    .unwrap()
                    .push(self.self_index);
            }
        }
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

    fn close(self) {
        std::mem::drop(self);
    }

    fn redirect(mut self, network_service: &mut N, connection_service: N::ConnectionServiceType) {
        debug_assert!(self.connection_redirect.is_some());
        debug_assert!(self.write_buffers.is_empty());

        let unintialized = UninitializedConnection {
            fd: self.fd,
            rbuff_data_offset: self.rbuff_data_offset,
            rbuff_write_offset: self.rbuff_write_offset,
            read_buffer: self.read_buffer,
        };

        self.connection_redirect.take().unwrap()(
            network_service,
            unintialized,
            &connection_service,
        );

        connection_service.close();
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

    fn close(self) {
        std::mem::drop(self);
    }
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
        connections: &mut ConnectionSlab<Self>, // todo: move this field into accepter
        accepter: NewConnectionAccepter<Self>,
    ) -> anyhow::Result<()>;
}

pub struct NetworkManager<N: NetworkManagerService> {
    pub service: N,

    ring: IoUring,
    backlog: VecDeque<squeue::Entry>,
    connections: ConnectionSlab<N>,

    connections_waiting_for_redirect: Slab<u16>,
    connections_waiting_for_close: Vec<u16>,

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
            connections_waiting_for_close: Default::default(),

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

            // Submit from submission queue and wait for some event on the completion queue
            match ring_submitter.submit_and_wait(1) {
                Ok(_) => (),
                Err(ref err) if err.raw_os_error() == Some(libc::EBUSY) => (),
                Err(err) => return Err(err.into()),
            }

            // Sync completion queue
            ring_cqueue.sync();

            // Redirect connections
            self.connections_waiting_for_redirect
                .retain(|_, connection_index| {
                    let (connection, _) = self.connections.get(*connection_index as _).unwrap();
                    let is_empty = connection.write_buffers.is_empty();

                    if is_empty {
                        let (connection, service) = self.connections.remove(*connection_index as _);
                        connection.redirect(&mut self.service, service);

                        false // remove from waitlist
                    } else {
                        true // keep in waitlist
                    }
                });

            // Close connections
            self.connections_waiting_for_close
                .iter()
                .for_each(|connection_index| {
                    NetworkManager::<N>::try_close_connection_by_index(
                        &mut self.connections,
                        *connection_index,
                    );
                });
            self.connections_waiting_for_close.clear();

            // Read all the entries in the completion queue
            for cqe in &mut ring_cqueue {
                let result = cqe.result();
                let user_data = UserData::from(cqe.user_data());

                // Handle cqe error
                if result < 0 {
                    match -result {
                        libc::EALREADY | libc::ECANCELED => continue,
                        libc::ETIME | libc::ECONNRESET => (),
                        err => {
                            const EBADFD: i32 = 9;
                            if user_data.is_write() && err == EBADFD {
                                continue;
                            }
                            panic!(
                                "io_uring: completion query entry error:\n  {:?}\n  userdata: {:?}",
                                io::Error::from_raw_os_error(err),
                                user_data
                            );
                        }
                    }
                }

                match user_data {
                    UserData::CancelRead => (),
                    UserData::Write {
                        connection_index,
                        write_buffer_index,
                    } => {
                        // Write has completed, we can drop the associated buffer
                        let (connection, _) =
                            self.connections.get_mut(connection_index as usize).unwrap();

                        connection
                            .write_buffers
                            .try_remove(write_buffer_index as usize)
                            .unwrap();
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
                            connections_waiting_for_close: &mut self.connections_waiting_for_close,
                        };
                        self.service.tick(&mut self.connections, accepter)?;
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
                                connections_waiting_for_close: &mut self
                                    .connections_waiting_for_close,

                                // connections_waiting_for_redirect: &mut self.connections_waiting_for_redirect,
                                self_index: connection_index as u16,
                                is_processing_read: false,

                                connection_redirect: None,
                                close_requested: false,

                                rbuff_data_offset: 0,
                                rbuff_write_offset: 0,
                                read_buffer,
                                write_buffers: Slab::new(),
                            },
                            self.service.new_connection_service(),
                        ));

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
                    UserData::Read {
                        connection_index,
                        _2,
                        _3,
                    } => {
                        // Received some data over the TCP connection

                        if result <= 0 {
                            // Connection closed by remote, clean up
                            NetworkManager::<N>::try_close_connection_by_index(
                                &mut self.connections,
                                connection_index,
                            );
                            continue;
                        } else {
                            // Some data has been read

                            let connection_result = self.connections.get_mut(connection_index as _);
                            if connection_result.is_none() {
                                continue;
                            }
                            let (connection, connection_service) = connection_result.unwrap();

                            // Sanity check, this should never be set
                            debug_assert!(!connection.close_requested);

                            connection.rbuff_write_offset += result as usize;
                            if connection.rbuff_write_offset
                                >= N::ConnectionServiceType::BUFFER_SIZE as usize
                            {
                                // Exceeded buffer size... crap...
                                NetworkManager::<N>::close_connection_by_index(
                                    &mut self.connections,
                                    connection_index,
                                );
                                continue;
                            }

                            // Call the service-defined receive method
                            connection.is_processing_read = true;
                            let receive_result = connection_service.on_receive(connection);
                            connection.is_processing_read = false;

                            if let Err(err) = receive_result {
                                // Error during handling
                                eprintln!("error: {:?}", err);
                                NetworkManager::<N>::close_connection_by_index(
                                    &mut self.connections,
                                    connection_index,
                                );
                                continue;
                            }

                            // Update read and write offsets
                            let remaining_bytes = receive_result.unwrap();
                            if remaining_bytes == 0 {
                                // Fully read

                                // Since we fully read, we can start receving again from the start of the buffer
                                connection.rbuff_data_offset = 0;
                                connection.rbuff_write_offset = 0;
                            } else {
                                // Partial read

                                // Set the `data_offset` to the start of the partially-unread data
                                connection.rbuff_data_offset =
                                    connection.rbuff_write_offset - remaining_bytes as usize;

                                // Keep reading from the existing write offset
                            }

                            if connection.close_requested {
                                // Close requested
                                NetworkManager::<N>::close_connection_by_index(
                                    &mut self.connections,
                                    connection_index,
                                );
                            } else if connection.connection_redirect.is_some() {
                                // Redirect requested
                                if !connection.write_buffers.is_empty() {
                                    // Pending write, add connection to the wait list
                                    self.connections_waiting_for_redirect
                                        .insert(connection_index);
                                } else {
                                    // No pending writes, we can redirect the connection immediately
                                    let (connection, service) =
                                        self.connections.remove(connection_index as _);
                                    connection.redirect(&mut self.service, service);
                                }
                            } else {
                                // Re-queue the recv event, lets read some more data!
                                let read_buffer_ptr = unsafe {
                                    connection
                                        .read_buffer
                                        .as_mut_ptr()
                                        .add(connection.rbuff_write_offset)
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
                            }
                        }
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

    fn close_connection_by_index(connections: &mut ConnectionSlab<N>, connection_index: u16) {
        let (connection, service) = connections.remove(connection_index as _);
        connection.close();
        N::ConnectionServiceType::close(service);
    }

    fn try_close_connection_by_index(connections: &mut ConnectionSlab<N>, connection_index: u16) {
        if let Some((connection, service)) = connections.try_remove(connection_index as _) {
            connection.close();
            N::ConnectionServiceType::close(service);
        }
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
        // todo: maybe try RecvMsg?
        let read_e = opcode::Recv::new(types::Fd(fd), read_buffer_ptr, buffer_size)
            .build()
            .user_data(UserData::create_read(connection_index).into());

        unsafe {
            if sq.push(&read_e).is_err() {
                backlog.push_back(read_e);
            }
        }
    }
}
