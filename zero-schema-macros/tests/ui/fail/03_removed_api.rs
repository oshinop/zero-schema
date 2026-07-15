use zero_schema_macros::zero;

#[zero(crate = zs)]
struct LegacyApi {
    value: u8,
}

fn main() {
    let mut bytes = [1_u8];
    let _ = LegacyApi::parse(&bytes);
    let _ = LegacyApi::parse_prefix(&bytes);
    let _ = LegacyApi::encode(&LegacyApi { value: 1 });
    let _ = LegacyApi::encode_into(&LegacyApi { value: 1 }, &mut bytes);
    let _ = LegacyApi::encoded_len();
    let _ = LegacyApi::build();
    let _ = LegacyApi::init();
}
