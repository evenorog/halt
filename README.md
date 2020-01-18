# halt

[![Travis](https://api.travis-ci.com/evenorog/halt.svg?branch=master)](https://travis-ci.com/evenorog/halt)
[![Crates.io](https://img.shields.io/crates/v/halt.svg)](https://crates.io/crates/halt)
[![Docs](https://docs.rs/halt/badge.svg)](https://docs.rs/halt)

Provides functionality for pausing, stopping, and resuming iterators, readers, and writers.

## Examples

Add this to `Cargo.toml`:

```toml
[dependencies]
halt = "1"
```

And this to `main.rs`:

```rust
use std::{io, thread, time::Duration};

fn main() {
    let mut halt = halt::new(io::repeat(0));
    let remote = halt.remote();
    thread::spawn(move || io::copy(&mut halt, &mut io::sink()).unwrap());
    thread::sleep(Duration::from_secs(5));
    remote.pause();
    thread::sleep(Duration::from_secs(5));
    remote.resume();
    thread::sleep(Duration::from_secs(5));
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
