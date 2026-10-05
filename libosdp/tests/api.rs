//
// Copyright (c) 2026 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

use libosdp::{Channel, ChannelError, ControlPanelBuilder, Error, FileError, PdInfoBuilder};

struct Idle;

impl Channel for Idle {
    fn get_id(&self) -> i32 {
        0
    }
    fn read(&mut self, _: &mut [u8]) -> Result<usize, ChannelError> {
        Err(ChannelError::WouldBlock)
    }
    fn write(&mut self, buf: &[u8]) -> Result<usize, ChannelError> {
        Ok(buf.len())
    }
    fn flush(&mut self) -> Result<(), ChannelError> {
        Ok(())
    }
    fn close(&mut self) -> Result<(), ChannelError> {
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
    let cp = ControlPanelBuilder::new(Box::new(Idle))
        .add_pd(PdInfoBuilder::new().address(101)?.baud_rate(115200)?)
        .build()?;
    assert_eq!(cp.file_transfer_status(0), Err(Error::NotInProgress));
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
