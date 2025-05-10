This project implements a very simple HTTP server in Rust:
- Using `std` components for TCP i/o and thread control,
- And the `http` crate [ref: docs.rs](https://docs.rs/http/latest/http/) to define request and response types,
- With hand-written stream/byte parsing inbetween, as a "first Rust project" learning exercise.

The path-matching behavior in `handle_request_stream()` demonstrates how some straightforward HTTP request handling could be implemented on top of this pattern.

(But in practice, no, don't; this project's broadly-untested and unoptimized HTTP implementation shouldn't be used in a real application. Use a community-accepted library instead!)

**This was a learning exercise** and isn't intended for future enhancement or for re-use. The Rust code here *may* be some of the worst you'll ever see.

The author looks forward to re-reading this in the future, and laughing at how his past self awkwardly stumbled through it.

🍻
