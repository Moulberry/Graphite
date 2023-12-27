use std::{sync::{Arc, Mutex}, thread};

use graphite_concierge::LoginInformation;
use mio::net::TcpStream;

fn main() {
    println!("Starting concierge on port 25565!");

    let (mut core_network, mut core_sender) = graphite_core_server::CoreServer::new();

    let sender = Box::new(move |login_information: LoginInformation, stream: TcpStream| {
        core_sender.send(stream, login_information);
    });

    thread::spawn(move || {
        graphite_concierge::listen("0.0.0.0:25565", sender, Arc::new(Mutex::new(
            r#"{
                "version": {
                    "name": "1.20.4",
                    "protocol": 765
                },
                "players": {
                    "max": 100,
                    "online": 5,
                    "sample": [
                        {
                            "name": "thinkofdeath",
                            "id": "4566e69f-c907-48ee-8d71-d7ba5aa00d20"
                        }
                    ]
                },
                "description": {
                    "text": "Hello world"
                },
                "favicon": "data:image/png;base64,<data>",
                "enforcesSecureChat": true,
                "previewsChat": true
            }"#.into()
        )));
    });

    println!("Starting core server");

    core_network.listen().unwrap();
}
