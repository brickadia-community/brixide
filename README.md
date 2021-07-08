# brixide

Brixide is a very experimental server wrapper for the game [Brickadia](https://brickadia.com/).
It supports plugins over JSON RPC.

## Installation

Install [Rust](https://rust-lang.org/), then clone this repository.

You can run the server with `cargo run -p server`.

## Plugins

Plugins work over JSON RPC. For reference, see `ping_pong_plugin` under the base `plugins` in
this repository.

*TODO: define the RPC spec and `plugin.toml` spec here*

## Credits

* voximity - creator/maintainer
* [Meshiest](https://github.com/Meshiest) - [Omegga](https://github.com/brickadia-community/omegga), huge inspiration and borrowed logic/Regex/tools/etc.
