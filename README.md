# Graphite -- CURRENTLY ON HIATUS

1.19.1 Minecraft Server

# TODO

- [ ] Complete `protocol` with all 1.19.1 Minecraft packets 
- [x] Command dispatch system
- [x] Viewable objects (entities)
- [x] Allow multiple players to connect (and see each other)
- [x] Base lib crate that reexports everything
- [x] ItemStacks with NBT
- [ ] Add layer for modifying the NBT of items easily
- [x] Player Input Handling (Left/Right Click Air/Block)
- [ ] TextComponent things & macro
- [x] Allow chunk/world modification
- [ ] Complete missing block placement (walls, double blocks, candles, etc.)
- [ ] "Extras" subproject - raycasting, collision, ...

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

# Building

Make sure to run `git submodule update --init --recursive` before building/running

# How to run the example server

!!! Currently Graphite only supports modern versions of Linux with io\_uring !!!  
(An alternative network backend will be available eventually, but is not a priority)  

```
$ cargo run --bin example_server
```
