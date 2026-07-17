#[path = "support/capabilities.rs"]
#[allow(dead_code)]
mod capabilities;
#[path = "support/producer.rs"]
#[allow(dead_code)]
mod producer;

use core::mem::{align_of_val, size_of_val};

use capabilities::{AllFeatures, FixedBytes};

type AllFeaturesBuffer = zero_schema::schema_buffer!(AllFeatures<'static>);

#[test]
fn schema_buffer_is_initialized_receiving_storage_not_a_schema_initializer() {
    let mut storage: AllFeaturesBuffer = zero_schema::make_schema_buffer!(AllFeatures<'static>);
    assert_eq!(storage.as_bytes().len(), AllFeatures::SCHEMA_SIZE);
    assert_eq!(size_of_val(&storage), AllFeatures::SCHEMA_STRIDE);
    assert_eq!(align_of_val(&storage), AllFeatures::SCHEMA_ALIGN);
    assert_eq!(
        storage
            .as_bytes()
            .as_ptr()
            .align_offset(AllFeatures::SCHEMA_ALIGN),
        0
    );
    assert!(
        AllFeatures::access(storage.as_bytes()).is_err(),
        "these concrete zero bytes are not a valid AllFeatures instance"
    );

    let producer = producer::all_features_mut();
    storage.as_bytes_mut().copy_from_slice(producer.as_bytes());
    let view = AllFeatures::access(storage.as_bytes())
        .expect("producer-filled receiving storage must be accessed explicitly");
    assert_eq!(
        (view.sequence(), view.name(), view.config().tag()),
        (
            0x0707_0707_0707_0707,
            "api",
            capabilities::ConfigKind::Memory
        )
    );
}

#[test]
fn schema_buffer_type_can_be_stored_and_passed() {
    struct Receiver {
        storage: AllFeaturesBuffer,
    }

    fn initialized_len(storage: &mut AllFeaturesBuffer) -> usize {
        storage.as_bytes_mut().len()
    }

    let mut receiver = Receiver {
        storage: zero_schema::make_schema_buffer!(AllFeatures<'static>),
    };
    assert_eq!(
        initialized_len(&mut receiver.storage),
        AllFeatures::SCHEMA_SIZE
    );
}

#[test]
fn fully_concrete_generic_root_has_receiving_storage_and_accesses_producer_bytes() {
    type FiveBytesBuffer = zero_schema::schema_buffer!(FixedBytes<'static, 5>);
    let mut storage = FiveBytesBuffer::new();
    assert_eq!(
        (storage.as_bytes().len(), align_of_val(&storage)),
        (FixedBytes::<5>::SCHEMA_SIZE, FixedBytes::<5>::SCHEMA_ALIGN)
    );

    let producer = producer::all_features_mut();
    let token = &producer.as_bytes()
        [producer::all_features_offsets::TOKEN..producer::all_features_offsets::TOKEN + 5];
    storage.as_bytes_mut().copy_from_slice(token);
    assert_eq!(
        FixedBytes::<5>::access(storage.as_bytes()).unwrap().bytes(),
        b"\x10\x20\x30\x40\x50"
    );
}
