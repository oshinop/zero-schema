#![deny(unreachable_pub)]

pub mod crate_override {
    pub mod zs {
        pub use zero_schema::*;
    }

    use zero_schema::zero;

    #[zero(crate = crate::crate_override::zs)]
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    #[repr(u8)]
    #[allow(non_camel_case_types)]
    pub enum r#Type {
        r#type = 255,
    }
}

pub mod shadowed_prelude {
    #![allow(dead_code, non_camel_case_types)]

    pub(crate) struct Result;
    pub(crate) struct u8;

    use zero_schema::zero;

    #[zero]
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    #[repr(u8)]
    pub enum Code {
        Maximum = 255,
    }
}

#[test]
fn attribute_runtime_path_raw_identifiers_and_shadowed_prelude_access_producer_bytes() {
    let producer = include_bytes!("../test-fixtures/schema-corpus/golden/code8-max.bin");
    assert_eq!(
        crate_override::Type::access(producer).unwrap().get(),
        crate_override::Type::r#type
    );
    assert_eq!(
        shadowed_prelude::Code::access(producer)
            .unwrap()
            .copy_into(),
        shadowed_prelude::Code::Maximum
    );
    assert_eq!(crate_override::Type::LAYOUT.enum_values()[0].name(), "type");
}
