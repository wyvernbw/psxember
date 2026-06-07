use std::fs::File;

use super::*;
use crate::encoders::Encode;
use crate::iso9660::vol_desc::{PrimaryVolumeDescriptor, PrimaryVolumeDescriptorSpec};

#[test]
fn write_disc() -> crate::Result<()> {
    let mut f = File::create("./test-file.iso").unwrap();
    let sys = PsxSystemArea {
        license_string: LicenseString::JP,
    };
    let encode_ctx = EncodeCtx {
        cursor: Mss::new(0.into(), 0.into(), 0.into()),
    };
    sys.encode(&mut f, &encode_ctx)?;

    let desc = PrimaryVolumeDescriptor::new(PrimaryVolumeDescriptorSpec {
        vol_ident:          Some("my volume"),
        vol_space_size:     [0; 2],
        vol_set_size:       [0; 2],
        vol_seq_number:     [0; 2],
        lbs_bytes:          [0; 2],
        pt_bytes:           [0; 2],
        pt_block_num:       vol_desc::PathTableBlockNumbers {
            pt_1: 0.into(),
            pt_2: 0.into(),
            pt_3: 0.into(),
            pt_4: 0.into(),
        },
        vol_set_ident:      None,
        publisher_ident:    Some("WOOYVERN INC."),
        data_prep_ident:    None,
        copyright_file:     Some("copyright.txt"),
        abstract_file:      None,
        bibliographic_file: None,
    })?;
    desc.encode(&mut f, &encode_ctx)?;
    Ok(())
}
