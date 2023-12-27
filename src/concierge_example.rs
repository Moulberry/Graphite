use std::{sync::{Arc, Mutex}, io::Write};

use graphite_concierge::LoginInformation;
use mio::net::TcpStream;

fn main() {
    println!("Starting concierge on port 25565!");

    let sender = Box::new(|login_information: LoginInformation, mut stream: TcpStream| {
        let message = format!("Hello {}, {:x}!", login_information.username, login_information.uuid);
        stream.write(&[(message.len()+4) as u8]).unwrap();
        stream.write(&[1]).unwrap();
        stream.write(&[8]).unwrap();
        stream.write(&[0]).unwrap();
        stream.write(&[message.len() as u8]).unwrap();
        stream.write(message.as_bytes()).unwrap();

        // todo: figure out how to wait for data to be written before disconnecting
    });

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

    println!("Finished concierge")
}
