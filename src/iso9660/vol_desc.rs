//! # Volume Descriptors
//!
//! module for the primary volume descriptor (sector 16) and the descriptor set
//! terminator (sector 17).

use crate::encoders::{
    BigEndian, ByteConst, Encode, EncodeCtx, FillConst, LittleEndian, PaddedConst,
};
use crate::iso9660::Todo;

/// Primary Volume Descriptor (sector 16 on PSX disks)
///```plaintext
///  000h 1    Volume Descriptor Type        (01h=Primary Volume Descriptor)
///  001h 5    Standard Identifier           ("CD001")
///  006h 1    Volume Descriptor Version     (01h=Standard)
///  007h 1    Reserved                      (00h)
///  008h 32   System Identifier             (a-characters) ("PLAYSTATION")
///  028h 32   Volume Identifier             (d-characters) (max 8 chars for PSX?)
///  048h 8    Reserved                      (00h)
///  050h 8    Volume Space Size             (2x32bit, number of logical blocks)
///  058h 32   Reserved                      (00h)
///  078h 4    Volume Set Size               (2x16bit) (usually 0001h)
///  07Ch 4    Volume Sequence Number        (2x16bit) (usually 0001h)
///  080h 4    Logical Block Size in Bytes   (2x16bit) (usually 0800h) (1 sector)
///  084h 8    Path Table Size in Bytes      (2x32bit) (max 800h for PSX)
///  08Ch 4    Path Table 1 Block Number     (32bit little-endian)
///  090h 4    Path Table 2 Block Number     (32bit little-endian) (or 0=None)
///  094h 4    Path Table 3 Block Number     (32bit big-endian)
///  098h 4    Path Table 4 Block Number     (32bit big-endian) (or 0=None)
///  09Ch 34   Root Directory Record         (see next chapter)
///  0BEh 128  Volume Set Identifier         (d-characters) (usually empty)
///  13Eh 128  Publisher Identifier          (a-characters) (company name)
///  1BEh 128  Data Preparer Identifier      (a-characters) (empty or other)
///  23Eh 128  Application Identifier        (a-characters) ("PLAYSTATION")
///  2BEh 37   Copyright Filename            ("FILENAME.EXT;VER") (empty or text)
///  2E3h 37   Abstract Filename             ("FILENAME.EXT;VER") (empty)
///  308h 37   Bibliographic Filename        ("FILENAME.EXT;VER") (empty)
///  32Dh 17   Volume Creation Timestamp     ("YYYYMMDDHHMMSSFF",timezone)
///  33Eh 17   Volume Modification Timestamp ("0000000000000000",00h)
///  34Fh 17   Volume Expiration Timestamp   ("0000000000000000",00h)
///  360h 17   Volume Effective Timestamp    ("0000000000000000",00h)
///  371h 1    File Structure Version        (01h=Standard)
///  372h 1    Reserved for future           (00h-filled)
///  373h 141  Application Use Area          (00h-filled for PSX and VCD)
///  400h 8    CD-XA Identifying Signature   ("CD-XA001" for PSX and VCD)
///  408h 2    CD-XA Flags (unknown purpose) (00h-filled for PSX and VCD)
///  40Ah 8    CD-XA Startup Directory       (00h-filled for PSX and VCD)
///  412h 8    CD-XA Reserved                (00h-filled for PSX and VCD)
///  41Ah 345  Application Use Area          (00h-filled for PSX and VCD)
///  573h 653  Reserved for future           (00h-filled)
///```
pub struct PrimaryVolumeDescriptor {
    desc_type:             ByteConst<0x01>,
    std_ident:             StandardIdentifier,
    desc_ver:              ByteConst<0x01>,
    _res_01:               ByteConst<0x00>,
    sys_ident:             PaddedConst<SystemIdentifier, 32>,
    vol_ident:             PaddedConst<[u8; 8], 32>,
    _res_02:               FillConst<0x0, 8>,
    vol_space_size:        [u32; 2],
    _res_03:               FillConst<0x0, 32>,
    vol_set_size:          [u16; 2],
    vol_seq_number:        [u16; 2],
    /// logical block size in bytes
    lbs_bytes:             [u16; 2],
    pt_bytes:              [u32; 2],
    pt_block_num:          PathTableBlockNumbers,
    root_dir_record:       RootDirectoryRecord,
    vol_set_ident:         [u8; 128],
    publisher_ident:       [u8; 128],
    data_prep_ident:       [u8; 128],
    app_ident:             ApplicationIdentifier,
    copyright_file:        [u8; 37],
    abstract_file:         [u8; 37],
    bibliographic_file:    [u8; 37],
    vol_creation_time:     VolumeZeroTimestamp,
    vol_modification_time: VolumeZeroTimestamp,
    vol_expiration_time:   VolumeZeroTimestamp,
    vol_effective_time:    VolumeZeroTimestamp,
    file_structure_ver:    ByteConst<0x01>,
    _res_04:               ByteConst<0x00>,
    app_use_area_01:       FillConst<0x00, 141>,
    cdxa_ident_sig:        CdXAIdentSignature,
    cdxa_flags:            FillConst<0x00, 2>,
    cdxa_startup_dir:      FillConst<0x00, 8>,
    cdxa_reserved:         FillConst<0x00, 8>,
    app_use_area_02:       FillConst<0x00, 345>,
    _res_05:               FillConst<0x0, 653>,
}

struct StandardIdentifier;

impl Encode for StandardIdentifier {
    fn size(&self) -> usize {
        5
    }
    fn encode<W: super::DiscWrite>(&self, writer: &mut W, ctx: &EncodeCtx) -> std::io::Result<()> {
        "CD001".encode(writer, ctx)
    }
}

struct SystemIdentifier;

impl Encode for SystemIdentifier {
    fn size(&self) -> usize {
        "PLAYSTATION".len()
    }
    fn encode<W: super::DiscWrite>(&self, writer: &mut W, ctx: &EncodeCtx) -> std::io::Result<()> {
        "PLAYSTATION".encode(writer, ctx)
    }
}

/// ```plaintext
///  08Ch 4    Path Table 1 Block Number     (32bit little-endian)
///  090h 4    Path Table 2 Block Number     (32bit little-endian) (or 0=None)
///  094h 4    Path Table 3 Block Number     (32bit big-endian)
///  098h 4    Path Table 4 Block Number     (32bit big-endian) (or 0=None)
/// ```
struct PathTableBlockNumbers {
    pt_1: LittleEndian<u32>,
    pt_2: LittleEndian<u32>,
    pt_3: BigEndian<u32>,
    pt_4: BigEndian<u32>,
}

impl Encode for PathTableBlockNumbers {
    fn encode<W: super::DiscWrite>(&self, writer: &mut W, ctx: &EncodeCtx) -> std::io::Result<()> {
        self.pt_1.encode(writer, ctx)?;
        self.pt_2.encode(writer, ctx)?;
        self.pt_3.encode(writer, ctx)?;
        self.pt_4.encode(writer, ctx)?;
        Ok(())
    }
}

type RootDirectoryRecord = Todo;

struct ApplicationIdentifier;

impl Encode for ApplicationIdentifier {
    fn size(&self) -> usize {
        128
    }
    fn encode<W: super::DiscWrite>(&self, writer: &mut W, ctx: &EncodeCtx) -> std::io::Result<()> {
        PaddedConst::new::<128>("PLAYSTATION").encode(writer, ctx)
    }
}

struct VolumeZeroTimestamp;

impl Encode for VolumeZeroTimestamp {
    fn size(&self) -> usize {
        17
    }
    fn encode<W: super::DiscWrite>(&self, writer: &mut W, ctx: &EncodeCtx) -> std::io::Result<()> {
        PaddedConst::new::<17>("0000000000000000").encode(writer, ctx)
    }
}

struct CdXAIdentSignature;

impl Encode for CdXAIdentSignature {
    fn size(&self) -> usize {
        "CD-XA001".len()
    }
    fn encode<W: super::DiscWrite>(&self, writer: &mut W, ctx: &EncodeCtx) -> std::io::Result<()> {
        "CD-XA001".encode(writer, ctx)
    }
}

impl Encode for PrimaryVolumeDescriptor {
    fn encode<W: super::DiscWrite>(&self, writer: &mut W, ctx: &EncodeCtx) -> std::io::Result<()> {
        let PrimaryVolumeDescriptor {
            desc_type,
            std_ident,
            desc_ver,
            _res_01,
            sys_ident,
            vol_ident,
            _res_02,
            vol_space_size,
            _res_03,
            vol_set_size,
            vol_seq_number,
            lbs_bytes,
            pt_bytes,
            pt_block_num,
            root_dir_record,
            vol_set_ident,
            publisher_ident,
            data_prep_ident,
            app_ident,
            copyright_file,
            abstract_file,
            bibliographic_file,
            vol_creation_time,
            vol_modification_time,
            vol_expiration_time,
            vol_effective_time,
            file_structure_ver,
            _res_04,
            app_use_area_01,
            cdxa_ident_sig,
            cdxa_flags,
            cdxa_startup_dir,
            cdxa_reserved,
            app_use_area_02,
            _res_05,
        } = self;
        desc_type.encode(writer, ctx)?;
        std_ident.encode(writer, ctx)?;
        desc_ver.encode(writer, ctx)?;
        _res_01.encode(writer, ctx)?;
        sys_ident.encode(writer, ctx)?;
        vol_ident.encode(writer, ctx)?;
        _res_02.encode(writer, ctx)?;
        vol_space_size.encode(writer, ctx)?;
        _res_03.encode(writer, ctx)?;
        vol_set_size.encode(writer, ctx)?;
        vol_seq_number.encode(writer, ctx)?;
        lbs_bytes.encode(writer, ctx)?;
        pt_bytes.encode(writer, ctx)?;
        pt_block_num.encode(writer, ctx)?;
        root_dir_record.encode(writer, ctx)?;
        vol_set_ident.encode(writer, ctx)?;
        publisher_ident.encode(writer, ctx)?;
        data_prep_ident.encode(writer, ctx)?;
        app_ident.encode(writer, ctx)?;
        copyright_file.encode(writer, ctx)?;
        abstract_file.encode(writer, ctx)?;
        bibliographic_file.encode(writer, ctx)?;
        vol_creation_time.encode(writer, ctx)?;
        vol_modification_time.encode(writer, ctx)?;
        vol_expiration_time.encode(writer, ctx)?;
        vol_effective_time.encode(writer, ctx)?;
        file_structure_ver.encode(writer, ctx)?;
        _res_04.encode(writer, ctx)?;
        app_use_area_01.encode(writer, ctx)?;
        cdxa_ident_sig.encode(writer, ctx)?;
        cdxa_flags.encode(writer, ctx)?;
        cdxa_startup_dir.encode(writer, ctx)?;
        cdxa_reserved.encode(writer, ctx)?;
        app_use_area_02.encode(writer, ctx)?;
        _res_05.encode(writer, ctx)?;
        Ok(())
    }
}

/// # Volume Descriptor Set Terminator (sector 17 on PSX disks)
/// ```plaintext
///   000h 1    Volume Descriptor Type    (FFh=Terminator)
///   001h 5    Standard Identifier       ("CD001")
///   006h 1    Terminator Version        (01h=Standard)
///   007h 2041 Reserved                  (00h-filled)
/// ```
pub struct VolumeDescriptorSetTerminator {
    desc_ty:        ByteConst<0xff>,
    standard_ident: StandardIdentifier,
    terminator_ver: ByteConst<0x01>,
    zerofill:       FillConst<0x0, 2041>,
}

impl Encode for VolumeDescriptorSetTerminator {
    fn encode<W: super::DiscWrite>(&self, writer: &mut W, ctx: &EncodeCtx) -> std::io::Result<()> {
        let VolumeDescriptorSetTerminator {
            desc_ty,
            standard_ident,
            terminator_ver,
            zerofill,
        } = self;
        desc_ty.encode(writer, ctx)?;
        standard_ident.encode(writer, ctx)?;
        terminator_ver.encode(writer, ctx)?;
        zerofill.encode(writer, ctx)?;
        Ok(())
    }
}
