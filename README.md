# Graphite

1.19.1 Minecraft Server

# TODO

- [ ] Complete `protocol` with all 1.19.1 Minecraft packets 
- [x] Command dispatch system
- [ ] Viewable objects (entities)
- [ ] Allow multiple players to connect (and see each other)
- [ ] Allow chunk/world modification

# Subprojects

- `example_server`: example server using various Graphite components
- `server`: the minecraft server. entities, chunks, players, all that good stuff
- `concierge`: component that accepts new connections, handles status and login. Can be used to create proxies, servers, etc.
- `protocol`: the minecraft protocol
- `net`: networking components and utilities
- `binary`: zero-copy serialization
- `command`: command dispatch and low-level creation
- `command_derive`: attribute macro to easily create commands
- `sticky`: collection(s) that guarantee the memory-location of its contents

# Warning

Project is currently under heavy development, many things are completely non-functional

# How to run the example server

!!! Currently Graphite only supports modern versions of Linux with io_uring !!!  
(An alternative network backend will be available eventually, but is not a priority)  

```
$ cargo run --bin example_server
```
