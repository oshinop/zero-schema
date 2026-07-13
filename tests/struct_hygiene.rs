use zero_schema::{ValidationContext, ValidationResult, ZeroSchema};

fn input(_: &u8, _: &ValidationContext<'_>) -> ValidationResult {
    Ok(())
}
fn value(_: &u8, _: &ValidationContext<'_>) -> ValidationResult {
    Ok(())
}
fn validator(_: &u8, _: &ValidationContext<'_>) -> ValidationResult {
    Ok(())
}
fn __zero_input(_: &u8, _: &ValidationContext<'_>) -> ValidationResult {
    Ok(())
}
fn __zero_decoded_field_0(_: &u8, _: &ValidationContext<'_>) -> ValidationResult {
    Ok(())
}
fn __zero_decoded_value(_: &u8, _: &ValidationContext<'_>) -> ValidationResult {
    Ok(())
}

#[allow(non_snake_case)]
#[derive(ZeroSchema)]
struct Collisions {
    #[zero(validate_with = input)]
    input: u8,
    #[zero(validate_with = value)]
    value: u8,
    #[zero(validate_with = validator)]
    validator: u8,
    #[zero(validate_with = __zero_input)]
    __zero_input: u8,
    #[zero(validate_with = __zero_decoded_field_0)]
    __zero_decoded_field_0: u8,
    #[zero(validate_with = __zero_decoded_value)]
    __zero_decoded_value: u8,
    #[zero(must_equal = 9)]
    __ZERO_MUST_EQUAL_FIELD_0: u8,
    __end: u8,
    Result: u8,
    Option: u8,
    u8: u8,
}
impl Collisions {
    const __ZERO_MUST_EQUAL_FIELD_6: u8 = 9;
}

#[test]
fn declaration_scope_paths_are_not_shadowed_by_fields() {
    let value = Collisions {
        input: 1,
        value: 2,
        validator: 3,
        __zero_input: 4,
        __zero_decoded_field_0: 5,
        __zero_decoded_value: 6,
        __ZERO_MUST_EQUAL_FIELD_0: 9,
        __end: 8,
        Result: 10,
        Option: 11,
        u8: 12,
    };
    let mut buffer = zero_schema::make_buffer_for!(Collisions);
    value.encode_into(buffer.as_bytes_mut()).unwrap();
    let decoded = Collisions::parse(buffer.as_bytes()).unwrap();
    assert_eq!(
        (
            decoded.input,
            decoded.value,
            decoded.validator,
            decoded.__zero_input,
            decoded.__zero_decoded_field_0,
            decoded.__zero_decoded_value,
            decoded.__ZERO_MUST_EQUAL_FIELD_0,
            decoded.__end,
            decoded.Result,
            decoded.Option,
            decoded.u8
        ),
        (1, 2, 3, 4, 5, 6, 9, 8, 10, 11, 12)
    );
}
