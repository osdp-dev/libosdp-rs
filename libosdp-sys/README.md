# libosdp-sys

This crate hosts bindgen generated `-sys` bindings to [osdp-dev/libosdp][1]. It
tracks LibOSDP releases and uses the same version numbers to make it easy to
determine the underlying LibOSDP version.

This crate is not intended to be directly consumed. Please take a look at
[libosdp][2] (see doc [here][3]) if you intend to use LibOSDP in your project.

## Maintainer notes

`src/bindings.rs` is checked into git and used for normal builds.

When bumping `vendor` (LibOSDP submodule), regenerate bindings and include the
updated file in the same commit:

```sh
CCACHE_DISABLE=1 LIBOSDP_SYS_REGENERATE_BINDINGS=1 cargo build -p libosdp-sys
```

[1]: https://github.com/osdp-dev/libosdp
[2]: https://crates.io/crates/libosdp
[3]: https://docs.rs/libosdp
