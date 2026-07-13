#![deny(unreachable_pub)]

mod self_override {
    pub(crate) mod zs {
        pub(crate) use zero_schema::*;
    }
    use zero_schema::ZeroSchema;

    #[derive(ZeroSchema)]
    #[repr(u16)]
    #[zero(crate = self::zs, endian = "big")]
    pub(crate) enum Code {
        Value = 0x0102,
    }

    pub(crate) fn bytes() -> [u8; 2] {
        let mut buffer = zero_schema::make_buffer_for!(Code);
        Code::Value.encode_into(buffer.as_bytes_mut()).unwrap();
        [buffer.as_bytes()[0], buffer.as_bytes()[1]]
    }
}

mod super_override {
    pub(crate) mod zs {
        pub(crate) use zero_schema::*;
    }
    pub(crate) mod nested {
        use zero_schema::ZeroSchema;

        #[derive(ZeroSchema)]
        #[repr(u8)]
        #[zero(crate = super::zs)]
        pub(crate) enum Flag {
            Set = 7,
        }

        pub(crate) fn roundtrip() -> u8 {
            let mut buffer = zero_schema::make_buffer_for!(Flag);
            Flag::Set.encode_into(buffer.as_bytes_mut()).unwrap();
            match Flag::parse(buffer.as_bytes()).unwrap() {
                Flag::Set => 7,
            }
        }
    }
}

mod shadowed_prelude {
    #![allow(dead_code, non_camel_case_types, non_snake_case, unused_macros)]
    pub(crate) struct Option;
    pub(crate) struct Result;
    pub(crate) struct u16;
    macro_rules! write {
        () => {};
    }
    macro_rules! panic {
        () => {};
    }
    macro_rules! assert {
        () => {};
    }

    use zero_schema::ZeroSchema;
    mod __zero_schema_Shadow {}

    #[derive(ZeroSchema)]
    #[repr(u16)]
    pub(crate) enum Shadow {
        Raw = 9,
    }

    pub(crate) fn check() -> (usize, usize, usize, usize) {
        let buffer = zero_schema::make_buffer_for!(Shadow);
        let offset = buffer.as_bytes().as_ptr().align_offset(Shadow::WIRE_ALIGN);
        (
            offset,
            buffer.as_bytes().len(),
            core::mem::size_of_val(&buffer),
            core::mem::align_of_val(&buffer),
        )
    }
}

mod derive_name_collision {
    use zero_schema::ZeroSchema as Clone;

    #[derive(Clone)]
    #[repr(u8)]
    pub(crate) enum Code {
        Value = 11,
    }

    pub(crate) fn roundtrip() -> u8 {
        let mut buffer = zero_schema::make_buffer_for!(Code);
        Code::Value.encode_into(buffer.as_bytes_mut()).unwrap();
        match Code::parse(buffer.as_bytes()).unwrap() {
            Code::Value => 11,
        }
    }
}

pub mod public_parent {
    mod implementation {
        #![allow(non_camel_case_types)]
        use zero_schema::ZeroSchema;
        #[derive(ZeroSchema)]
        #[repr(u8)]
        pub enum r#Type {
            r#type = 3,
        }
    }
    pub use implementation::Type;
}

#[test]
fn explicit_relative_runtime_paths_work_from_parent_and_hidden_scopes() {
    assert_eq!(self_override::bytes(), [1, 2]);
    assert_eq!(super_override::nested::roundtrip(), 7);
}

#[test]
fn expansion_is_shadow_safe_and_buffer_layout_is_exact() {
    let (offset, len, size, align) = shadowed_prelude::check();
    assert_eq!((offset, len, size, align), (0, 2, 2, 2));
}

#[test]
fn generated_error_derives_ignore_macro_alias_collisions() {
    assert_eq!(derive_name_collision::roundtrip(), 11);
}

#[test]
fn public_parent_reexport_and_raw_names_work() {
    let mut bytes = [0u8; 1];
    public_parent::Type::r#type.encode_into(&mut bytes).unwrap();
    assert_eq!(bytes, [3]);
    assert_eq!(public_parent::Type::LAYOUT.name(), "Type");
    assert_eq!(public_parent::Type::LAYOUT.enum_values()[0].name(), "type");
}
