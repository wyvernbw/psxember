pub mod fs;
#[cfg(test)]
mod tests;
pub mod vol_desc;

use std::io::{Cursor, Seek, Write};
use std::marker::Destruct;

use arbitrary_int::prelude::*;
use bitbybit::{bitfield, *};
use miette::{Context, IntoDiagnostic, miette};

use crate::encoders::{Encode, EncodeError, Fill};
use crate::iso9660::vol_desc::PrimaryVolumeDescriptor;

#[bitfield(u8, debug)]
pub struct Bcd {
    /// the least significant digit
    #[bits(0..=3, rw)]
    digit_01: u4,
    /// the most significant digit
    #[bits(4..=7, rw)]
    digit_02: u4,
}

#[derive(derive_more::DerefMut, derive_more::Deref)]
pub struct Lba(u64);

pub trait DiscWrite: Write + Seek {
    fn lba(&mut self) -> crate::Result<Lba> {
        let pos = self.stream_position().into_diagnostic()?;
        let lba = pos / SECTOR_RAW_SIZE as u64;
        Ok(Lba(lba))
    }
    fn address(&mut self) -> crate::Result<Mss<Bcd>> {
        self.lba().map(|lba| lba.to_mss())
    }
}

impl<T: Write + Seek> DiscWrite for T {}

impl Lba {
    pub fn to_mss<T: From<u8>>(self) -> Mss<T> {
        let lba = self.0 + 150;

        let sect = (lba % 75) as u8;
        let sec = (lba / 75 % 60) as u8;
        let min = (lba / 75 / 60) as u8;

        Mss {
            min:  T::from(min),
            sec:  T::from(sec),
            sect: T::from(sect),
        }
    }
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
        let digit_01 = value % 10;
        let digit_02 = value / 10 % 10;
        Bcd::builder()
            .with_digit_01(digit_01.as_())
            .with_digit_02(digit_02.as_())
            .build()
    }
}

#[derive(Default, Debug, Clone, Copy)]
pub struct CdromCursor {
    /// FIXME: use [`Lba`]
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

pub const SECTOR_RAW_SIZE: usize = 0x930;

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
        self.lba * SECTOR_RAW_SIZE as u32 + self.byte
    }

    pub const fn advance_by(&mut self, mut by_bytes: u32, sect_size: SectSize) {
        let (pad, end) = match sect_size {
            SectSize::DataOnly0x800 => (0x18, 0x18 + 0x800),
            SectSize::Whole0x924 => (0x0c, SECTOR_RAW_SIZE),
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

impl Encode for Todo {
    fn encode<W: ?Sized + DiscWrite>(&self, writer: &mut W) -> Result<(), EncodeError> {
        // todo!()
        Ok(())
    }
}

/// ```plaintext
///  000h 0Ch  Sync   (00h,FFh,FFh,FFh,FFh,FFh,FFh,FFh,FFh,FFh,FFh,00h)
///  00Ch 4    Header (Minute,Second,Sector,Mode=02h)
///  010h 4    Sub-Header (File, Channel, Submode AND DFh, Codinginfo)
///  014h 4    Copy of Sub-Header
///  018h 800h Data (2048 bytes)
///  818h 4    EDC (checksum across [010h..817h])
///  81Ch 114h ECC (error correction codes)
/// ```
pub struct Form1Sector<T> {
    sync:      SyncHeader,
    header:    Header,
    subheader: Subheader,
    data:      T,
    edc:       u32,
    ecc:       Todo,
}

struct SyncHeader;

/// `4 Header (Minute,Second,Sector,Mode)`
struct Header {
    mss:  Mss<Bcd>,
    mode: HeaderMode,
}

#[derive(Debug, Clone, Copy)]
enum HeaderMode {
    Mode2 = 0x02,
}

/// `4 Sub-Header (File, Channel, Submode AND DFh, Codinginfo)`
struct Subheader {
    file:    FileNumber,
    channel: ChannelNumber,
    submode: Submode,
    cinfo:   CodingInfo,
}

/// # 1st Subheader byte - File Number (FN)
struct FileNumber(u8);
/// # 2nd Subheader byte - Channel Number (CN)
struct ChannelNumber(u8);

macro_rules! impl_encode_newtype {
    ($type:ty) => {
        impl Encode for $type {
            fn encode<W: ?Sized + DiscWrite>(&self, writer: &mut W) -> Result<(), EncodeError> {
                self.0.encode(writer)
            }
        }
    };
}

impl_encode_newtype!(FileNumber);
impl_encode_newtype!(ChannelNumber);

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
pub struct Submode {
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
#[bitfield(u8, default = 0x00)]
struct CodingInfo {
    #[bits(0..=1, rw)]
    mono_stereo: u2,
    #[bits(2..=3)]
    sample_rate: XAADPCMSampleRate,
    // TODO: the rest of the fields
}

impl Encode for CodingInfo {
    fn encode<W: DiscWrite + ?Sized>(&self, writer: &mut W) -> Result<(), EncodeError> {
        self.raw_value().encode(writer)
    }
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
struct PsxSystemArea {
    /// see [`LicenseString`]
    license_string: LicenseString,
}

/// # System Area License String
///
/// ```plaintext
///   000h 32    Line 1      ("          Licensed  by          ")
///   020h 32+6  Line 2 (EU) ("Sony Computer Entertainment Euro"," pe   ") ;\either
///   020h 32+1  Line 2 (JP) ("Sony Computer Entertainment Inc.",0Ah)      ; one of
///   020h 32+6  Line 2 (US) ("Sony Computer Entertainment Amer","  ica ") ;/these
///   041h 1983  Empty (JP)    (filled by repeating pattern 62x30h,1x0Ah, 1x30h)
///   046h 1978  Empty (EU/US) (filled by 00h-bytes)
/// ```
#[derive(Debug, Clone, Copy)]
enum LicenseString {
    EU,
    JP,
    US,
}

impl Encode for SyncHeader {
    fn encode<W: ?Sized + DiscWrite>(&self, writer: &mut W) -> Result<(), EncodeError> {
        let pattern = &[
            0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00,
        ];
        let fill = Fill {
            pattern,
            total_bytes: 0x0c,
        };
        fill.encode(writer)?;
        Ok(())
    }
}

impl<T: Encode> Encode for Mss<T> {
    fn encode<W: ?Sized + DiscWrite>(&self, writer: &mut W) -> Result<(), EncodeError> {
        self.min.encode(writer)?;
        self.sec.encode(writer)?;
        self.sect.encode(writer)?;
        Ok(())
    }
}

impl Encode for Bcd {
    fn encode<W: ?Sized + DiscWrite>(&self, writer: &mut W) -> Result<(), EncodeError> {
        self.raw_value().encode(writer)
    }
}

impl Encode for HeaderMode {
    fn encode<W: ?Sized + DiscWrite>(&self, writer: &mut W) -> Result<(), EncodeError> {
        let mode = *self as u8;
        mode.encode(writer)
    }
}

impl Encode for Header {
    fn encode<W: ?Sized + DiscWrite>(&self, writer: &mut W) -> Result<(), EncodeError> {
        self.mss.encode(writer)?;
        self.mode.encode(writer)?;
        Ok(())
    }
}

impl Encode for Submode {
    fn encode<W: ?Sized + DiscWrite>(&self, writer: &mut W) -> Result<(), EncodeError> {
        self.raw_value().encode(writer)
    }
}

impl Encode for Subheader {
    fn encode<W: ?Sized + DiscWrite>(&self, writer: &mut W) -> Result<(), EncodeError> {
        self.file.encode(writer)?;
        self.channel.encode(writer)?;
        self.submode.encode(writer)?;
        self.cinfo.encode(writer)?;
        Ok(())
    }
}

static EDC_TABLE: [u32; 256] = const {
    //  for i=0 to FFh
    //    x=i, for j=0 to 7, x=x shr 1, if carry then x=x xor D8018001h
    //    edc_table[i]=x
    //  GF8_LOG[00h]=00h, GF8_ILOG[FFh]=00h, x=01h
    //  for i=00h to FEh
    //    GF8_LOG[x]=i, GF8_ILOG[i]=x
    //    x=x SHL 1, if carry8bit then x=x xor 1dh
    //  for j=0 to 42
    //    xx=GF8_ILOG[44-j],  yy=subfunc(xx xor 1,19h)
    //    xx=subfunc(xx,01h), xx=subfunc(xx xor 1,18h)
    //    xx=GF8_LOG[xx], yy = GF8_LOG[yy]
    //    GF8_PRODUCT[j,0]=0000h
    //    for i=01h to FFh
    //      x=xx+GF8_LOG[i], if x>=255 then x=x-255
    //      y=yy+GF8_LOG[i], if y>=255 then y=y-255
    //      GF8_PRODUCT[j,i]=GF8_ILOG[x]+(GF8_ILOG[y] shl 8)

    let mut edc_table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut x = i as u32;
        let mut j = 0;
        while j < 8 {
            let carry = (x & 1) == 0;
            x >>= 1;
            if carry {
                x ^= 0xd8018001
            }
            j += 1;
        }
        edc_table[i] = x;
        i += 1
    }

    edc_table
};

fn edc_checksum(bytes: &[u8]) -> u32 {
    // x=00000000h
    // for i=0 to len-1
    //   x=x xor byte[addr+i], x=(x shr 8) xor edc_table[x and FFh]
    // word[addr+len]=x  ;append EDC value (little endian)
    let mut edc = 0u32;
    for byte in bytes.iter() {
        edc ^= *byte as u32;
        edc <<= 8;
        edc ^= EDC_TABLE[(edc & 0xff) as usize];
    }

    edc
}

impl<T: Encode> Encode for Form1Sector<T> {
    fn encode<W: ?Sized + DiscWrite>(&self, writer: &mut W) -> Result<(), EncodeError> {
        self.sync.encode(writer)?;
        self.header.encode(writer)?;

        let mut temp_buf = [0u8; 0x800 + 0x4 + 0x4];
        let mut temp_buf = Cursor::new(temp_buf.as_mut_slice());
        self.subheader.encode(&mut temp_buf)?;
        self.subheader.encode(&mut temp_buf)?; // subheader copy

        match self.data.encode(&mut temp_buf) {
            Ok(_) => {}
            Err(EncodeError::IO(err)) if err.kind() == std::io::ErrorKind::WriteZero => {
                tracing::warn!(
                    "data in form1 sector was truncated to 0x800 bytes (possible data loss)"
                );
            }
            err => return err,
        };

        let temp_buf = temp_buf.into_inner();
        // commit data from temporary buffer
        temp_buf.encode(writer)?;

        let edc = edc_checksum(temp_buf);
        edc.encode(writer)?;

        // self.ecc.encode(writer)?;
        Fill::zero(0x114).encode(writer)?;
        Ok(())
    }
}

impl<T: Encode> Form1Sector<T> {
    #[must_use]
    pub fn new(data: T, mss: Mss<Bcd>, submode: Submode) -> Self {
        let subheader = Subheader {
            file: FileNumber(0),
            channel: ChannelNumber(0),
            submode,
            cinfo: CodingInfo::default(),
        };
        let header = Header {
            mss,
            mode: HeaderMode::Mode2,
        };

        Self {
            sync: SyncHeader,
            header,
            subheader,
            data,
            edc: 0,
            ecc: (),
        }
    }
}

impl<T> Form2Sector<T> {
    #[must_use]
    pub fn new(data: T, mss: Mss<Bcd>, submode: Submode) -> Self {
        Self {
            sync: SyncHeader,
            header: Header {
                mss,
                mode: HeaderMode::Mode2,
            },
            subheader: Subheader {
                file: FileNumber(0),
                channel: ChannelNumber(0),
                submode,
                cinfo: CodingInfo::default(),
            },
            data,
            edc: (),
        }
    }
}
/// ```plaintext
/// Mode2/Form2 (CD-XA)
///
///   000h 0Ch  Sync   (00h,FFh,FFh,FFh,FFh,FFh,FFh,FFh,FFh,FFh,FFh,00h)
///   00Ch 4    Header (Minute,Second,Sector,Mode=02h)
///   010h 4    Sub-Header (File, Channel, Submode OR 20h, Codinginfo)
///   014h 4    Copy of Sub-Header
///   018h 914h Data (2324 bytes)
///   92Ch 4    EDC (checksum across [010h..92Bh]) (or 00000000h if no EDC)
/// ```
pub struct Form2Sector<T> {
    sync:      SyncHeader,
    header:    Header,
    subheader: Subheader,
    data:      T,
    edc:       Todo,
}

impl<T: Encode> Encode for Form2Sector<T> {
    fn encode<W: ?Sized + DiscWrite>(&self, writer: &mut W) -> Result<(), EncodeError> {
        self.sync.encode(writer)?;
        self.header.encode(writer)?;

        let mut temp_buf = [0u8; 0x914 + 0x4 + 0x4];
        let mut temp_buf = Cursor::new(temp_buf.as_mut_slice());
        self.subheader.encode(&mut temp_buf)?;
        self.subheader.encode(&mut temp_buf)?; // subheader copy

        match self.data.encode(&mut temp_buf) {
            Ok(_) => {}
            Err(EncodeError::IO(err)) if err.kind() == std::io::ErrorKind::WriteZero => {
                tracing::warn!(
                    "data in form1 sector was truncated to 0x800 bytes (possible data loss)"
                );
            }
            err => return err,
        };

        let temp_buf = temp_buf.into_inner();
        // commit data from temporary buffer
        temp_buf.encode(writer)?;

        let edc = edc_checksum(temp_buf);
        edc.encode(writer)?;
        Ok(())
    }
}

impl Encode for LicenseString {
    fn encode<W: ?Sized + DiscWrite>(&self, writer: &mut W) -> Result<(), EncodeError> {
        static LICENSED_BY: &str = "          Licensed  by          ";

        LICENSED_BY.encode(writer)?;

        match self {
            LicenseString::EU => {
                "Sony Computer Entertainment Euro".encode(writer)?;
                " pe   ".encode(writer)?;
            }
            LicenseString::JP => {
                static LICENSE_JP: &str = "Sony Computer Entertainment Inc.";
                LICENSE_JP.encode(writer)?;
                0x0Au8.encode(writer)?;
            }
            LicenseString::US => {
                "Sony Computer Entertainment Amer".encode(writer)?;
                "  ica ".encode(writer)?;
            }
        };
        match self {
            LicenseString::EU | LicenseString::US => {
                Fill::zero(1978)
                    .encode(writer)
                    .wrap_err_with(|| miette!("error zero filling {self:?}"))?;
            }
            LicenseString::JP => {
                const fn value(idx: usize) -> u8 {
                    match idx {
                        0..=62 => 0x30,
                        63 => 0x0a,
                        64 => 0x30,
                        _ => unreachable!(),
                    }
                }
                static FILL: &[u8] = &core::array::from_fn::<u8, 64, _>(value);

                Fill::new(1983, FILL)
                    .encode(writer)
                    .wrap_err_with(|| miette!("error zero filling {self:?}"))?;
            }
        }
        Ok(())
    }
}

impl Encode for PsxSystemArea {
    fn encode<W: ?Sized + DiscWrite>(&self, writer: &mut W) -> Result<(), EncodeError> {
        for _ in 0..=3 {
            let bytes_in_sector = 0x800;
            let address = writer.address()?;
            let sector = Form1Sector::new(
                Fill::zero(bytes_in_sector),
                address,
                Submode::new_with_raw_value(0),
            );
            sector.encode(writer)?;
        }

        let license_string = Form1Sector::new(
            self.license_string,
            writer.address()?,
            Submode::new_with_raw_value(0),
        );
        license_string.encode(writer)?;

        // logo area
        for _ in 5..=11 {
            let bytes_in_sector = 0x800;
            let address = writer.address()?;
            let sector = Form1Sector::new(
                Fill::new(bytes_in_sector, &[0xff]),
                address,
                Submode::new_with_raw_value(0),
            );
            sector.encode(writer)?;
        }

        for _ in 12..=15 {
            let bytes_in_sector = 0x914;
            let sector = Form2Sector::new(
                Fill::zero(bytes_in_sector),
                writer.address()?,
                Submode::new_with_raw_value(0),
            );
            sector.encode(writer)?;
        }

        Ok(())
    }
}

fn write_primary_volumde_descriptor<W: DiscWrite>(
    w: &mut W,
    desc: &PrimaryVolumeDescriptor,
) -> Result<(), EncodeError> {
    let sector = Form2Sector::new(desc, w.address()?, Submode::new_with_raw_value(0));
    sector.encode(w)?;
    Ok(())
}
