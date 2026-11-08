//! HDMI-CEC protocol driver and erased handle.

pub mod client;
pub mod driver;
pub mod error;
pub mod handle;
#[cfg(feature = "cec-native")]
pub mod native;
pub mod opcode;
pub mod transport;
pub mod types;

pub use client::{active_source_frame, check, CecClient};
pub use driver::CecDriver;
pub use error::CecError;
pub use handle::CecDeviceHandle;
#[cfg(feature = "cec-native")]
pub use native::NativeTransport;
pub use transport::{CecTransport, ClientTransport, PersistentClientTransport};
pub use types::{CecAddress, CecCommand};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cec_address_conversions() {
        assert_eq!(CecAddress::try_from(0).unwrap(), CecAddress::Tv);
        assert_eq!(CecAddress::try_from(4).unwrap(), CecAddress::Playback1);
        assert_eq!(CecAddress::try_from(8).unwrap(), CecAddress::Playback2);
        assert_eq!(CecAddress::try_from(5).unwrap(), CecAddress::AudioSystem);
        assert_eq!(CecAddress::try_from(15).unwrap(), CecAddress::Broadcast);
        assert!(CecAddress::try_from(99).is_err());

        assert_eq!(CecAddress::try_from("0").unwrap(), CecAddress::Tv);
        assert_eq!(CecAddress::try_from("tv").unwrap(), CecAddress::Tv);
        assert_eq!(
            CecAddress::try_from("playback1").unwrap(),
            CecAddress::Playback1
        );
        assert_eq!(
            CecAddress::try_from("playback_2").unwrap(),
            CecAddress::Playback2
        );
        assert_eq!(
            CecAddress::try_from("audio").unwrap(),
            CecAddress::AudioSystem
        );
        assert_eq!(
            CecAddress::try_from("broadcast").unwrap(),
            CecAddress::Broadcast
        );
        assert!(CecAddress::try_from("invalid_addr").is_err());
    }

    #[test]
    fn cec_command_display_and_parsing() {
        let cmd_on = CecCommand::try_from("on tv").unwrap();
        assert_eq!(cmd_on, CecCommand::PowerOn(CecAddress::Tv));
        assert_eq!(cmd_on.to_string(), "on 0");

        let cmd_st = CecCommand::try_from("standby 4").unwrap();
        assert_eq!(cmd_st, CecCommand::Standby(CecAddress::Playback1));
        assert_eq!(cmd_st.to_string(), "standby 4");

        let cmd_as = CecCommand::try_from("as 0x1000").unwrap();
        assert_eq!(cmd_as, CecCommand::SetActiveSource(0x1000));
        assert_eq!(cmd_as.to_string(), "tx 0F:82:10:00");

        let cmd_as_frame = CecCommand::try_from("tx 0F:82:20:00").unwrap();
        assert_eq!(cmd_as_frame, CecCommand::SetActiveSource(0x2000));
        assert_eq!(cmd_as_frame.to_string(), "tx 0F:82:20:00");

        let cmd_custom = CecCommand::try_from("tx 10:44:41").unwrap();
        assert_eq!(
            cmd_custom,
            CecCommand::CustomHexFrame("tx 10:44:41".to_string())
        );
        assert_eq!(cmd_custom.to_string(), "tx 10:44:41");

        let cmd_raw = CecCommand::try_from("volup").unwrap();
        assert_eq!(cmd_raw, CecCommand::Raw("volup".to_string()));
        assert_eq!(cmd_raw.to_string(), "volup");
    }

    #[test]
    fn cec_command_grammar_validation() {
        assert!(CecCommand::try_from("on tv\nstandby tv").is_err());
        assert!(CecCommand::try_from("on tv\0").is_err());
        assert!(CecCommand::try_from("").is_err());
        assert!(CecCommand::try_from("   ").is_err());

        let invalid_raw = CecCommand::Raw("bad\ncommand".to_string());
        assert!(invalid_raw.validate().is_err());
    }
}
