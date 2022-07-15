# Graphite

1.19.1 Minecraft Server

# TODO

- [ ] Complete `protocol` with all 1.19.1 Minecraft packets 
- [ ] Command dispatch system
- [ ] Viewable objects (entities)
- [ ] Allow multiple players to connect (and see each other)
- [ ] Allow chunk/world modification

# Subprojects

- `binary`: zero-copy serialization
- `concierge`: component that accepts new connections, handles status and login. Can be used to create proxies, servers, etc.
- `example_server`: example server using various Graphite components
- `net`: networking components and utilities
- `protocol`: the minecraft protocol
- `server`: the minecraft server. entities, chunks, players, all that good stuff
- `sticky`: collection(s) that guarantee the memory-location of its contents

# Warning

Project is currently under heavy development, many things are completely non-functional

# How to run the example server

!!! Currently Graphite only supports modern versions of Linux with io_uring !!!  
(An alternative network backend will be available eventually, but is not a priority)  

```
$ cargo run --bin example_server
```
