use std::fs::File;

use super::*;
use crate::Encode;

#[test]
fn write_psx_system_area() {
    let mut f = File::create("./test-file.iso").unwrap();
    let sys = PsxSystemArea {
        license_string: LicenseString::JP,
    };
    sys.encode(
        &mut f,
        &EncodeCtx {
            cursor: Mss::new(0.into(), 0.into(), 0.into()),
        },
    )
    .unwrap();
}
