# LibOSDP-rs

[![Build CI][7]][8]
[![Crates.io libosdp-sys version][3]][4]
[![Crates.io libosdp version][5]][6]
[![Crates.io osdpctl version][9]][10]

This project hosts cargo crates that add rust support for [LibOSDP][1] in rust
and other OSDP related tools such as osdpctl. You can also take a look at the
[documentation][2] for more details.

## Project structure:

- `libosdp-sys` - Low level rust `-sys` crate for the C library.
- `libosdp` - Safe wrapper around `libosdp-sys` to be consumed by rust projects.
- `osdpctl` - A tool to create and manage OSDP devices.
- `scripts` - Tools for developers working on this project.

[1]: https://github.com/osdp-dev/libosdp
[2]: https://doc.osdp.dev/
[3]: https://img.shields.io/crates/v/libosdp-sys?style=flat&logo=rust&logoColor=DDD&label=crate%20%3A%20libosdp-sys&link=https%3A%2F%2Fcrates.io%2Fcrates%2Flibosdp-sys
[4]: https://crates.io/crates/libosdp-sys
[5]: https://img.shields.io/crates/v/libosdp?style=flat&logo=rust&logoColor=DDD&label=crate%20%3A%20libosdp&link=https%3A%2F%2Fcrates.io%2Fcrates%2Flibosdp
[6]: https://crates.io/crates/libosdp
[7]: https://github.com/osdp-dev/libosdp-rs/actions/workflows/build-ci.yml/badge.svg
[8]: https://github.com/osdp-dev/libosdp-rs/actions/workflows/build-ci.yml
[9]: https://img.shields.io/crates/v/osdpctl?style=flat&logo=rust&logoColor=DDD&label=crate%20%3A%20osdpctl&link=https%3A%2F%2Fcrates.io%2Fcrates%2Fosdpctl
[10]: https://crates.io/crates/osdpctl
