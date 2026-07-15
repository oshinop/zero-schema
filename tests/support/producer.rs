use core::mem::{align_of_val, size_of_val};

pub const ALL_FEATURES_LEN: usize = 112;
pub const ALL_FEATURES_ALIGN: usize = 16;
pub const ALL_FEATURES_SHA256: &str =
    "8d8364d711f44feb2183a27db21d7211888c32e3125f15b800b13a42bce207c1";

pub mod all_features_offsets {
    pub const SEQUENCE: usize = 0;
    pub const ACTIVE: usize = 8;
    pub const PRIORITY: usize = 9;
    pub const NAME: usize = 10;
    pub const C_NAME: usize = 18;
    pub const WIDE: usize = 24;
    pub const WIDE_C: usize = 32;
    pub const TOKEN: usize = 38;
    pub const HEADER: usize = 44;
    pub const SAMPLES: usize = 52;
    pub const HEADERS: usize = 64;
    pub const CONFIG_KIND: usize = 80;
    pub const CONFIG: usize = 84;
    pub const CHECKSUM: usize = 96;

    pub const PADDING: &[(usize, usize)] = &[(25, 26), (43, 44), (81, 84), (87, 88), (97, 112)];
    pub const UNUSED_CAPACITY: &[(usize, usize)] = &[(14, 18), (22, 24), (28, 32), (36, 38)];
    pub const INACTIVE_UNION: (usize, usize) = (88, 96);
}

static ALL_FEATURES: &[u8; ALL_FEATURES_LEN] =
    include_bytes!("../../test-fixtures/schema-corpus/golden/all-features-record.bin");

#[derive(Clone)]
#[repr(C, align(16))]
pub struct AlignedAllFeatures {
    bytes: [u8; ALL_FEATURES_LEN],
}

impl AlignedAllFeatures {
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        &mut self.bytes
    }

    pub fn is_exactly_aligned(&self) -> bool {
        align_of_val(self) == ALL_FEATURES_ALIGN
            && self.bytes.as_ptr().align_offset(ALL_FEATURES_ALIGN) == 0
            && size_of_val(self) == ALL_FEATURES_LEN
    }
}

pub fn all_features() -> &'static [u8] {
    ALL_FEATURES
}

pub fn all_features_mut() -> AlignedAllFeatures {
    AlignedAllFeatures {
        bytes: *ALL_FEATURES,
    }
}
