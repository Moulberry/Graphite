use concierge::Concierge;
use concierge::ConciergeService;
use net::network_handler::UninitializedConnection;

mod universe;

struct MyConciergeImpl {
    counter: u8
}

impl ConciergeService for MyConciergeImpl {
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

    fn accept_player(&mut self, player_connection: UninitializedConnection, protoplayer: concierge::ProtoPlayer<Self>) {
        println!("managed to get connection: {:?}", protoplayer.username);
        let universe = universe::create_and_start();
        universe.send(player_connection).unwrap();
    }
}

fn main() {
    Concierge::bind("127.0.0.1:25565", MyConciergeImpl {
        counter: 0
    }).unwrap();
}