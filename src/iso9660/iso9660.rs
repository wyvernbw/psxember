pub mod fs;
#[cfg(test)]
mod tests;
pub mod vol_desc;

use std::io::{self, Seek, Write};
use std::marker::{Destruct, PhantomData};

use arbitrary_int::prelude::*;
use bitbybit::{bitfield, *};
use miette::IntoDiagnostic;

use crate::encoders::{Encode, EncodeCtx, EncodeError, Fill};

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
        Self::new_with_raw_value(value)
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
    fn encode<W: ?Sized + DiscWrite>(
        &self,
        writer: &mut W,
        _ctx: &EncodeCtx,
    ) -> Result<(), EncodeError> {
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
    edc:       Todo,
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
    cinfo:   Todo,
}

/// # 1st Subheader byte - File Number (FN)
struct FileNumber(u8);
/// # 2nd Subheader byte - Channel Number (CN)
struct ChannelNumber(u8);

macro_rules! impl_encode_newtype {
    ($type:ty) => {
        impl Encode for $type {
            fn encode<W: ?Sized + DiscWrite>(
                &self,
                writer: &mut W,
                ctx: &EncodeCtx,
            ) -> Result<(), EncodeError> {
                self.0.encode(writer, ctx)
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
#[bitfield(u8)]
struct CodingInfo {
    #[bits(0..=1, rw)]
    mono_stereo: u2,
    #[bits(2..=3)]
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
enum LicenseString {
    EU,
    JP,
    US,
}

impl Encode for SyncHeader {
    fn encode<W: ?Sized + DiscWrite>(
        &self,
        writer: &mut W,
        ctx: &EncodeCtx,
    ) -> Result<(), EncodeError> {
        let pattern = &[
            0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00,
        ];
        let fill = Fill {
            pattern,
            total_bytes: 0x0c,
        };
        fill.encode(writer, ctx)?;
        Ok(())
    }
}

impl<T: Encode> Encode for Mss<T> {
    fn encode<W: ?Sized + DiscWrite>(
        &self,
        writer: &mut W,
        ctx: &EncodeCtx,
    ) -> Result<(), EncodeError> {
        self.min.encode(writer, ctx)?;
        self.sec.encode(writer, ctx)?;
        self.sect.encode(writer, ctx)?;
        Ok(())
    }
}

impl Encode for Bcd {
    fn encode<W: ?Sized + DiscWrite>(
        &self,
        writer: &mut W,
        ctx: &EncodeCtx,
    ) -> Result<(), EncodeError> {
        self.raw_value().encode(writer, ctx)
    }
}

impl Encode for HeaderMode {
    fn encode<W: ?Sized + DiscWrite>(
        &self,
        writer: &mut W,
        ctx: &EncodeCtx,
    ) -> Result<(), EncodeError> {
        let mode = *self as u8;
        mode.encode(writer, ctx)
    }
}

impl Encode for Header {
    fn encode<W: ?Sized + DiscWrite>(
        &self,
        writer: &mut W,
        ctx: &EncodeCtx,
    ) -> Result<(), EncodeError> {
        self.mss.encode(writer, ctx)?;
        self.mode.encode(writer, ctx)?;
        Ok(())
    }
}

impl Encode for Submode {
    fn encode<W: ?Sized + DiscWrite>(
        &self,
        writer: &mut W,
        ctx: &EncodeCtx,
    ) -> Result<(), EncodeError> {
        self.raw_value().encode(writer, ctx)
    }
}

impl Encode for Subheader {
    fn encode<W: ?Sized + DiscWrite>(
        &self,
        writer: &mut W,
        ctx: &EncodeCtx,
    ) -> Result<(), EncodeError> {
        self.file.encode(writer, ctx)?;
        self.channel.encode(writer, ctx)?;
        self.submode.encode(writer, ctx)?;
        self.cinfo.encode(writer, ctx)?;
        Ok(())
    }
}

struct DataBlock<'a, T: Encode, F> {
    data: &'a T,
    _ty:  PhantomData<F>,
}

impl<T: Encode, F> DataBlock<'_, T, F> {
    fn encode_impl<W: ?Sized + DiscWrite>(
        &self,
        writer: &mut W,
        ctx: &EncodeCtx,
    ) -> Result<(), EncodeError>
    where
        Self: Encode,
    {
        let block_size = self.size();
        assert!(
            self.data.size() <= block_size,
            "data ({}) is {} bytes, greater than data block size {}",
            std::any::type_name::<T>(),
            self.data.size(),
            block_size
        );
        let padding = block_size.saturating_sub(self.data.size());
        let fill = Fill::zero(padding);
        self.data.encode(writer, ctx)?;
        fill.encode(writer, ctx)?;
        Ok(())
    }
}

impl<'a, T: Encode> DataBlock<'a, T, Form1Sector<T>> {
    #[must_use]
    fn new_form1(data: &'a T) -> Self {
        Self {
            data,
            _ty: PhantomData,
        }
    }
}
impl<'a, T: Encode> DataBlock<'a, T, Form2Sector<T>> {
    #[must_use]
    fn new_form2(data: &'a T) -> Self {
        Self {
            data,
            _ty: PhantomData,
        }
    }
}

impl<T: Encode> Encode for DataBlock<'_, T, Form1Sector<T>> {
    fn size(&self) -> usize {
        0x800
    }
    fn encode<W: ?Sized + DiscWrite>(
        &self,
        writer: &mut W,
        ctx: &EncodeCtx,
    ) -> Result<(), EncodeError> {
        self.encode_impl(writer, ctx)
    }
}

impl<T: Encode> Encode for DataBlock<'_, T, Form2Sector<T>> {
    fn size(&self) -> usize {
        0x924
    }
    fn encode<W: ?Sized + DiscWrite>(
        &self,
        writer: &mut W,
        ctx: &EncodeCtx,
    ) -> Result<(), EncodeError> {
        self.encode_impl(writer, ctx)
    }
}

impl<T: Encode> Encode for Form1Sector<T> {
    fn encode<W: ?Sized + DiscWrite>(
        &self,
        writer: &mut W,
        ctx: &EncodeCtx,
    ) -> Result<(), EncodeError> {
        self.sync.encode(writer, ctx)?;
        self.header.encode(writer, ctx)?;
        self.subheader.encode(writer, ctx)?;
        self.subheader.encode(writer, ctx)?; // subheader copy

        DataBlock::new_form1(&self.data).encode(writer, ctx)?;

        self.edc.encode(writer, ctx)?;
        self.ecc.encode(writer, ctx)?;
        Ok(())
    }
}

impl<T> Form1Sector<T> {
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
                cinfo: (),
            },
            data,
            edc: (),
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
                cinfo: (),
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
    fn encode<W: ?Sized + DiscWrite>(
        &self,
        writer: &mut W,
        ctx: &EncodeCtx,
    ) -> Result<(), EncodeError> {
        self.sync.encode(writer, ctx)?;
        self.header.encode(writer, ctx)?;
        self.subheader.encode(writer, ctx)?;
        self.subheader.encode(writer, ctx)?; // subheader copy

        let data = DataBlock::new_form2(&self.data);
        data.encode(writer, ctx)?;

        self.edc.encode(writer, ctx)?;
        Ok(())
    }
}

impl Encode for LicenseString {
    fn encode<W: ?Sized + DiscWrite>(
        &self,
        writer: &mut W,
        ctx: &EncodeCtx,
    ) -> Result<(), EncodeError> {
        "          Licensed  by          ".encode(writer, ctx)?;
        match self {
            LicenseString::EU => {
                "Sony Computer Entertainment Euro".encode(writer, ctx)?;
                " pe   ".encode(writer, ctx)?;
            }
            LicenseString::JP => {
                "Sony Computer Entertainment Inc.".encode(writer, ctx)?;
                0x0Au8.encode(writer, ctx)?;
            }
            LicenseString::US => {
                "Sony Computer Entertainment Amer".encode(writer, ctx)?;
                "  ica ".encode(writer, ctx)?;
            }
        };
        match self {
            LicenseString::EU | LicenseString::US => {
                Fill::zero(1978).encode(writer, ctx)?;
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

                Fill::new(1983, FILL).encode(writer, ctx)?;
            }
        }
        Ok(())
    }
}

impl Encode for PsxSystemArea {
    fn encode<W: ?Sized + DiscWrite>(
        &self,
        writer: &mut W,
        ctx: &EncodeCtx,
    ) -> Result<(), EncodeError> {
        for _ in 0..=3 {
            let bytes_in_sector = 0x800;
            let address = writer.address()?;
            let sector = Form1Sector::new(
                Fill::zero(bytes_in_sector),
                address,
                Submode::new_with_raw_value(0),
            );
            sector.encode(writer, ctx)?;
        }
        self.license_string.encode(writer, ctx)?;

        // logo area
        Fill::new(0x930 * 0x7, &[0xff]).encode(writer, ctx)?;

        for _ in 12..=15 {
            let bytes_in_sector = 0x924;
            let sector = Form2Sector::new(
                Fill::zero(bytes_in_sector),
                writer.address()?,
                Submode::new_with_raw_value(0),
            );
            sector.encode(writer, ctx)?;
        }

        Ok(())
    }
}
