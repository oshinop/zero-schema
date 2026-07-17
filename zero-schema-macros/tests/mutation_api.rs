use core::ffi::CStr;
use zero_schema_macros::zero;
use zs::__private::{U16CStr, U16Str};
use zs::SchemaError as _;

#[path = "ui/pass/08_optional_zero_sentinel.rs"]
mod optional_zero_sentinel_pass;
fn release<T>(value: T) {
    drop(value);
}

#[test]
fn optional_ui_pass_fixture_exercises_runtime_proof_and_copy_into() {
    optional_zero_sentinel_pass::exercise();
}

#[zero(crate = zs)]
pub struct Packet {
    value: u16,
    enabled: bool,
    samples: [u16; 2],
}

#[zero(crate = zs)]
pub struct TextPacket<'a> {
    #[zero(capacity = 3)]
    pub narrow: &'a str,
    #[zero(capacity = 4)]
    pub c_narrow: &'a CStr,
    #[zero(capacity = 2)]
    pub wide: &'a U16Str,
    #[zero(capacity = 2)]
    pub c_wide: &'a U16CStr,
}

#[zero(crate = zs)]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    One = 1,
    Two = 2,
}

#[zero(crate = zs)]
pub struct EnumPacket {
    kind: Kind,
}

#[zero(crate = zs)]
pub struct BytesPacket<'a> {
    pub bytes: &'a [u8; 3],
}

#[zero(crate = zs)]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigKind {
    File = 1,
    Memory = 2,
}

#[zero(crate = zs)]
pub struct FileConfig {
    pub value: u8,
}

#[zero(crate = zs)]
pub struct MemoryConfig {
    pub value: u8,
}

#[zero(crate = zs)]
pub enum Config {
    #[zero(tag = ConfigKind::File)]
    File(FileConfig),
    #[zero(tag = ConfigKind::Memory)]
    Memory(MemoryConfig),
}

#[zero(crate = zs)]
pub struct Envelope {
    pub kind: ConfigKind,
    #[zero(tag_field = kind)]
    pub config: Config,
}

#[zero(crate = zs)]
pub struct EnvelopeArrayRoot {
    pub entries: [Envelope; 1],
}

#[zero(crate = zs)]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SwitchKind {
    Source = 1,
    Target = 2,
}

#[zero(crate = zs)]
pub struct InactiveSource {
    pub raw: u16,
}

#[zero(crate = zs)]
pub struct NestedSwitchTarget {
    pub kind: ConfigKind,
    #[zero(tag_field = kind)]
    pub config: Config,
}

#[zero(crate = zs)]
pub enum NestedSwitchPayload {
    #[zero(tag = SwitchKind::Source)]
    Source(InactiveSource),
    #[zero(tag = SwitchKind::Target)]
    Target(NestedSwitchTarget),
}

#[zero(crate = zs)]
pub struct NestedSwitchRoot {
    pub kind: SwitchKind,
    #[zero(tag_field = kind)]
    pub payload: NestedSwitchPayload,
}

#[zero(crate = zs)]
pub struct NestedSwitchArrayRoot {
    pub entries: [NestedSwitchRoot; 1],
}

#[zero(crate = zs)]
#[derive(Debug)]
pub struct GenericLeaf {
    pub value: u8,
}

pub trait CollisionBound<'a> {
    type Marker;
}
impl<'a> CollisionBound<'a> for GenericLeaf {
    type Marker = &'a ();
}

#[zero(crate = zs)]
pub struct GenericRecord<
    T: zs::__private::WireTypeSupport
        + zs::__private::SchemaPatchType
        + for<'view> zs::__private::LogicalSchema<'view>
        + core::fmt::Debug
        + for<'__zero_schema_source> CollisionBound<'__zero_schema_source>
        + 'static,
> {
    pub child: T,
}

#[zero(crate = zs)]
pub enum GenericPayload<
    T: zs::__private::WireTypeSupport
        + zs::__private::SchemaPatchType
        + for<'view> zs::__private::LogicalSchema<'view>
        + core::fmt::Debug
        + 'static,
> {
    #[zero(tag = ConfigKind::File)]
    File(T),
    #[zero(tag = ConfigKind::Memory)]
    Memory(MemoryConfig),
}

#[zero(crate = zs)]
pub struct GenericEnvelope<
    T: zs::__private::WireTypeSupport
        + zs::__private::SchemaPatchType
        + for<'view> zs::__private::LogicalSchema<'view>
        + core::fmt::Debug
        + 'static,
> {
    pub kind: ConfigKind,
    #[zero(tag_field = kind)]
    pub payload: GenericPayload<T>,
}

#[zero(crate = zs)]
pub struct BorrowedArrayChild<'a> {
    #[zero(capacity = 3)]
    pub name: &'a str,
}

#[zero(crate = zs)]
pub struct BorrowedArrayRoot<'a> {
    pub children: [BorrowedArrayChild<'a>; 2],
}

#[zero(crate = zs)]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OptionalKind {
    One = 1,
    Two = 2,
}

#[zero(crate = zs)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OptionalChild {
    pub required: OptionalKind,
    pub payload: u32,
}

#[zero(crate = zs)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OptionalPayload {
    pub required: OptionalKind,
}

#[zero(crate = zs)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OptionalTagged {
    #[zero(tag = OptionalKind::One)]
    One(OptionalPayload),
    #[zero(tag = OptionalKind::Two)]
    Two(OptionalPayload),
}

#[zero(crate = zs)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OptionalTaggedHolder {
    pub tag: OptionalKind,
    #[zero(tag_field = tag)]
    pub payload: OptionalTagged,
}

#[zero(crate = zs)]
pub struct OptionalMutationRoot {
    pub before: u8,
    #[zero(align = 8)]
    pub maybe_kind: Option<OptionalKind>,
    pub maybe_child: Option<OptionalChild>,
    pub maybe_array: Option<[OptionalKind; 2]>,
    pub maybe_tagged: Option<OptionalTaggedHolder>,
    pub after: u8,
}

#[zero(crate = zs)]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ZeroTaggedKind {
    Selected = 0,
}

#[zero(crate = zs)]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NonzeroRequired {
    One = 1,
}

#[zero(crate = zs)]
pub struct AllInvalidTaggedChild {
    pub required: NonzeroRequired,
}

#[zero(crate = zs)]
pub enum AllInvalidTaggedPayload {
    #[zero(tag = ZeroTaggedKind::Selected)]
    Selected(AllInvalidTaggedChild),
}

#[zero(crate = zs)]
pub struct AllInvalidTaggedRecord {
    pub tag: ZeroTaggedKind,
    #[zero(tag_field = tag)]
    pub payload: AllInvalidTaggedPayload,
}

#[zero(crate = zs)]
pub struct AllInvalidTaggedOption {
    pub value: Option<AllInvalidTaggedRecord>,
}

fn optional_field(name: &str) -> &'static zs::FieldDescriptor {
    OptionalMutationRoot::LAYOUT
        .fields()
        .iter()
        .find(|field| field.name() == name)
        .expect("declared field metadata")
}

fn optional_root_bytes() -> [u8; OptionalMutationRoot::SCHEMA_SIZE] {
    let mut bytes = [0xa5_u8; OptionalMutationRoot::SCHEMA_SIZE];
    for field in OptionalMutationRoot::LAYOUT
        .fields()
        .iter()
        .filter(|field| field.is_optional())
    {
        bytes[field.offset()..field.offset() + field.size()].fill(0);
    }
    bytes[optional_field("before").offset()] = 0x31;
    bytes[optional_field("after").offset()] = 0x53;
    bytes
}

#[test]
fn optional_mutation_and_patches_preserve_sentinel_invariants() {
    let child_field = optional_field("maybe_child");
    let kind_field = optional_field("maybe_kind");
    assert_eq!(
        optional_field("maybe_array").size(),
        2,
        "optional arrays store only their inner wire bytes"
    );
    assert!(child_field.is_optional());
    assert!(kind_field.is_optional());
    assert!(
        kind_field.size() > 1,
        "aligned storage has sentinel padding"
    );

    let parent_padding = (0..OptionalMutationRoot::SCHEMA_SIZE)
        .find(|byte| {
            !OptionalMutationRoot::LAYOUT
                .fields()
                .iter()
                .any(|field| field.offset() <= *byte && *byte < field.offset() + field.size())
        })
        .expect("aligned root has parent padding");
    let mut bytes = optional_root_bytes();
    bytes[parent_padding] = 0x7b;
    let absent = OptionalMutationRoot::access(&bytes).expect("parent padding is not a sentinel");
    assert!(absent.maybe_kind().is_none());
    assert!(absent.maybe_child().is_none());
    assert!(absent.maybe_array().is_none());
    assert!(absent.maybe_tagged().is_none());

    let mut malformed_some = bytes;
    malformed_some[kind_field.offset() + kind_field.size() - 1] = 0x44;
    assert!(
        OptionalMutationRoot::access(&malformed_some).is_err(),
        "nonzero storage padding makes the field Some, then the zero inner value is invalid"
    );

    {
        let mut root =
            OptionalMutationRoot::access_mut(&mut bytes).expect("absent optionals are valid");
        root.maybe_kind_mut()
            .set(Some(OptionalKind::One))
            .expect("initialize aligned optional storage");
    }
    bytes[kind_field.offset() + kind_field.size() - 1] = 0x5d;
    assert_eq!(
        OptionalMutationRoot::access(&bytes)
            .expect("valid inner value tolerates initialized aligned padding")
            .maybe_kind(),
        Some(OptionalKind::One)
    );
    {
        let mut root =
            OptionalMutationRoot::access_mut(&mut bytes).expect("aligned optional is valid");
        root.maybe_kind_mut()
            .set(None)
            .expect("clear aligned optional storage");
    }
    assert!(
        bytes[kind_field.offset()..kind_field.offset() + kind_field.size()]
            .iter()
            .all(|byte| *byte == 0),
        "None clears aligned StorageWire padding as well as its value"
    );
    assert_eq!(
        bytes[parent_padding], 0x7b,
        "aligned clear excludes parent padding"
    );

    {
        let mut root =
            OptionalMutationRoot::access_mut(&mut bytes).expect("absent optionals are valid");
        let mut child = root.maybe_child_mut();
        assert!(child.get().is_none());
        assert!(child.get_mut().is_none());
        child
            .set(Some(OptionalChild {
                required: OptionalKind::One,
                payload: 23,
            }))
            .expect("complete logical source initializes an absent optional");
        assert_eq!(
            child.get().expect("present child").required(),
            OptionalKind::One
        );
        child
            .get_mut()
            .expect("present child can be borrowed mutably")
            .required_mut()
            .set(OptionalKind::Two)
            .expect("nested child mutation");
    }
    assert_eq!(
        OptionalMutationRoot::access(&bytes)
            .expect("present child with zero padding is valid")
            .maybe_child()
            .expect("child is present")
            .required(),
        OptionalKind::Two
    );

    bytes[child_field.offset() + 1] = 0xc1;
    assert_eq!(
        OptionalMutationRoot::access(&bytes)
            .expect("nonzero inner padding does not invalidate a nonzero child")
            .maybe_child()
            .expect("child remains present")
            .payload(),
        23
    );
    let before_clear = bytes[optional_field("before").offset()];
    let after_clear = bytes[optional_field("after").offset()];
    {
        let mut root = OptionalMutationRoot::access_mut(&mut bytes).expect("padded child is valid");
        root.maybe_child_mut()
            .set(None)
            .expect("clearing a present optional is infallible");
    }
    assert!(
        bytes[child_field.offset()..child_field.offset() + child_field.size()]
            .iter()
            .all(|byte| *byte == 0),
        "None clears the complete StorageWire including internal padding"
    );
    assert_eq!(bytes[optional_field("before").offset()], before_clear);
    assert_eq!(bytes[optional_field("after").offset()], after_clear);
    assert_eq!(
        bytes[parent_padding], 0x7b,
        "parent padding is outside the clear span"
    );

    let before_incomplete = bytes;
    let incomplete = OptionalMutationRootPatch {
        maybe_child: Some(Some(OptionalChildPatch {
            required: None,
            payload: Some(99),
        })),
        ..Default::default()
    };
    let error = OptionalMutationRoot::access_mut(&mut bytes)
        .expect("absent optionals are valid")
        .copy_from(&incomplete)
        .expect_err("partial patches cannot initialize an absent optional");
    assert_eq!(
        error.kind(),
        zs::ErrorKind::IncompleteOptionalInitialization
    );
    assert_eq!(
        bytes, before_incomplete,
        "failed absent promotion is byte-exact"
    );

    {
        let mut root = OptionalMutationRoot::access_mut(&mut bytes)
            .expect("unchanged absent storage is valid");
        root.maybe_child_mut()
            .set(Some(OptionalChild {
                required: OptionalKind::Two,
                payload: 41,
            }))
            .expect("reinitialize child");
    }
    let present_partial = OptionalMutationRootPatch {
        maybe_child: Some(Some(OptionalChildPatch {
            required: Some(OptionalKind::One.into()),
            payload: None,
        })),
        ..Default::default()
    };
    OptionalMutationRoot::access_mut(&mut bytes)
        .expect("present child is valid")
        .copy_from(&present_partial)
        .expect("partial patch updates a present optional");
    let child = OptionalMutationRoot::access(&bytes)
        .expect("updated child is valid")
        .maybe_child()
        .expect("child remains present");
    assert_eq!(child.required(), OptionalKind::One);
    assert_eq!(child.payload(), 41);

    let before_retain = bytes;
    OptionalMutationRoot::access_mut(&mut bytes)
        .expect("present child is valid")
        .copy_from(&OptionalMutationRootPatch::default())
        .expect("outer None retains every optional field");
    assert_eq!(bytes, before_retain, "outer None is a byte-exact no-op");

    let clear = OptionalMutationRootPatch {
        maybe_child: Some(None),
        ..Default::default()
    };
    OptionalMutationRoot::access_mut(&mut bytes)
        .expect("present child is valid")
        .copy_from(&clear)
        .expect("Some(None) clears an optional field");
    assert!(
        bytes[child_field.offset()..child_field.offset() + child_field.size()]
            .iter()
            .all(|byte| *byte == 0),
        "patch clear uses the full optional storage span"
    );
    let complete = OptionalMutationRootPatch {
        maybe_child: Some(Some(OptionalChildPatch {
            required: Some(OptionalKind::Two.into()),
            payload: Some(73),
        })),
        maybe_array: Some(Some([OptionalKind::One, OptionalKind::Two])),
        ..Default::default()
    };
    OptionalMutationRoot::access_mut(&mut bytes)
        .expect("absent fields are valid")
        .copy_from(&complete)
        .expect("complete patches initialize absent path and array optionals");
    let copied = OptionalMutationRoot::access(&bytes)
        .expect("complete optional promotion is valid")
        .copy_into();
    assert_eq!(
        copied.maybe_child,
        Some(OptionalChild {
            required: OptionalKind::Two,
            payload: 73,
        })
    );
    assert_eq!(
        copied.maybe_array,
        Some([OptionalKind::One, OptionalKind::Two])
    );

    {
        let mut root =
            OptionalMutationRoot::access_mut(&mut bytes).expect("promoted fields are valid");
        root.maybe_tagged_mut()
            .set(Some(OptionalTaggedHolder {
                tag: OptionalKind::Two,
                payload: OptionalTagged::Two(OptionalPayload {
                    required: OptionalKind::One,
                }),
            }))
            .expect("tagged-containing eligible record initializes from an all-zero destination");
    }
    let tagged = OptionalMutationRoot::access(&bytes)
        .expect("tagged optional initialization commits payload before its tag")
        .maybe_tagged()
        .expect("tagged record is present");
    assert_eq!(tagged.tag(), OptionalKind::Two);
    assert_eq!(
        tagged
            .payload()
            .two()
            .expect("selected tagged payload")
            .required(),
        OptionalKind::One
    );
}

#[test]
fn optional_record_with_zero_tag_and_all_invalid_payload_is_eligible() {
    let mut bytes = [0_u8; AllInvalidTaggedOption::SCHEMA_SIZE];
    assert!(
        AllInvalidTaggedOption::access(&bytes)
            .expect("all-zero optional storage is None before inspecting its tagged value")
            .value()
            .is_none()
    );

    AllInvalidTaggedOption::access_mut(&mut bytes)
        .expect("all-zero optional storage is mutable")
        .value_mut()
        .set(Some(AllInvalidTaggedRecord {
            tag: ZeroTaggedKind::Selected,
            payload: AllInvalidTaggedPayload::Selected(AllInvalidTaggedChild {
                required: NonzeroRequired::One,
            }),
        }))
        .expect("all-invalid tagged payload permits a valid Some");

    let value = AllInvalidTaggedOption::access(&bytes)
        .expect("initialized optional tagged record proves")
        .value()
        .expect("Some record");
    assert_eq!(value.tag(), ZeroTaggedKind::Selected);
    assert_eq!(
        value
            .payload()
            .selected()
            .expect("selected payload")
            .required(),
        NonzeroRequired::One
    );
}
#[test]
fn generated_field_and_patch_mutation_are_transactional() {
    let mut bytes = [0_u8; Packet::SCHEMA_SIZE];
    bytes[0..2].copy_from_slice(&7_u16.to_ne_bytes());
    bytes[2] = 1;
    bytes[4..6].copy_from_slice(&3_u16.to_ne_bytes());
    bytes[6..8].copy_from_slice(&5_u16.to_ne_bytes());

    let original = bytes;
    let mut packet = Packet::access_mut(&mut bytes).expect("producer bytes are valid");
    assert!(packet.samples_mut().copy_from(&[31]).is_err());
    release(packet);
    assert_eq!(bytes, original, "wrong array length is a complete no-op");

    let mut packet = Packet::access_mut(&mut bytes).expect("producer bytes are valid");
    packet.value_mut().set(9).unwrap();
    packet.enabled_mut().set(false).unwrap();
    packet.samples_mut().copy_from(&[11, 13]).unwrap();
    release(packet);
    let before_default = bytes;
    let mut packet = Packet::access_mut(&mut bytes).expect("producer bytes are valid");
    let default = PacketPatch::default();
    packet.copy_from(&default).unwrap();
    release(packet);
    assert_eq!(bytes, before_default, "default patch preserves every byte");

    let mut packet = Packet::access_mut(&mut bytes).expect("producer bytes are valid");
    let patch = PacketPatch {
        value: Some(17),
        enabled: None,
        samples: Some([19, 23]),
    };
    packet.copy_from(&patch).unwrap();
    release(packet);

    let packet = Packet::access(&bytes).unwrap();
    assert_eq!(packet.value(), 17);
    assert!(!packet.enabled());
    assert_eq!(packet.samples().copy_into(), [19, 23]);
}

#[test]
fn bare_generic_record_and_tagged_payload_patches_are_real_and_atomic() {
    let mut record_bytes = [7_u8];
    let record_before = record_bytes;
    {
        let mut record = GenericRecord::<GenericLeaf>::access_mut(&mut record_bytes).unwrap();
        let default: GenericRecordPatch<'_, GenericLeaf> = Default::default();
        record.copy_from(&default).unwrap();
    }
    assert_eq!(
        record_bytes, record_before,
        "generic Default is a byte-exact no-op"
    );

    {
        let mut record = GenericRecord::<GenericLeaf>::access_mut(&mut record_bytes).unwrap();
        let full: GenericRecordPatch<'_, GenericLeaf> = GenericRecord {
            child: GenericLeaf { value: 9 },
        }
        .into();
        record.copy_from(&full).unwrap();
    }
    assert_eq!(
        GenericRecord::<GenericLeaf>::access(&record_bytes)
            .unwrap()
            .child()
            .value(),
        9
    );

    {
        let mut record = GenericRecord::<GenericLeaf>::access_mut(&mut record_bytes).unwrap();
        let partial = GenericRecordPatch {
            child: Some(GenericLeafPatch { value: Some(11) }),
        };
        record.copy_from(&partial).unwrap();
    }
    assert_eq!(
        GenericRecord::<GenericLeaf>::access(&record_bytes)
            .unwrap()
            .child()
            .value(),
        11
    );

    let mut envelope_bytes = [ConfigKind::File as u8, 5];
    let before_default = envelope_bytes;
    {
        let mut envelope = GenericEnvelope::<GenericLeaf>::access_mut(&mut envelope_bytes).unwrap();
        let default: GenericEnvelopePatch<'_, GenericLeaf> = Default::default();
        envelope.copy_from(&default).unwrap();
    }
    assert_eq!(
        envelope_bytes, before_default,
        "generic tagged Default is a byte-exact no-op"
    );

    {
        let mut envelope = GenericEnvelope::<GenericLeaf>::access_mut(&mut envelope_bytes).unwrap();
        let same = GenericEnvelopePatch {
            kind: None,
            payload: Some(GenericPayloadPatch::File(GenericLeafPatch {
                value: Some(9),
            })),
        };
        envelope.copy_from(&same).unwrap();
    }
    let selected = GenericEnvelope::<GenericLeaf>::access(&envelope_bytes)
        .unwrap()
        .payload();
    assert_eq!(selected.file().unwrap().value(), 9);

    let before_incomplete = envelope_bytes;
    {
        let mut envelope = GenericEnvelope::<GenericLeaf>::access_mut(&mut envelope_bytes).unwrap();
        let incomplete = GenericEnvelopePatch {
            kind: Some(ConfigKind::Memory),
            payload: Some(GenericPayloadPatch::Memory(MemoryConfigPatch {
                value: None,
            })),
        };
        assert!(envelope.copy_from(&incomplete).is_err());
    }
    assert_eq!(
        envelope_bytes, before_incomplete,
        "generic incomplete switch is byte-exact"
    );

    {
        let mut envelope = GenericEnvelope::<GenericLeaf>::access_mut(&mut envelope_bytes).unwrap();
        let switched = GenericEnvelopePatch {
            kind: Some(ConfigKind::Memory),
            payload: Some(GenericPayloadPatch::Memory(MemoryConfigPatch {
                value: Some(15),
            })),
        };
        envelope.copy_from(&switched).unwrap();
    }
    let selected = GenericEnvelope::<GenericLeaf>::access(&envelope_bytes)
        .unwrap()
        .payload();
    assert_eq!(selected.memory().unwrap().value(), 15);

    let full: GenericEnvelopePatch<'_, GenericLeaf> = GenericEnvelope {
        kind: ConfigKind::File,
        payload: GenericPayload::File(GenericLeaf { value: 17 }),
    }
    .into();
    assert!(matches!(full.payload, Some(GenericPayloadPatch::File(_))));
}

#[test]
fn nested_array_mutation_accepts_noncopy_borrowed_children_atomically() {
    let mut bytes = [0_u8; BorrowedArrayRoot::<'static>::SCHEMA_SIZE];
    bytes[0..4].copy_from_slice(&1_u32.to_ne_bytes());
    bytes[4] = b'a';
    bytes[8..12].copy_from_slice(&1_u32.to_ne_bytes());
    bytes[12] = b'b';

    {
        let mut root = BorrowedArrayRoot::access_mut(&mut bytes).unwrap();
        root.children_mut()
            .set(0, BorrowedArrayChild { name: "xy" })
            .unwrap();
        root.children_mut()
            .copy_from(&[
                BorrowedArrayChild { name: "one" },
                BorrowedArrayChild { name: "two" },
            ])
            .unwrap();
    }
    let view = BorrowedArrayRoot::access(&bytes).unwrap();
    assert_eq!(view.children().get(0).unwrap().name(), "one");
    assert_eq!(view.children().get(1).unwrap().name(), "two");

    let before = bytes;
    {
        let mut root = BorrowedArrayRoot::access_mut(&mut bytes).unwrap();
        assert!(
            root.children_mut()
                .copy_from(&[
                    BorrowedArrayChild { name: "ok" },
                    BorrowedArrayChild { name: "toolong" },
                ])
                .is_err()
        );
    }
    assert_eq!(
        bytes, before,
        "later non-Copy child preflight is byte-exact"
    );
}

#[test]
fn nested_tagged_logical_array_rejects_mismatched_sibling_tag_atomically() {
    let mut bytes = [0_u8; EnvelopeArrayRoot::SCHEMA_SIZE];
    bytes[0] = ConfigKind::File as u8;
    bytes[1] = 7;
    let before = bytes;

    let inconsistent = Envelope {
        kind: ConfigKind::File,
        config: Config::Memory(MemoryConfig { value: 9 }),
    };
    assert!(
        EnvelopeArrayRoot::access_mut(&mut bytes)
            .unwrap()
            .entries_mut()
            .set(0, inconsistent)
            .is_err()
    );
    assert_eq!(
        bytes, before,
        "mismatched nested logical tag must be a byte-exact no-op"
    );

    EnvelopeArrayRoot::access_mut(&mut bytes)
        .unwrap()
        .entries_mut()
        .set(
            0,
            Envelope {
                kind: ConfigKind::Memory,
                config: Config::Memory(MemoryConfig { value: 9 }),
            },
        )
        .unwrap();
    let entry = EnvelopeArrayRoot::access(&bytes)
        .unwrap()
        .entries()
        .get(0)
        .unwrap();
    assert_eq!(entry.config().memory().unwrap().value(), 9);
}

#[test]
fn generated_string_handles_preserve_capacity_tails() {
    let mut storage = zs::make_schema_buffer!(TextPacket<'static>);
    let bytes = storage.as_bytes_mut();
    bytes[0..4].copy_from_slice(&1_u32.to_ne_bytes());
    bytes[4] = b'a';
    bytes[8..10].copy_from_slice(b"b\0");
    bytes[12..16].copy_from_slice(&1_u32.to_ne_bytes());
    bytes[16..18].copy_from_slice(&11_u16.to_ne_bytes());
    bytes[20..22].copy_from_slice(&13_u16.to_ne_bytes());
    bytes[22..24].copy_from_slice(&0_u16.to_ne_bytes());
    let tail = bytes[7];
    let c = c"xy";
    let wide = U16Str::from_slice(&[17]);
    let c_wide = U16CStr::from_slice(&[19, 0]).unwrap();
    let mut text = TextPacket::access_mut(bytes).unwrap();
    text.narrow_mut().set("z").unwrap();
    text.c_narrow_mut().set(c).unwrap();
    text.wide_mut().set(wide).unwrap();
    text.c_wide_mut().set(c_wide).unwrap();
    release(text);

    let text = TextPacket::access(storage.as_bytes()).unwrap();
    assert_eq!(text.narrow(), "z");
    assert_eq!(text.c_narrow().to_bytes(), b"xy");
    assert_eq!(text.wide().as_slice(), &[17]);
    assert_eq!(text.c_wide().as_slice(), &[19]);
    assert_eq!(
        storage.as_bytes()[7],
        tail,
        "unused narrow capacity stays untouched"
    );
}

#[test]
fn enum_field_mutates_without_exposing_wire_storage() {
    let mut bytes = [1_u8];
    let mut packet = EnumPacket::access_mut(&mut bytes).unwrap();
    packet.kind_mut().set(Kind::Two).unwrap();
    release(packet);
    assert_eq!(EnumPacket::access(&bytes).unwrap().kind(), Kind::Two);
}

#[test]
fn fixed_byte_handle_requires_exact_source_length() {
    let mut bytes = [1_u8, 2, 3];
    let original = bytes;
    let mut packet = BytesPacket::access_mut(&mut bytes).unwrap();
    assert!(packet.bytes_mut().set(&[4, 5]).is_err());
    release(packet);
    assert_eq!(bytes, original);
    let mut packet = BytesPacket::access_mut(&mut bytes).unwrap();
    packet.bytes_mut().set(&[4, 5, 6]).unwrap();
    release(packet);
    assert_eq!(BytesPacket::access(&bytes).unwrap().bytes(), &[4, 5, 6]);
}

#[test]
fn tagged_patch_rejects_tag_only_and_updates_same_variant() {
    let mut bytes = [ConfigKind::File as u8, 7];
    let original = bytes;
    let mut envelope = Envelope::access_mut(&mut bytes).unwrap();
    let tag_only = EnvelopePatch {
        kind: Some(ConfigKind::Memory),
        config: None,
    };
    assert!(envelope.copy_from(&tag_only).is_err());
    release(envelope);
    assert_eq!(bytes, original, "tag-only patch is atomic");

    let mut envelope = Envelope::access_mut(&mut bytes).unwrap();
    let same = EnvelopePatch {
        kind: None,
        config: Some(ConfigPatch::File(FileConfigPatch { value: Some(9) })),
    };
    envelope.copy_from(&same).unwrap();
    release(envelope);
    assert_eq!(bytes, [ConfigKind::File as u8, 9]);
    let before_incomplete = bytes;
    let mut envelope = Envelope::access_mut(&mut bytes).unwrap();
    let incomplete = EnvelopePatch {
        kind: Some(ConfigKind::Memory),
        config: Some(ConfigPatch::Memory(MemoryConfigPatch { value: None })),
    };
    assert!(envelope.copy_from(&incomplete).is_err());
    release(envelope);
    assert_eq!(bytes, before_incomplete, "incomplete switch is atomic");

    let mut envelope = Envelope::access_mut(&mut bytes).unwrap();
    let switched = EnvelopePatch {
        kind: Some(ConfigKind::Memory),
        config: Some(ConfigPatch::Memory(MemoryConfigPatch { value: Some(15) })),
    };
    envelope.copy_from(&switched).unwrap();
    release(envelope);
    assert_eq!(bytes, [ConfigKind::Memory as u8, 15]);
}

#[test]
fn tagged_patch_switch_initializes_nested_tagged_target_before_outer_tag() {
    let mut bytes = [0_u8; NestedSwitchRoot::SCHEMA_SIZE];
    let outer_tag = NestedSwitchRoot::LAYOUT
        .fields()
        .iter()
        .find(|field| field.name() == "kind")
        .expect("outer tag descriptor");
    bytes[outer_tag.offset()] = SwitchKind::Source as u8;
    NestedSwitchRoot::access(&bytes)
        .expect("source variant is valid while target tag bytes are zero");

    let partial = NestedSwitchRootPatch {
        kind: Some(SwitchKind::Target),
        payload: Some(NestedSwitchPayloadPatch::Target(NestedSwitchTargetPatch {
            kind: Some(ConfigKind::File),
            config: Some(ConfigPatch::File(FileConfigPatch { value: None })),
        })),
    };
    let before_partial = bytes;
    {
        let mut root = NestedSwitchRoot::access_mut(&mut bytes).expect("source variant is mutable");
        assert!(root.copy_from(&partial).is_err());
    }
    assert_eq!(
        bytes, before_partial,
        "partial variant switch is byte-exact"
    );

    let complete = NestedSwitchRootPatch {
        kind: Some(SwitchKind::Target),
        payload: Some(NestedSwitchPayloadPatch::Target(NestedSwitchTargetPatch {
            kind: Some(ConfigKind::File),
            config: Some(ConfigPatch::File(FileConfigPatch { value: Some(27) })),
        })),
    };
    {
        let mut root =
            NestedSwitchRoot::access_mut(&mut bytes).expect("source variant remains mutable");
        root.copy_from(&complete).expect(
            "complete switch initializes the target without decoding its inactive zero tag",
        );
    }

    let root = NestedSwitchRoot::access(&bytes)
        .expect("payload initializes before its outer tag is committed");
    assert_eq!(root.kind(), SwitchKind::Target);
    let target = root
        .payload()
        .target()
        .expect("target payload selected after commit");
    assert_eq!(target.kind(), ConfigKind::File);
    assert_eq!(
        target
            .config()
            .file()
            .expect("inner payload selected")
            .value(),
        27
    );
}

#[test]
fn tagged_logical_copy_initializes_nested_inactive_target_before_outer_tag() {
    let mut bytes = [0_u8; NestedSwitchArrayRoot::SCHEMA_SIZE];
    let entries = NestedSwitchArrayRoot::LAYOUT
        .fields()
        .iter()
        .find(|field| field.name() == "entries")
        .expect("array descriptor");
    let outer_tag = NestedSwitchRoot::LAYOUT
        .fields()
        .iter()
        .find(|field| field.name() == "kind")
        .expect("outer tag descriptor");
    bytes[entries.offset() + outer_tag.offset()] = SwitchKind::Source as u8;

    NestedSwitchArrayRoot::access_mut(&mut bytes)
        .expect("source entry remains valid while target payload bytes are zero")
        .entries_mut()
        .copy_from(&[NestedSwitchRoot {
            kind: SwitchKind::Target,
            payload: NestedSwitchPayload::Target(NestedSwitchTarget {
                kind: ConfigKind::File,
                config: Config::File(FileConfig { value: 31 }),
            }),
        }])
        .expect("logical switch must initialize nested target bytes without decoding inactive zero tags");

    let entry = NestedSwitchArrayRoot::access(&bytes)
        .expect("payload must commit before its outer tag")
        .entries()
        .get(0)
        .expect("single entry");
    assert_eq!(entry.kind(), SwitchKind::Target);
    let target = entry.payload().target().expect("outer target selected");
    assert_eq!(target.kind(), ConfigKind::File);
    assert_eq!(
        target
            .config()
            .file()
            .expect("inner target selected")
            .value(),
        31
    );
}

#[test]
fn record_patch_preflights_later_string_before_earlier_commit() {
    let mut storage = zs::make_schema_buffer!(TextPacket<'static>);
    let bytes = storage.as_bytes_mut();
    bytes[0..4].copy_from_slice(&1_u32.to_ne_bytes());
    bytes[4] = b'a';
    bytes[8..10].copy_from_slice(b"b\0");
    bytes[12..16].copy_from_slice(&1_u32.to_ne_bytes());
    bytes[16..18].copy_from_slice(&11_u16.to_ne_bytes());
    bytes[20..22].copy_from_slice(&13_u16.to_ne_bytes());
    bytes[22..24].copy_from_slice(&0_u16.to_ne_bytes());
    let before: [u8; TextPacket::SCHEMA_SIZE] = storage.as_bytes().try_into().unwrap();
    let too_long = c"long";
    let patch = TextPacketPatch {
        narrow: Some("ok"),
        c_narrow: Some(too_long),
        wide: None,
        c_wide: None,
    };

    assert!(
        TextPacket::access_mut(storage.as_bytes_mut())
            .unwrap()
            .copy_from(&patch)
            .is_err()
    );
    assert_eq!(
        storage.as_bytes(),
        before,
        "a later string preflight failure must be byte-exact"
    );
}

#[test]
fn selected_payload_patch_cannot_switch_the_external_tag() {
    let mut bytes = [ConfigKind::File as u8, 7];
    let before = bytes;
    let switch = ConfigPatch::Memory(MemoryConfigPatch { value: Some(15) });
    {
        let mut envelope = Envelope::access_mut(&mut bytes).unwrap();
        let mut selected = envelope.config_mut();
        assert!(selected.copy_from(&switch).is_err());
    }
    assert_eq!(
        bytes, before,
        "selected payload mutation cannot alter the sibling tag"
    );
}

#[test]
fn scalar_layout_errors_preserve_their_source_after_mutation_conversion() {
    let access = match Kind::access(&[]) {
        Ok(_) => panic!("an empty scalar span is not exact"),
        Err(error) => error,
    };
    assert!(matches!(
        std::error::Error::source(&access)
            .and_then(|source| source.downcast_ref::<zs::LayoutError>()),
        Some(zs::LayoutError::IncorrectSize {
            expected: 1,
            actual: 0
        })
    ));

    let mutation: KindMutationError = access.into();
    let access_source = std::error::Error::source(&mutation)
        .and_then(|source| source.downcast_ref::<KindAccessError>())
        .expect("mutation conversion preserves the scalar access error");
    assert!(matches!(
        std::error::Error::source(access_source)
            .and_then(|source| source.downcast_ref::<zs::LayoutError>()),
        Some(zs::LayoutError::IncorrectSize {
            expected: 1,
            actual: 0
        })
    ));
}
