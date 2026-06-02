use std::io::{self, Write};
use std::marker::Destruct;

use arbitrary_int::prelude::*;
use bitbybit::{bitfield, *};

use crate::Encode;

#[bitfield(u8, debug)]
pub struct Bcd {
    /// the least significant digit
    #[bits(0..=3, rw)]
    digit_01: u4,
    /// the most significant digit
    #[bits(4..=7, rw)]
    digit_02: u4,
}

impl Bcd {
    pub const fn unpack(self) -> u8 {
        self.digit_02().value() * 10 + self.digit_01().value()
    }
}

impl From<Bcd> for u8 {
    fn from(val: Bcd) -> Self {
        val.unpack()
    }
}

impl From<u8> for Bcd {
    fn from(value: u8) -> Self {
        Self::new_with_raw_value(value)
    }
}

#[derive(Default, Debug, Clone, Copy)]
pub struct CdromCursor {
    pub lba:  u32,
    pub byte: u32,
}

/// (minute, second, sector) tuple
#[derive(Debug, Clone, Copy, derive_more::Display)]
#[display("{min:02}:{sec:02}:{sect:02}")]
pub struct Mss<T> {
    pub min:  T,
    pub sec:  T,
    pub sect: T,
}

impl<T> Mss<T> {
    pub fn new(min: T, sec: T, sect: T) -> Self {
        Self { min, sec, sect }
    }
}

pub const SECTOR_USER_SIZE: usize = 0x930;

impl CdromCursor {
    pub fn from_mss<T: Into<u8> + Destruct>(mss: Mss<T>) -> Self {
        let min = mss.min.into() as u32;
        let sec = mss.sec.into() as u32;
        let sect = mss.sect.into() as u32;
        Self {
            lba:  (min * (60 * 75) + sec * 75 + sect).saturating_sub(150),
            byte: 0,
        }
    }

    pub fn to_mss<T: From<u8>>(self) -> Mss<T> {
        let lba = self.lba + 150;

        let sect = (lba % 75) as u8;
        let sec = (lba / 75 % 60) as u8;
        let min = (lba / 75 / 60) as u8;

        Mss {
            min:  T::from(min),
            sec:  T::from(sec),
            sect: T::from(sect),
        }
    }

    pub const fn to_byte(self) -> u32 {
        self.lba * SECTOR_USER_SIZE as u32 + self.byte
    }

    pub const fn advance_by(&mut self, mut by_bytes: u32, sect_size: SectSize) {
        let (pad, end) = match sect_size {
            SectSize::DataOnly0x800 => (0x18, 0x18 + 0x800),
            SectSize::Whole0x924 => (0x0c, SECTOR_USER_SIZE),
        };
        let mut to_end = end as u32 - self.byte;
        while by_bytes >= to_end {
            by_bytes -= to_end;
            self.byte = pad;
            self.lba += 1;
            to_end = end as u32;
        }
        self.byte += by_bytes;
    }
}

#[bitenum(u1, exhaustive = true)]
#[derive(Debug)]
pub enum SectSize {
    DataOnly0x800 = 0x0,
    Whole0x924    = 0x1,
}

type Todo = ();

/// ```plaintext
///  000h 0Ch  Sync   (00h,FFh,FFh,FFh,FFh,FFh,FFh,FFh,FFh,FFh,FFh,00h)
///  00Ch 4    Header (Minute,Second,Sector,Mode=02h)
///  010h 4    Sub-Header (File, Channel, Submode AND DFh, Codinginfo)
///  014h 4    Copy of Sub-Header
///  018h 800h Data (2048 bytes)
///  818h 4    EDC (checksum across [010h..817h])
///  81Ch 114h ECC (error correction codes)
/// ```
pub struct Form1 {
    header:    Header,
    subheader: Subheader,
    data:      Todo,
    edc:       Todo,
    ecc:       Todo,
}

/// `4 Header (Minute,Second,Sector,Mode)`
struct Header {
    mss:  Mss<Bcd>,
    mode: u8,
}

/// `4 Sub-Header (File, Channel, Submode AND DFh, Codinginfo)`
struct Subheader {
    file:    FileNumber,
    channel: ChannelNumber,
    submode: Submode,
    cinfo:   Todo,
}

/// # 1st Subheader byte - File Number (FN)
struct FileNumber(u8);
/// # 2nd Subheader byte - Channel Number (CN)
struct ChannelNumber(u4);

/// # 3rd Subheader byte - Submode (SM)
///
/// ```plaintext
/// 0   End of Record (EOR) (all Volume Descriptors, and all sectors with EOF)
/// 1   Video     ;\Sector Type (usually ONE of these bits should be set)
/// 2   Audio     ; Note: PSX .STR files are declared as Data (not as Video)
/// 3   Data      ;/
/// 4   Trigger           (for application use)
/// 5   Form2             (0=Form1/800h-byte data, 1=Form2, 914h-byte data)
/// 6   Real Time (RT)
/// 7   End of File (EOF) (or end of Directory/PathTable/VolumeTerminator)
/// ```
#[bitfield(u8)]
struct Submode {
    #[bit(0, rw)]
    eor:     bool,
    #[bit(1, rw)]
    video:   bool,
    #[bit(2, rw)]
    audio:   bool,
    #[bit(3, rw)]
    data:    bool,
    #[bit(4, rw)]
    trigger: bool,
    #[bit(5, rw)]
    form2:   bool,
    #[bit(6, rw)]
    rt:      bool,
    #[bit(7, rw)]
    eof:     bool,
}

/// # 4th Subheader byte - Codinginfo (CI)
///
/// When used for Data sectors:
/// ```plaintext
///   0-7 Reserved (00h)
/// ```
/// When used for XA-ADPCM audio sectors:
/// ```plaintext
///   0-1 Mono/Stereo     (0=Mono, 1=Stereo, 2-3=Reserved)
///   2-2 Sample Rate     (0=37800Hz, 1=18900Hz, 2-3=Reserved)
///   4-5 Bits per Sample (0=Normal/4bit, 1=8bit, 2-3=Reserved)
///   6   Emphasis        (0=Normal/Off, 1=Emphasis)
///   7   Reserved        (0)
/// ```
#[bitfield(u8)]
struct CodingInfo {
    #[bits(0..=1, rw)]
    mono_stereo: u2,
    #[bits(0..=1)]
    sample_rate: XAADPCMSampleRate,
    // TODO: the rest of the fields
}

#[bitenum(u2, exhaustive = true)]
enum XAADPCMSampleRate {
    Full37800Hz = 0x0,
    Half18900Hz = 0x1,
    Reserved01  = 0x2,
    Reserved02  = 0x3,
}

/// # System Area (prior to Volume Descriptors)
///
/// ```plaintext
/// The first 16 sectors on the first track are the system area, for a Playstation disk, it contains the following:
///
///   Sector 0..3   - Zerofilled (Mode2/Form1, 4x800h bytes, plus ECC/EDC)
///   Sector 4      - Licence String
///   Sector 5..11  - Playstation Logo (3278h bytes) (remaining bytes FFh-filled)
///   Sector 12..15 - Zerofilled (Mode2/Form2, 4x914h bytes, plus EDC)
/// ```
///
/// Of which, the Licence String in sector 4 is,
///
/// ```plaintext
///   000h 32    Line 1      ("          Licensed  by          ")
///   020h 32+6  Line 2 (EU) ("Sony Computer Entertainment Euro"," pe   ") ;\either
///   020h 32+1  Line 2 (JP) ("Sony Computer Entertainment Inc.",0Ah)      ; one of
///   020h 32+6  Line 2 (US) ("Sony Computer Entertainment Amer","  ica ") ;/these
///   041h 1983  Empty (JP)    (filled by repeating pattern 62x30h,1x0Ah, 1x30h)
///   046h 1978  Empty (EU/US) (filled by 00h-bytes)
/// ```
struct PsxSystemArea {}

struct Fill {
    pattern:     &'static [u8],
    total_bytes: usize,
}

impl Fill {
    fn from_sectors() {}
}

impl Encode for Fill {
    fn encode<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        let mut written = 0;
        while written < self.total_bytes {
            let remaining = self.total_bytes - written;
            let chunk = &self.pattern[..self.pattern.len().min(remaining)];
            writer.write_all(chunk)?;
            written += chunk.len();
        }
        Ok(())
    }
}
