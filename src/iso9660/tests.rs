use std::fs::File;

use super::*;
use crate::encoders::Encode;
use crate::iso9660::vol_desc::{PrimaryVolumeDescriptor, PrimaryVolumeDescriptorSpec};

#[test]
fn write_disc() -> crate::Result<()> {
    miette::set_panic_hook();

    let mut f = File::create("./test-file.iso").into_diagnostic()?;
    let sys = PsxSystemArea {
        license_string: LicenseString::JP,
    };
    sys.encode(&mut f)?;

    let desc = PrimaryVolumeDescriptor::new(PrimaryVolumeDescriptorSpec {
        vol_ident:          Some("MY_VOLUME"),
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
        copyright_file:     Some("COPYRIGHT.TXT"),
        abstract_file:      None,
        bibliographic_file: None,
    })?;
    write_primary_volumde_descriptor(&mut f, &desc)?;
    Ok(())
}
