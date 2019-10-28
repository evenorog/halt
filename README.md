![nyx](https://raw.githubusercontent.com/evenorog/halt/master/halt.svg?sanitize=true)

[![Travis](https://api.travis-ci.com/evenorog/halt.svg?branch=master)](https://travis-ci.com/evenorog/halt)
[![Crates.io](https://img.shields.io/crates/v/halt.svg)](https://crates.io/crates/halt)
[![Docs](https://docs.rs/halt/badge.svg)](https://docs.rs/halt)

Provides functionality for pausing, stopping, and resuming iterators, readers, and writers.

## Examples

Add this to `Cargo.toml`:

```toml
[dependencies]
halt = "0.2"
```

And this to `main.rs`:

```rust
use halt::Halt;
use std::{io, thread};

fn main() {
    // Wrap a reader in the halt structure.
    let mut halt = Halt::new(io::repeat(0));
    // Get a remote to the reader.
    let remote = halt.remote();
    // Copy forever into a sink, in a separate thread.
    thread::spawn(move || io::copy(&mut halt, &mut io::sink()).unwrap());
    // The remote can now be used to either pause, stop, or resume the reader from the main thread.
    remote.pause().unwrap();
    remote.resume().unwrap();
}
```

### License

Licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.
