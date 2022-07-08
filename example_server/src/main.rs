use concierge::Concierge;
use concierge::ConciergeService;
use net::network_handler::Connection;

// mod universe;

struct MyConciergeImpl {
    counter: u8,
    connected_players: Vec<Connection<Concierge<Self>>>
}

impl ConciergeService for MyConciergeImpl {
    /*fn get_message(&mut self) -> String {
        self.counter += 1;
        let string = format!("times called: {}", self.counter);
        string
    }*/

    fn get_serverlist_response(&mut self) -> String {
        self.counter += 1;
        format!("{{\
            \"version\": {{
                \"name\": \"1.19\",
                \"protocol\": 759
            }},
            \"players\": {{
                \"max\": 100,
                \"online\": {},
                \"sample\": []
            }},
            \"description\": {{
                \"text\": \"Hello world\"
            }},
            \"favicon\": \"data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAEAAAABACAIAAAAlC+aJAAABGklEQVRo3u2aUQ7EIAhEbcNReiPP6Y16F/djk/1bozJASYffJu08BRxMj957yRxnSR4EIMDbAQTylrvWwdOrNTuAY6+NjhV7YiwDbEg3xVgDUKq3wIgp4rtW1FqYAEwuMAQDk0L/FE/q02TUqVR/tTb4vGkDBaTQjL4xIU/i91gJVNeDV8gZ+HnIorAGCJAAwKIBAACAhixyIvsyKL3Qg0bKqzXnbZlNoXmH/NwitvBkeuC1Ira2lk5daBvDAn6/iH9qAi+Fyva9EDDvlYTxVkJZx/RCBMgHgO1L3IEXAmANn+SV7r0DRk5b0im2BfAfaCRcn/JYkBIXwXejDzmPJZ1iVwCHAfrgD08EIAABCEAAAhCAAAQgwG58AEFWdXlZzlUbAAAAAElFTkSuQmCC\"
        }}", self.counter)
    }

    fn accept_player(&mut self, player_connection: Connection<Concierge<Self>>) {
        self.connected_players.push(player_connection);
        //let universe = universe::create_and_start();
        //universe.send(player_connection);

        // fake play, for testing
            
        /*std::thread::sleep(std::time::Duration::from_millis(100));
            
        

        println!("accepted player from concierge!");*/
    }
}

fn main() {
    Concierge::bind("127.0.0.1:25565", MyConciergeImpl {
        counter: 0,
        connected_players: Vec::new()
    }).unwrap();
}