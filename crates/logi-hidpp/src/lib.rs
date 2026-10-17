//! Logitech HID++ 2.0, the protocol of recent Logitech mice (G502 X…),
//! keyboards and receivers. Pure protocol: building requests and decoding
//! replies. Sending the bytes (hidapi on the receiver's vendor collection)
//! is the caller's job, so this crate has no OS dependency.
//!
//! A device exposes numbered *features* (adjustable DPI = 0x2201, onboard
//! profiles = 0x8100…). Each feature sits at a device-specific *index*, found
//! through the Root feature (always index 0). A request names the index, a
//! function number (0..15) and up to 16 parameter bytes:
//!
//! ```text
//! long report: 0x11 | device index | feature index | function << 4 | sw_id | 16 parameter bytes
//! ```
//!
//! The device answers with the same header (and its 16 result bytes), or an
//! error: feature index 0xFF (HID++ 2.0) or sub-id 0x8F (HID++ 1.0, e.g. "no
//! device at this index" from a receiver). Devices behind a receiver use
//! indices 1..=6; a device on its own USB cable uses 0xFF.

pub mod features;
pub mod onboard;

/// Short report: 7 bytes (report id included).
pub const REPORT_SHORT: u8 = 0x10;
/// Long report: 20 bytes (report id included).
pub const REPORT_LONG: u8 = 0x11;
pub const LONG_LEN: usize = 20;
pub const SHORT_LEN: usize = 7;

/// Device index of a device wired directly (no receiver).
pub const DIRECT: u8 = 0xFF;

/// Software id put in our requests (1..15): replies carry it back, which tells
/// them apart from those of another program talking to the same device.
pub const SW_ID: u8 = 0x0D;

/// A HID++ 2.0 request (always sent as a long report).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Request {
    pub device: u8,
    pub feature_index: u8,
    /// 0..=15.
    pub function: u8,
    pub params: [u8; 16],
    /// Software id (1..15) carried back by the reply; [`SW_ID`] unless changed.
    pub sw_id: u8,
}

impl Request {
    pub fn new(device: u8, feature_index: u8, function: u8, params: &[u8]) -> Self {
        let mut p = [0u8; 16];
        p[..params.len().min(16)].copy_from_slice(&params[..params.len().min(16)]);
        Self { device, feature_index, function: function & 0x0F, params: p, sw_id: SW_ID }
    }

    /// Same request with another software id (two programs, or tools, on one device).
    pub fn with_sw_id(mut self, sw_id: u8) -> Self {
        self.sw_id = sw_id & 0x0F;
        self
    }

    pub fn to_bytes(&self) -> [u8; LONG_LEN] {
        let mut b = [0u8; LONG_LEN];
        b[0] = REPORT_LONG;
        b[1] = self.device;
        b[2] = self.feature_index;
        b[3] = (self.function << 4) | (self.sw_id & 0x0F);
        b[4..].copy_from_slice(&self.params);
        b
    }

    /// Is `msg` the answer to this request (reply or error)?
    pub fn matches(&self, msg: &Message) -> bool {
        match *msg {
            Message::Reply { device, feature_index, function, sw_id, .. } => {
                device == self.device && feature_index == self.feature_index && function == self.function && sw_id == self.sw_id
            }
            Message::Error { device, feature_index, function, sw_id, .. } => {
                device == self.device && feature_index == self.feature_index && function == self.function && sw_id == self.sw_id
            }
            Message::Error10 { device, sub_id, .. } => device == self.device && sub_id == self.feature_index,
            Message::Other { .. } => false,
        }
    }
}

/// A report received from the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Message {
    /// Answer (or notification when `sw_id` is 0).
    Reply { device: u8, feature_index: u8, function: u8, sw_id: u8, data: [u8; 16] },
    /// HID++ 2.0 error.
    Error { device: u8, feature_index: u8, function: u8, sw_id: u8, code: Error20 },
    /// HID++ 1.0 error (receivers, or nothing paired at this index).
    Error10 { device: u8, sub_id: u8, code: u8 },
    /// Anything else (other report ids, HID++ 1.0 notifications…).
    Other { report_id: u8 },
}

impl Message {
    /// Decode a short or long report (`bytes[0]` = report id).
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let id = *bytes.first()?;
        if id != REPORT_SHORT && id != REPORT_LONG {
            return Some(Self::Other { report_id: id });
        }
        if bytes.len() < SHORT_LEN {
            return None;
        }
        let device = bytes[1];
        if bytes[2] == 0x8F {
            return Some(Self::Error10 { device, sub_id: bytes[3], code: bytes[5] });
        }
        if bytes[2] == 0xFF && id == REPORT_LONG && bytes.len() >= 6 {
            return Some(Self::Error {
                device,
                feature_index: bytes[3],
                function: bytes[4] >> 4,
                sw_id: bytes[4] & 0x0F,
                code: Error20::from(bytes[5]),
            });
        }
        let mut data = [0u8; 16];
        let n = (bytes.len() - 4).min(16);
        data[..n].copy_from_slice(&bytes[4..4 + n]);
        Some(Self::Reply { device, feature_index: bytes[2], function: bytes[3] >> 4, sw_id: bytes[3] & 0x0F, data })
    }
}

/// HID++ 2.0 error codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error20 {
    NoError,
    Unknown,
    InvalidArgument,
    OutOfRange,
    HardwareError,
    LogitechInternal,
    InvalidFeatureIndex,
    InvalidFunctionId,
    Busy,
    Unsupported,
    Other(u8),
}

impl From<u8> for Error20 {
    fn from(c: u8) -> Self {
        match c {
            0 => Self::NoError,
            1 => Self::Unknown,
            2 => Self::InvalidArgument,
            3 => Self::OutOfRange,
            4 => Self::HardwareError,
            5 => Self::LogitechInternal,
            6 => Self::InvalidFeatureIndex,
            7 => Self::InvalidFunctionId,
            8 => Self::Busy,
            9 => Self::Unsupported,
            o => Self::Other(o),
        }
    }
}

pub(crate) fn be16(b: &[u8], i: usize) -> u16 {
    u16::from_be_bytes([b[i], b[i + 1]])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_layout() {
        let r = Request::new(1, 0, 1, &[0, 0, 0x5A]);
        let b = r.to_bytes();
        assert_eq!(&b[..7], &[0x11, 0x01, 0x00, 0x1D, 0x00, 0x00, 0x5A]);
        assert_eq!(b.len(), 20);
    }

    #[test]
    fn replies_and_errors() {
        let req = Request::new(1, 0, 1, &[]);
        let mut reply = [0u8; 20];
        reply[..7].copy_from_slice(&[0x11, 0x01, 0x00, 0x1D, 0x04, 0x02, 0x5A]);
        let m = Message::parse(&reply).unwrap();
        assert!(req.matches(&m));
        let Message::Reply { data, .. } = m else { panic!() };
        assert_eq!(&data[..3], &[4, 2, 0x5A]);

        // another program's answer (other sw_id) is not ours
        reply[3] = 0x1A;
        assert!(!req.matches(&Message::parse(&reply).unwrap()));

        let err = [0x11, 0x01, 0xFF, 0x00, 0x1D, 0x07, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let m = Message::parse(&err).unwrap();
        assert_eq!(m, Message::Error { device: 1, feature_index: 0, function: 1, sw_id: SW_ID, code: Error20::InvalidFunctionId });
        assert!(req.matches(&m));

        // receiver: nothing paired at index 3
        let e10 = [0x10, 0x03, 0x8F, 0x00, 0x1D, 0x08, 0x00];
        let m = Message::parse(&e10).unwrap();
        assert_eq!(m, Message::Error10 { device: 3, sub_id: 0, code: 8 });
        assert!(Request::new(3, 0, 1, &[]).matches(&m));

        assert_eq!(Message::parse(&[0x02, 1, 2]), Some(Message::Other { report_id: 2 }));
    }
}
