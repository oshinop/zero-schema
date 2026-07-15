use core::{ffi::CStr, mem};
use std::hint::black_box;
use widestring::{U16CStr, U16Str};
use zero_schema::zero;

// Reviewed all-features producer output. This is copied into aligned receiving storage; it is
// deliberately not a Rust-side initializer or encoder for the schema.
const REVIEWED_BYTES: &[u8; 112] = b"\x07\x07\x07\x07\x07\x07\x07\x07\x01\x03\x03\x61\x70\x69\x71\x72\x73\x74\x73\x76\x63\x00\x78\x79\x01\x5a\x41\x41\x42\x42\x43\x43\x44\x44\x00\x00\x45\x45\x10\x20\x30\x40\x50\x5b\x22\x22\x70\x72\x6f\x64\x00\x7a\x11\x11\x11\x11\x12\x12\x12\x12\x13\x13\x13\x13\x24\x24\x6f\x6e\x65\x00\x78\x79\x25\x25\x74\x77\x6f\x00\x71\x72\x02\x5c\x5d\x5e\x33\x33\x01\x5f\x49\x4e\x41\x43\x54\x49\x56\x45\x6a\x6b\x6b\x6b\x6b\x6b\x6b\x6b\x6b\x6b\x6b\x6b\x6b\x6b\x6b\x6b";
const SCHEMA_SIZE: usize = 112;
const SCHEMA_ALIGN: usize = 16;

#[zero]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Priority {
    Low = 1,
    Normal = 2,
    High = 3,
}

#[zero]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ConfigKind {
    File = 1,
    Memory = 2,
    Reserved = 3,
}

#[zero]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Header<'a> {
    version: u16,
    #[zero(capacity = 6)]
    producer: &'a CStr,
}

#[zero]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryConfig {
    capacity: u16,
    enabled: bool,
}

#[zero]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileConfig<'a> {
    header: Header<'a>,
    flags: u32,
}

#[zero]
#[derive(Debug, PartialEq)]
pub enum Config<'a> {
    #[zero(tag = ConfigKind::File)]
    File(FileConfig<'a>),
    #[zero(tag = ConfigKind::Memory)]
    Memory(MemoryConfig),
}

#[zero(align = 16)]
#[derive(Debug, PartialEq)]
pub struct AllFeatures<'a> {
    sequence: u64,
    active: bool,
    priority: Priority,
    #[zero(capacity = 7, len_type = u8)]
    name: &'a str,
    #[zero(capacity = 6)]
    c_name: &'a CStr,
    #[zero(capacity = 2, len_type = u8, align = 4)]
    wide: &'a U16Str,
    #[zero(capacity = 3)]
    wide_c: &'a U16CStr,
    token: &'a [u8; 5],
    header: Header<'a>,
    samples: [u32; 3],
    headers: [Header<'a>; 2],
    config_kind: ConfigKind,
    #[zero(tag_field = config_kind)]
    config: Config<'a>,
    checksum: u8,
}

// Zero remains outside this enum's logical domain, so it can use the zero-sentinel Option
// representation without a separate presence byte.
#[zero]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum OptionalCode {
    Enabled = 1,
    Disabled = 2,
}

#[zero]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OptionalRecord {
    code: OptionalCode,
    count: u32,
}

#[zero(align = 8)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OptionalBenchmark {
    maybe_code: Option<OptionalCode>,
    maybe_record: Option<OptionalRecord>,
    maybe_array: Option<[OptionalCode; 2]>,
}

const OPTIONAL_SCHEMA_ALIGN: usize = 8;
const REVIEWED_OPTION_NONE_BYTES: [u8; OptionalBenchmark::SCHEMA_SIZE] =
    [0; OptionalBenchmark::SCHEMA_SIZE];

/// Producer-owned, reviewed all-zero bytes for the Option-only schema. Copying this fixture
/// creates aligned receiving storage; it does not initialize wire bytes through a schema API.
#[repr(C, align(8))]
#[derive(Clone, Copy)]
struct OptionProducerBytes([u8; OptionalBenchmark::SCHEMA_SIZE]);

impl OptionProducerBytes {
    fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    fn as_bytes_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }
}

fn option_producer_bytes() -> OptionProducerBytes {
    OptionProducerBytes(REVIEWED_OPTION_NONE_BYTES)
}

/// Producer-owned, reviewed bytes. Creating this value only copies the fixture; it does not
/// establish schema validity or initialize a wire value.
#[repr(C, align(16))]
struct ProducerBytes([u8; SCHEMA_SIZE]);

impl ProducerBytes {
    fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    fn as_bytes_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }
}

fn producer_bytes() -> ProducerBytes {
    ProducerBytes(*REVIEWED_BYTES)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HandConfig {
    File { version: u16, flags: u32 },
    Memory { capacity: u16, enabled: bool },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct HandMaterialized<'a> {
    sequence: u64,
    active: bool,
    priority: u8,
    name: &'a str,
    c_name: &'a [u8],
    wide: [u16; 2],
    wide_len: usize,
    wide_c: [u16; 3],
    wide_c_len: usize,
    token: &'a [u8],
    header: (u16, &'a [u8]),
    samples: [u32; 3],
    headers: [(u16, &'a [u8]); 2],
    config: HandConfig,
    checksum: u8,
}

#[derive(Clone, Copy)]
struct HandView<'a> {
    bytes: &'a [u8],
}

impl<'a> HandView<'a> {
    /// This is the fixed-layout, hand-written observer used as the comparison baseline. It
    /// performs the same representation checks that matter for the reviewed schema while
    /// intentionally ignoring padding, unused string capacity, and inactive union storage.
    fn access(bytes: &'a [u8]) -> Result<Self, ()> {
        if bytes.len() != SCHEMA_SIZE || bytes.as_ptr().align_offset(SCHEMA_ALIGN) != 0 {
            return Err(());
        }

        if !matches!(bytes[8], 0 | 1) || !matches!(bytes[9], 1..=3) {
            return Err(());
        }
        let name_len = bytes[10] as usize;
        if name_len > 7 || core::str::from_utf8(&bytes[11..11 + name_len]).is_err() {
            return Err(());
        }
        if nul_terminated(&bytes[18..24]).is_none() {
            return Err(());
        }
        if bytes[24] as usize > 2 || nul_terminated_u16(bytes, 32, 3).is_none() {
            return Err(());
        }
        if nul_terminated(&bytes[46..52]).is_none()
            || nul_terminated(&bytes[66..72]).is_none()
            || nul_terminated(&bytes[74..80]).is_none()
        {
            return Err(());
        }
        match bytes[80] {
            1 => {
                if nul_terminated(&bytes[86..92]).is_none() {
                    return Err(());
                }
            }
            2 if matches!(bytes[86], 0 | 1) => {}
            _ => return Err(()),
        }

        Ok(Self { bytes })
    }

    fn sequence(&self) -> u64 {
        read_u64(self.bytes, 0)
    }

    fn active(&self) -> bool {
        self.bytes[8] != 0
    }

    fn priority(&self) -> u8 {
        self.bytes[9]
    }

    fn name(&self) -> &'a str {
        let len = self.bytes[10] as usize;
        core::str::from_utf8(&self.bytes[11..11 + len]).expect("access proved UTF-8")
    }

    fn c_name(&self) -> &'a [u8] {
        nul_terminated(&self.bytes[18..24]).expect("access proved terminator")
    }

    fn wide(&self) -> ([u16; 2], usize) {
        let words = [read_u16(self.bytes, 26), read_u16(self.bytes, 28)];
        (words, self.bytes[24] as usize)
    }

    fn wide_c(&self) -> ([u16; 3], usize) {
        let words = [
            read_u16(self.bytes, 32),
            read_u16(self.bytes, 34),
            read_u16(self.bytes, 36),
        ];
        let len = words
            .iter()
            .position(|word| *word == 0)
            .expect("access proved wide terminator");
        (words, len)
    }

    fn token(&self) -> &'a [u8] {
        &self.bytes[38..43]
    }

    fn header(&self, offset: usize) -> (u16, &'a [u8]) {
        (
            read_u16(self.bytes, offset),
            nul_terminated(&self.bytes[offset + 2..offset + 8]).expect("access proved terminator"),
        )
    }

    fn samples(&self) -> [u32; 3] {
        [
            read_u32(self.bytes, 52),
            read_u32(self.bytes, 56),
            read_u32(self.bytes, 60),
        ]
    }

    fn config(&self) -> HandConfig {
        match self.bytes[80] {
            1 => HandConfig::File {
                version: read_u16(self.bytes, 84),
                flags: read_u32(self.bytes, 92),
            },
            2 => HandConfig::Memory {
                capacity: read_u16(self.bytes, 84),
                enabled: self.bytes[86] != 0,
            },
            _ => unreachable!("access proved a selected union tag"),
        }
    }

    fn checksum(&self) -> u8 {
        self.bytes[96]
    }

    fn copy_into(&self) -> HandMaterialized<'a> {
        let (wide, wide_len) = self.wide();
        let (wide_c, wide_c_len) = self.wide_c();
        HandMaterialized {
            sequence: self.sequence(),
            active: self.active(),
            priority: self.priority(),
            name: self.name(),
            c_name: self.c_name(),
            wide,
            wide_len,
            wide_c,
            wide_c_len,
            token: self.token(),
            header: self.header(44),
            samples: self.samples(),
            headers: [self.header(64), self.header(72)],
            config: self.config(),
            checksum: self.checksum(),
        }
    }
}

fn nul_terminated(bytes: &[u8]) -> Option<&[u8]> {
    bytes
        .iter()
        .position(|byte| *byte == 0)
        .map(|end| &bytes[..end])
}

fn nul_terminated_u16(bytes: &[u8], offset: usize, count: usize) -> Option<usize> {
    (0..count).find(|index| read_u16(bytes, offset + index * mem::size_of::<u16>()) == 0)
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_ne_bytes(
        bytes[offset..offset + 2]
            .try_into()
            .expect("checked fixed offset"),
    )
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_ne_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("checked fixed offset"),
    )
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_ne_bytes(
        bytes[offset..offset + 8]
            .try_into()
            .expect("checked fixed offset"),
    )
}

fn handwritten_field_mutation(bytes: &mut [u8]) {
    bytes[0..8].copy_from_slice(&0x0102_0304_0506_0708_u64.to_ne_bytes());
    bytes[10] = 5;
    bytes[11..16].copy_from_slice(b"bench");
    bytes[46..52].copy_from_slice(b"bench\0");
    bytes[56..60].copy_from_slice(&0x0a0b_0c0d_u32.to_ne_bytes());
    bytes[84..86].copy_from_slice(&0x7777_u16.to_ne_bytes());
    bytes[86] = 0;
}

fn handwritten_patch(bytes: &mut [u8]) {
    handwritten_field_mutation(bytes);
    bytes[52..56].copy_from_slice(&31_u32.to_ne_bytes());
    bytes[56..60].copy_from_slice(&37_u32.to_ne_bytes());
    bytes[60..64].copy_from_slice(&41_u32.to_ne_bytes());
}

fn generated_field_mutation(storage: &mut ProducerBytes) {
    let mut view =
        AllFeatures::access_mut(storage.as_bytes_mut()).expect("reviewed producer bytes");
    view.sequence_mut()
        .set(0x0102_0304_0506_0708)
        .expect("scalar fits");
    view.name_mut().set("bench").expect("string fits");
    {
        let mut header = view.header_mut();
        header.producer_mut().set(c"bench").expect("string fits");
    }
    view.samples_mut().set(1, 0x0a0b_0c0d).expect("index fits");
    {
        let mut config = view.config_mut();
        let mut memory = config.memory_mut().expect("fixture selects Memory");
        memory.capacity_mut().set(0x7777).expect("scalar fits");
        memory.enabled_mut().set(false).expect("boolean fits");
    }
}

fn generated_patch(storage: &mut ProducerBytes) {
    let patch = AllFeaturesPatch {
        sequence: Some(0x0102_0304_0506_0708),
        name: Some("bench"),
        header: Some(HeaderPatch {
            version: None,
            producer: Some(c"bench"),
        }),
        samples: Some([31, 37, 41]),
        config_kind: None,
        config: Some(ConfigPatch::Memory(MemoryConfigPatch {
            capacity: Some(0x7777),
            enabled: Some(false),
        })),
        ..Default::default()
    };
    AllFeatures::access_mut(storage.as_bytes_mut())
        .expect("reviewed producer bytes")
        .copy_from(&patch)
        .expect("patch is valid");
}

fn set_present_options(storage: &mut OptionProducerBytes) {
    let mut view = OptionalBenchmark::access_mut(storage.as_bytes_mut())
        .expect("reviewed all-zero optional producer bytes");
    view.maybe_code_mut()
        .set(Some(OptionalCode::Enabled))
        .expect("nonzero enum initializes optional storage");
    view.maybe_record_mut()
        .set(Some(OptionalRecord {
            code: OptionalCode::Disabled,
            count: 0x0102_0304,
        }))
        .expect("complete record initializes optional storage");
    view.maybe_array_mut()
        .set(Some([OptionalCode::Enabled, OptionalCode::Disabled]))
        .expect("complete fixed array initializes optional storage");
}

fn clear_present_options(storage: &mut OptionProducerBytes) {
    let mut view = OptionalBenchmark::access_mut(storage.as_bytes_mut())
        .expect("present optional producer bytes");
    view.maybe_code_mut()
        .set(None)
        .expect("clear enum optional storage");
    view.maybe_record_mut()
        .set(None)
        .expect("clear record optional storage");
    view.maybe_array_mut()
        .set(None)
        .expect("clear fixed-array optional storage");
}

fn complete_options_patch() -> OptionalBenchmarkPatch {
    OptionalBenchmarkPatch {
        maybe_code: Some(Some(OptionalCodePatch {
            value: Some(OptionalCode::Enabled),
        })),
        maybe_record: Some(Some(OptionalRecordPatch {
            code: Some(OptionalCodePatch {
                value: Some(OptionalCode::Disabled),
            }),
            count: Some(0x0102_0304),
        })),
        maybe_array: Some(Some([OptionalCode::Enabled, OptionalCode::Disabled])),
    }
}

fn promote_options(storage: &mut OptionProducerBytes, patch: &OptionalBenchmarkPatch) {
    OptionalBenchmark::access_mut(storage.as_bytes_mut())
        .expect("reviewed all-zero optional producer bytes")
        .copy_from(patch)
        .expect("complete tri-state patch promotes every optional");
}

fn assert_all_none_options(storage: &OptionProducerBytes) {
    let view = OptionalBenchmark::access(storage.as_bytes())
        .expect("reviewed all-zero optional producer bytes");
    assert!(view.maybe_code().is_none());
    assert!(view.maybe_record().is_none());
    assert!(view.maybe_array().is_none());
}

fn assert_present_options(storage: &OptionProducerBytes) {
    let view = OptionalBenchmark::access(storage.as_bytes())
        .expect("present optional producer bytes remain valid");
    assert_eq!(view.maybe_code(), Some(OptionalCode::Enabled));
    let record = view.maybe_record().expect("record optional is present");
    assert_eq!(
        (record.code(), record.count()),
        (OptionalCode::Disabled, 0x0102_0304)
    );
    assert_eq!(
        view.maybe_array().map(|array| array.copy_into()),
        Some([OptionalCode::Enabled, OptionalCode::Disabled])
    );
}

fn capabilities(c: &mut criterion::Criterion) {
    assert_eq!(AllFeatures::SCHEMA_SIZE, SCHEMA_SIZE);
    assert_eq!(mem::size_of::<ProducerBytes>(), SCHEMA_SIZE);
    assert_eq!(mem::align_of::<ProducerBytes>(), SCHEMA_ALIGN);
    assert_eq!(AllFeatures::SCHEMA_ALIGN, SCHEMA_ALIGN);

    let producer = producer_bytes();
    let generated_view = AllFeatures::access(producer.as_bytes()).expect("reviewed producer bytes");
    let hand_view = HandView::access(producer.as_bytes()).expect("reviewed producer bytes");
    let expected = hand_view.copy_into();
    let materialized = generated_view.copy_into();
    assert_eq!(materialized.sequence, expected.sequence);
    assert_eq!(materialized.active, expected.active);
    assert_eq!(materialized.priority as u8, expected.priority);
    assert_eq!(materialized.name, expected.name);
    assert_eq!(materialized.c_name.to_bytes(), expected.c_name);
    assert_eq!(
        materialized.wide.as_slice(),
        &expected.wide[..expected.wide_len]
    );
    assert_eq!(
        materialized.wide_c.as_slice(),
        &expected.wide_c[..expected.wide_c_len]
    );
    assert_eq!(materialized.token, expected.token);
    assert_eq!(
        (
            materialized.header.version,
            materialized.header.producer.to_bytes()
        ),
        expected.header
    );
    assert_eq!(materialized.samples, expected.samples);
    for (index, expected_header) in expected.headers.iter().enumerate() {
        assert_eq!(
            (
                materialized.headers[index].version,
                materialized.headers[index].producer.to_bytes()
            ),
            *expected_header
        );
    }
    match (materialized.config, expected.config) {
        (Config::Memory(actual), HandConfig::Memory { capacity, enabled }) => {
            assert_eq!((actual.capacity, actual.enabled), (capacity, enabled));
        }
        (Config::File(actual), HandConfig::File { version, flags }) => {
            assert_eq!((actual.header.version, actual.flags), (version, flags));
        }
        _ => panic!("generated and handwritten union observations disagree"),
    }
    assert_eq!(materialized.checksum, expected.checksum);
    let mut generated_mutation = producer_bytes();
    generated_field_mutation(&mut generated_mutation);
    let mut handwritten_mutation = producer_bytes();
    handwritten_field_mutation(handwritten_mutation.as_bytes_mut());
    assert_eq!(
        generated_mutation.as_bytes(),
        handwritten_mutation.as_bytes()
    );
    assert!(AllFeatures::access(generated_mutation.as_bytes()).is_ok());

    let mut generated_patch_bytes = producer_bytes();
    generated_patch(&mut generated_patch_bytes);
    let mut handwritten_patch_bytes = producer_bytes();
    handwritten_patch(handwritten_patch_bytes.as_bytes_mut());
    assert_eq!(
        generated_patch_bytes.as_bytes(),
        handwritten_patch_bytes.as_bytes()
    );
    assert!(AllFeatures::access(generated_patch_bytes.as_bytes()).is_ok());

    let mut group = c.benchmark_group("capabilities");
    group.throughput(criterion::Throughput::Bytes(SCHEMA_SIZE as u64));

    group.bench_function(
        criterion::BenchmarkId::new("generated_access", "all_features"),
        |b| {
            b.iter(|| {
                black_box(
                    AllFeatures::access(black_box(producer.as_bytes()))
                        .expect("reviewed producer bytes"),
                )
            })
        },
    );
    group.bench_function(
        criterion::BenchmarkId::new("handwritten_access", "all_features"),
        |b| {
            b.iter(|| {
                black_box(
                    HandView::access(black_box(producer.as_bytes()))
                        .expect("reviewed producer bytes"),
                )
            })
        },
    );

    group.bench_function(
        criterion::BenchmarkId::new("generated_field_reads", "all_features"),
        |b| {
            b.iter(|| {
                let header = generated_view.header();
                let samples = generated_view.samples();
                let nested_headers = generated_view.headers();
                let config = generated_view.config();
                let memory = config.memory().expect("fixture selects Memory");
                black_box((
                    generated_view.sequence(),
                    generated_view.active(),
                    generated_view.priority(),
                    generated_view.name(),
                    generated_view.c_name().to_bytes(),
                    generated_view.wide().as_slice(),
                    generated_view.wide_c().as_slice(),
                    generated_view.token(),
                    header.version(),
                    header.producer().to_bytes(),
                    samples.get(1),
                    nested_headers
                        .get(1)
                        .expect("in bounds")
                        .producer()
                        .to_bytes(),
                    config.tag(),
                    memory.capacity(),
                    memory.enabled(),
                    generated_view.checksum(),
                ))
            })
        },
    );
    group.bench_function(
        criterion::BenchmarkId::new("handwritten_field_reads", "all_features"),
        |b| {
            b.iter(|| {
                let (wide, wide_len) = hand_view.wide();
                let (wide_c, wide_c_len) = hand_view.wide_c();
                let header = hand_view.header(44);
                let nested_header = hand_view.header(72);
                let samples = hand_view.samples();
                let config = hand_view.config();
                black_box((
                    hand_view.sequence(),
                    hand_view.active(),
                    hand_view.priority(),
                    hand_view.name(),
                    hand_view.c_name(),
                    wide,
                    wide_len,
                    wide_c,
                    wide_c_len,
                    hand_view.token(),
                    header,
                    samples[1],
                    nested_header,
                    config,
                    hand_view.checksum(),
                ))
            })
        },
    );

    group.bench_function(
        criterion::BenchmarkId::new("generated_copy_into", "all_features"),
        |b| b.iter(|| black_box(generated_view.copy_into())),
    );
    group.bench_function(
        criterion::BenchmarkId::new("handwritten_copy_into", "all_features"),
        |b| b.iter(|| black_box(hand_view.copy_into())),
    );

    group.bench_function(
        criterion::BenchmarkId::new("generated_field_mutation", "all_features"),
        |b| {
            b.iter_batched_ref(
                producer_bytes,
                |storage| {
                    generated_field_mutation(storage);
                    black_box(storage.0[0])
                },
                criterion::BatchSize::SmallInput,
            )
        },
    );
    group.bench_function(
        criterion::BenchmarkId::new("handwritten_field_mutation", "all_features"),
        |b| {
            b.iter_batched_ref(
                producer_bytes,
                |storage| {
                    handwritten_field_mutation(storage.as_bytes_mut());
                    black_box(storage.0[0])
                },
                criterion::BatchSize::SmallInput,
            )
        },
    );

    group.bench_function(
        criterion::BenchmarkId::new("generated_patch", "all_features"),
        |b| {
            b.iter_batched_ref(
                producer_bytes,
                |storage| {
                    generated_patch(storage);
                    black_box(storage.0[0])
                },
                criterion::BatchSize::SmallInput,
            )
        },
    );
    group.bench_function(
        criterion::BenchmarkId::new("handwritten_patch", "all_features"),
        |b| {
            b.iter_batched_ref(
                producer_bytes,
                |storage| {
                    handwritten_patch(storage.as_bytes_mut());
                    black_box(storage.0[0])
                },
                criterion::BatchSize::SmallInput,
            )
        },
    );

    group.finish();
}

fn zero_sentinel_options(c: &mut criterion::Criterion) {
    assert_eq!(OptionalBenchmark::SCHEMA_ALIGN, OPTIONAL_SCHEMA_ALIGN);
    assert_eq!(
        mem::align_of::<OptionProducerBytes>(),
        OPTIONAL_SCHEMA_ALIGN
    );
    assert_eq!(
        mem::size_of::<OptionProducerBytes>(),
        OptionalBenchmark::SCHEMA_SIZE
    );

    let all_none = option_producer_bytes();
    assert_all_none_options(&all_none);

    let mut present_snapshot = option_producer_bytes();
    set_present_options(&mut present_snapshot);
    assert_present_options(&present_snapshot);

    let mut cleared = present_snapshot;
    clear_present_options(&mut cleared);
    assert_eq!(cleared.as_bytes(), &REVIEWED_OPTION_NONE_BYTES);
    assert_all_none_options(&cleared);

    let complete_patch = complete_options_patch();
    let mut promoted = option_producer_bytes();
    promote_options(&mut promoted, &complete_patch);
    assert_present_options(&promoted);

    let mut group = c.benchmark_group("zero_sentinel_options");
    group.throughput(criterion::Throughput::Bytes(
        OptionalBenchmark::SCHEMA_SIZE as u64,
    ));

    let mut all_none_get = option_producer_bytes();
    group.bench_function(
        criterion::BenchmarkId::new("eager_access_get", "all_none"),
        |b| {
            b.iter(|| {
                let mut view =
                    OptionalBenchmark::access_mut(black_box(all_none_get.as_bytes_mut()))
                        .expect("reviewed all-zero optional producer bytes");
                let code_absent = view.maybe_code_mut().get().is_none();
                let record_absent = view.maybe_record_mut().get().is_none();
                let array_absent = view.maybe_array_mut().get().is_none();
                black_box((code_absent, record_absent, array_absent))
            })
        },
    );

    group.bench_function(
        criterion::BenchmarkId::new("set_some_then_clear", "enum_record_array"),
        |b| {
            b.iter_batched_ref(
                option_producer_bytes,
                |storage| {
                    set_present_options(storage);
                    let present = OptionalBenchmark::access(storage.as_bytes())
                        .expect("set options remain valid");
                    black_box(present.copy_into());

                    clear_present_options(storage);
                    let cleared = OptionalBenchmark::access(storage.as_bytes())
                        .expect("cleared options remain valid");
                    black_box((
                        cleared.maybe_code().is_none(),
                        cleared.maybe_record().is_none(),
                        cleared.maybe_array().is_none(),
                    ));
                },
                criterion::BatchSize::SmallInput,
            )
        },
    );

    group.bench_function(
        criterion::BenchmarkId::new("complete_tri_state_patch_promotion", "enum_record_array"),
        |b| {
            b.iter_batched_ref(
                option_producer_bytes,
                |storage| {
                    promote_options(storage, &complete_patch);
                    let promoted = OptionalBenchmark::access(storage.as_bytes())
                        .expect("promoted options remain valid");
                    black_box(promoted.copy_into())
                },
                criterion::BatchSize::SmallInput,
            )
        },
    );

    let present_view = OptionalBenchmark::access(present_snapshot.as_bytes())
        .expect("present snapshot remains valid");
    group.bench_function(
        criterion::BenchmarkId::new("copy_into", "present_snapshot"),
        |b| b.iter(|| black_box(present_view.copy_into())),
    );

    group.finish();
}

criterion::criterion_group!(benches, capabilities);
criterion::criterion_group!(option_benches, zero_sentinel_options);
criterion::criterion_main!(benches, option_benches);
