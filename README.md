# Graphite

1.19.1 Minecraft Server

# TODO

- [ ] Complete `protocol` with all 1.19.1 Minecraft packets 
- [x] Command dispatch system
- [ ] Viewable objects (entities)
- [ ] Allow multiple players to connect (and see each other)
- [ ] Allow chunk/world modification

# Subprojects

- `example_server`: Example server using various Graphite components
- `server`: The minecraft server. Entities, chunks, players, all that good stuff
- `concierge`: Component that accepts new connections, handles status and login. Can be used to create proxies, servers, etc.
- `protocol`: The minecraft protocol
- `net`: Networking components and utilities
- `binary`: Zero-copy serialization
- `command`: Command dispatch and low-level creation
- `command_derive`: Attribute macro to easily create commands
- `sticky`: Collection(s) that guarantee the memory-location of its contents

# Warning

Project is currently under heavy development, many things are completely non-functional

# How to run the example server

!!! Currently Graphite only supports modern versions of Linux with io_uring !!!  
(An alternative network backend will be available eventually, but is not a priority)  

```
$ cargo run --bin example_server
```
