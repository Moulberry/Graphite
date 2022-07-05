use std::{net::SocketAddr, time::Duration, thread};
use tokio::net::{TcpListener, TcpStream};
use std::sync::{RwLock, Arc};

pub struct Concierge<T: ConciergeService> {
    service: T
}

impl <T: 'static + ConciergeService + std::marker::Sync + std::marker::Send> Concierge<T> {
    pub async fn bind(addr: &str, service: T) -> anyhow::Result<()> {
        let listener = TcpListener::bind(addr).await?;

        let concierge = Concierge { service };

        let locked_concierge = Arc::new(RwLock::new(concierge));

        // accept connections and process them
        loop {
            let (socket, _) = listener.accept().await?;

            let locked_concierge = Arc::clone(&locked_concierge);
            tokio::spawn(async move {
                Concierge::<T>::process_socket(locked_concierge, socket);
            });
        }
    }

    fn process_socket(locked_concierge: Arc<RwLock<Concierge<T>>>, stream: TcpStream) {
        stream.try_write(locked_concierge.write().unwrap().service.get_message().as_bytes()).unwrap();
        thread::sleep(Duration::from_secs(2));
    }
}

pub trait ConciergeService {
    fn get_message(&mut self) -> String;
    //fn get_serverlist_response() -> String;
}