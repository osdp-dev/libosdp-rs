//
// Copyright (c) 2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

use libosdp::{
    BuzzerCommand, Channel, ChannelError, Command, ControlPanelBuilder, Error, FileError,
    PdInfoBuilder,
};

struct Idle;

impl Channel for Idle {
    fn read(&mut self, _: &mut [u8]) -> Result<usize, ChannelError> {
        Err(ChannelError::WouldBlock)
    }
    fn write(&mut self, _: &[u8]) -> Result<(), ChannelError> {
        Ok(())
    }
}

#[test]
fn builder_reports_the_bad_argument() {
    assert!(matches!(
        PdInfoBuilder::new().address(127),
        Err(Error::InvalidArgument(_))
    ));
    assert!(matches!(
        PdInfoBuilder::new().baud_rate(1200),
        Err(Error::InvalidArgument(_))
    ));
}

#[test]
fn file_status_outside_a_transfer_is_not_in_progress() -> Result<(), Error> {
    let mut builder = ControlPanelBuilder::new(Box::new(Idle));
    let pd = builder.add_pd(PdInfoBuilder::new().address(101)?.baud_rate(115200)?)?;
    let cp = builder.build()?;
    assert_eq!(cp.file_transfer_status(pd), Err(Error::NotInProgress));
    Ok(())
}

#[test]
fn channel_errors_convert_into_error() {
    assert_eq!(
        Error::from(ChannelError::WouldBlock),
        Error::Channel(ChannelError::WouldBlock)
    );
}

#[test]
fn io_errors_convert_into_file_errors() {
    let missing = std::io::Error::from(std::io::ErrorKind::NotFound);
    assert_eq!(FileError::from(missing), FileError::NotFound);
    let other = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
    assert_eq!(FileError::from(other), FileError::Io);
}

fn pd_at(address: i32) -> Result<PdInfoBuilder, Error> {
    PdInfoBuilder::new().address(address)?.baud_rate(115200)
}

#[test]
fn builder_hands_out_indexes_in_the_order_pds_were_added() -> Result<(), Error> {
    let mut builder = ControlPanelBuilder::new(Box::new(Idle));
    let first = builder.add_pd(pd_at(101)?)?;
    let second = builder.add_pd(pd_at(102)?)?;
    assert_eq!(usize::from(first), 0);
    assert_eq!(usize::from(second), 1);
    let cp = builder.build()?;
    assert!(!cp.is_online(second));
    Ok(())
}

#[test]
fn builder_refuses_more_pds_than_a_bus_can_address() -> Result<(), Error> {
    let mut builder = ControlPanelBuilder::new(Box::new(Idle));
    for address in 0..126 {
        builder.add_pd(pd_at(address)?)?;
    }
    assert!(matches!(
        builder.add_pd(pd_at(126)?),
        Err(Error::InvalidArgument(_))
    ));
    Ok(())
}

#[test]
fn submit_to_an_offline_pd_is_rejected() -> Result<(), Error> {
    let mut builder = ControlPanelBuilder::new(Box::new(Idle));
    let pd = builder.add_pd(pd_at(101)?)?;
    let cp = builder.build()?;
    let buzz = Command::Buzzer(BuzzerCommand {
        reader: 0,
        control_code: 1,
        on_count: 0,
        off_count: 0,
        rep_count: 0,
    });
    assert!(matches!(cp.submit(pd, buzz), Err(Error::Rejected)));
    Ok(())
}
