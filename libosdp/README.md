# LibOSDP for Rust

This crate provides safe wrappers for accessing the C library [LibOSDP][1].
LibOSDP the most popular open source library for creating Open Supervised Device
Protocol (OSDP) devices. For more information about the library or OSDP in
general, see [doc.osdp.dev][4].

The documentation for this crate can be found [here][5].

## Features

  - Production ready implementation with active users and contributors
  - Supports secure channel communication (AES-128) ([see][6])
  - Can be used to setup a PD or CP mode of operation
  - Most OSDP specified commands and replies are supported (see [doc][3])

## Usage

To add libosdp to your rust project, do:

```
cargo add libosdp
```

### Control Panel:

A simplified CP implementation:

```rust
let mut builder = ControlPanelBuilder::new(Box::new(channel));
builder.add_pd(PdInfoBuilder::new().address(101)?.baud_rate(115200)?)?;
let cp = builder.build()?;
loop {
    cp.refresh();
    while let Some((from, event)) = cp.next_event() {
        println!("Received event from {from}: {event:?}");
        if let CpEvent::Event(Event::CardRead(_)) = event {
            cp.submit(from, Command::Buzzer(...))?;
        }
    }
}
```

See [examples][2] for a working implementation.

### Peripheral Device:

A simplified PD implementation:

```rust
let pd_info = PdInfo::new(...);
let mut pd = PeripheralDevice::new(&mut pd_info)?;
pd.set_command_callback(|cmd| {
    println!("Received command {:?}", cmd);
});
loop {
    pd.refresh();
    pd.submit(Event::new(...));
}
```

See [examples][2] for a working implementation.

[1]: https://github.com/osdp-dev/libosdp
[2]: https://github.com/osdp-dev/libosdp-rs/tree/master/libosdp/examples
[3]: https://doc.osdp.dev/protocol/commands-and-replies
[4]: https://doc.osdp.dev/
[5]: https://docs.rs/libosdp
[6]: https://doc.osdp.dev/libosdp/secure-channel