use std::io::Cursor;

use bitbybit::bitfield;

use crate::encoders::{
    BigEndian, ByteConst, Encode, EncodeCtx, EncodeError, FillConst, str_to_ascii_buf,
};

use super::DiscWrite;

/// # Directory Record
///
/// The location of the Root Directory is described by a 34-byte Directory Record
/// being located in Primary Volume Descriptor entries 09Ch..0BDh. The data
/// therein is: Block Number (usually 22 on PSX disks), LEN_FI=01h, Name=00h, and,
/// LEN_SU=00h (due to the 34-byte limit).
/// ```plaintext
///  00h 1      Length of Directory Record (LEN_DR) (33+LEN_FI+pad+LEN_SU) (0=Pad)
///  01h 1      Extended Attribute Record Length (usually 00h)
///  02h 8      Data Logical Block Number (2x32bit)
///  0Ah 8      Data Size in Bytes        (2x32bit)
///  12h 7      Recording Timestamp       (yy-1900,mm,dd,hh,mm,ss,timezone)
///  19h 1      File Flags 8 bits         (usually 00h=File, or 02h=Directory)
///  1Ah 1      File Unit Size            (usually 00h)
///  1Bh 1      Interleave Gap Size       (usually 00h)
///  1Ch 4      Volume Sequence Number    (2x16bit, usually 0001h)
///  20h 1      Length of Name            (LEN_FI)
///  21h LEN_FI File/Directory Name ("FILENAME.EXT;1" or "DIR_NAME" or 00h or 01h)
///  xxh 0..1   Padding Field (00h) (only if LEN_FI is even)
///  xxh LEN_SU System Use (LEN_SU bytes) (see below for CD-XA disks)
/// ```
pub struct DirectoryRecord {
    len:                 u8,
    ext_attr_record_len: u8,
    data_lba:            [u32; 2],
    data_bytes_len:      [u32; 2],
    timestamp:           Timestamp,
    flags:               FileFlags,
    file_unit_size:      u8,
    interleave_gap_size: u8,
    vol_seq_num:         [u16; 2],
    ///  - `20h 1      Length of Name            (LEN_FI)`
    ///  - `21h LEN_FI File/Directory Name ("FILENAME.EXT;1" or "DIR_NAME" or 00h or 01h)`
    filename:            Filename,
    filename_padding:    FilenamePadding,
    system_use:          SystemUse,
}

pub struct DirectoryRecordBuilder {
    pub filename:   Filename,
    pub system_use: SystemUse,
    pub data_lba:   [u32; 2],
    pub data_size:  [u32; 2],
    pub timestamp:  Timestamp,
    pub flags:      FileFlags,
}

impl DirectoryRecord {
    pub fn new(
        DirectoryRecordBuilder {
            filename,
            system_use,
            data_lba,
            data_size,
            timestamp,
            flags,
        }: DirectoryRecordBuilder,
    ) -> Self {
        let filename_padding = FilenamePadding::from_filename(&filename);
        let len = 33 + filename.len + filename_padding.size() as u8 + system_use.len;
        Self {
            len,
            ext_attr_record_len: 0,
            data_lba,
            data_bytes_len: data_size,
            timestamp,
            flags,
            file_unit_size: 0,
            interleave_gap_size: 0,
            vol_seq_num: [0x0000, 0x0001],
            filename,
            filename_padding,
            system_use,
        }
    }
}

pub struct Timestamp {
    year:     u8,
    month:    u8,
    day:      u8,
    hour:     u8,
    min:      u8,
    sec:      u8,
    timezone: u8,
}

impl Timestamp {
    pub fn now() -> Self {
        // TODO: the now part
        Timestamp {
            year:     4, // 1994
            month:    2,
            day:      28,
            hour:     1,
            min:      0,
            sec:      0,
            timezone: 0,
        }
    }
}

impl Encode for Timestamp {
    fn encode<W: DiscWrite + ?Sized>(
        &self,
        writer: &mut W,
        ctx: &EncodeCtx,
    ) -> Result<(), EncodeError> {
        let Timestamp {
            year,
            month,
            day,
            hour,
            min,
            sec,
            timezone,
        } = self;
        year.encode(writer, ctx)?;
        month.encode(writer, ctx)?;
        day.encode(writer, ctx)?;
        hour.encode(writer, ctx)?;
        min.encode(writer, ctx)?;
        sec.encode(writer, ctx)?;
        timezone.encode(writer, ctx)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub enum FileFlags {
    File      = 0x00,
    Directory = 0x02,
}

impl Encode for FileFlags {
    fn encode<W: DiscWrite + ?Sized>(
        &self,
        writer: &mut W,
        ctx: &EncodeCtx,
    ) -> Result<(), EncodeError> {
        let value = *self as u8;
        value.encode(writer, ctx)
    }
}

pub struct Filename {
    len:  u8,
    name: [u8; 14],
}

#[derive(Debug)]
pub enum FilenameError {
    FilenameTooLarge,
    NotAscii,
}

impl Filename {
    pub fn from_ascii_str(string: &str) -> Result<Self, FilenameError> {
        if string.len() > 14 {
            return Err(FilenameError::FilenameTooLarge);
        }
        let filename = str_to_ascii_buf(string).map_err(|_| FilenameError::NotAscii)?;
        let filename = Filename {
            len:  string.len() as u8,
            name: filename,
        };
        Ok(filename)
    }
}

impl Encode for Filename {
    fn encode<W: DiscWrite + ?Sized>(
        &self,
        writer: &mut W,
        ctx: &EncodeCtx,
    ) -> Result<(), EncodeError> {
        self.len.encode(writer, ctx)?;
        self.name.encode(writer, ctx)?;
        Ok(())
    }
}

struct FilenamePadding {
    even: bool,
}

impl FilenamePadding {
    fn from_filename(filename: &Filename) -> Self {
        Self {
            even: filename.len.is_multiple_of(2),
        }
    }
}

impl Encode for FilenamePadding {
    fn size(&self) -> usize {
        self.even as usize
    }
    fn encode<W: ?Sized + DiscWrite>(
        &self,
        writer: &mut W,
        ctx: &EncodeCtx,
    ) -> Result<(), EncodeError> {
        if self.even {
            0x0u8.encode(writer, ctx)?;
        };
        Ok(())
    }
}

/// ```plaintext
///  00h 2      Owner ID Group  (whatever, usually 0000h, big endian)
///  02h 2      Owner ID User   (whatever, usually 0000h, big endian)
///  04h 2      File Attributes (big endian):
///  06h 2      Signature     ("XA")
///  08h 1      File Number   (Must match Subheader's File Number)
///  09h 5      Reserved      (00h-filled)
/// ```
#[derive(Default)]
pub struct SystemUse {
    len:            u8,
    owner_id_group: ByteConst<0x0>,
    owner_id_user:  ByteConst<0x0>,
    file_attr:      FileAttributes,
    signature:      Signature,
    file_number:    u8,
    zerofill:       FillConst<0x0, 5>,
}

impl SystemUse {
    pub const PSX_LEN_SU: usize = 14;

    pub fn root() -> Self {
        Self {
            len: 0,
            ..Self::default()
        }
    }
}

impl SystemUse {
    fn encode_impl<W: ?Sized + DiscWrite>(
        &self,
        writer: &mut W,
        ctx: &EncodeCtx,
    ) -> Result<(), EncodeError> {
        let SystemUse {
            len: _,
            owner_id_group,
            owner_id_user,
            file_attr,
            signature,
            file_number,
            zerofill,
        } = self;
        owner_id_group.encode(writer, ctx)?;
        owner_id_user.encode(writer, ctx)?;
        file_attr.encode(writer, ctx)?;
        signature.encode(writer, ctx)?;
        file_number.encode(writer, ctx)?;
        zerofill.encode(writer, ctx)?;
        Ok(())
    }
}

impl Encode for SystemUse {
    fn encode<W: ?Sized + DiscWrite>(
        &self,
        writer: &mut W,
        ctx: &EncodeCtx,
    ) -> Result<(), EncodeError> {
        let mut temp_buf = [0u8; 14];
        self.encode_impl(&mut Cursor::new(temp_buf.as_mut_slice()), ctx)?;

        (&temp_buf[..self.len as usize]).encode(writer, ctx)
    }
}

/// ```plaintext
///    0   Owner Read    (usually 1)
///    1   Reserved      (0)
///    2   Owner Execute (usually 1)
///    3   Reserved      (0)
///    4   Group Read    (usually 1)
///    5   Reserved      (0)
///    6   Group Execute (usually 1)
///    7   Reserved      (0)
///    8   World Read    (usually 1)
///    9   Reserved      (0)
///    10  World Execute (usually 1)
///    11  IS_MODE2        (0=MODE1 or CD-DA, 1=MODE2)
///    12  IS_MODE2_FORM2  (0=FORM1, 1=FORM2)
///    13  IS_INTERLEAVED  (0=No, 1=Yes...?) (by file and/or channel?)
///    14  IS_CDDA         (0=Data or ADPCM, 1=CD-DA Audio Track)
///    15  IS_DIRECTORY    (0=File or CD-DA, 1=Directory Record)
///  Commonly used Attributes are:
///    0D55h=Normal Binary File (with 800h-byte sectors)
///    1555h=Uncommon           (fade to black .DPS and .XA files)
///    2555h=Uncommon           (wipeout .AV files) (MODE1 ??)
///    4555h=CD-DA Audio Track  (wipeout .SWP files, alone .WAV file)
///    3D55h=Streaming File     (ADPCM and/or MDEC or so)
///    8D55h=Directory Record   (parent-, current-, or sub-directory)
/// ```
#[bitfield(u16)]
pub struct FileAttributes {
    #[bit(0, rw)]
    owner_r:        bool,
    #[bit(2, rw)]
    owner_x:        bool,
    #[bit(4, rw)]
    group_r:        bool,
    #[bit(6, rw)]
    group_x:        bool,
    #[bit(8, rw)]
    world_r:        bool,
    #[bit(10, rw)]
    world_x:        bool,
    #[bit(11, rw)]
    is_mode2:       bool,
    #[bit(12, rw)]
    is_mode2_form2: bool,
    #[bit(13, rw)]
    is_interleaved: bool,
    #[bit(14, rw)]
    is_cdda:        bool,
    #[bit(15, rw)]
    is_directory:   bool,
}

impl Encode for FileAttributes {
    fn encode<W: ?Sized + DiscWrite>(
        &self,
        writer: &mut W,
        ctx: &EncodeCtx,
    ) -> Result<(), EncodeError> {
        let value = self.raw_value();
        BigEndian(value).encode(writer, ctx)?;
        Ok(())
    }
}

impl Default for FileAttributes {
    fn default() -> Self {
        Self::new_with_raw_value(0x0)
            .with_owner_r(true)
            .with_owner_x(true)
            .with_group_r(true)
            .with_group_x(true)
            .with_world_r(true)
            .with_world_x(true)
    }
}

#[derive(Default)]
struct Signature;

impl Encode for Signature {
    fn encode<W: ?Sized + DiscWrite>(
        &self,
        writer: &mut W,
        ctx: &EncodeCtx,
    ) -> Result<(), EncodeError> {
        "XA".encode(writer, ctx)
    }
}

impl Encode for DirectoryRecord {
    fn encode<W: DiscWrite + ?Sized>(
        &self,
        writer: &mut W,
        ctx: &EncodeCtx,
    ) -> Result<(), EncodeError> {
        let DirectoryRecord {
            len,
            ext_attr_record_len,
            data_lba,
            data_bytes_len,
            timestamp,
            flags,
            file_unit_size,
            interleave_gap_size,
            vol_seq_num,
            filename,
            filename_padding,
            system_use,
        } = self;
        len.encode(writer, ctx)?;
        ext_attr_record_len.encode(writer, ctx)?;
        data_lba.encode(writer, ctx)?;
        data_bytes_len.encode(writer, ctx)?;
        timestamp.encode(writer, ctx)?;
        flags.encode(writer, ctx)?;
        file_unit_size.encode(writer, ctx)?;
        interleave_gap_size.encode(writer, ctx)?;
        vol_seq_num.encode(writer, ctx)?;
        filename.encode(writer, ctx)?;
        filename_padding.encode(writer, ctx)?;
        system_use.encode(writer, ctx)?;
        Ok(())
    }
}
