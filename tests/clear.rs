//! Record and string `clear()`: the exact bytes zeroed, the bytes left alone, and the
//! records the gate admits.
//!
//! Every clearable record below also asserts `ZERO_IS_VALID`, because the gate is an
//! under-approximation of that constant. The records `clear()` must reject are pinned
//! by the macro UI fixture `fail/81_clear_zero_invalid_record.rs`.

#![deny(warnings)]
#![allow(dead_code)]

use core::{ffi::CStr, ops::Range};

use widestring::{U16CStr, U16Str};
use zero_schema::{
    __private::{LogicalSchema, SchemaPatchType, WireTypeSupport},
    FieldKind, LayoutDescriptor, make_schema_buffer, schema_buffer, zero,
};
use zero_schema_cross_crate_child::{DirectChild, OptionalChild, OptionalCode};

#[zero]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Idle = 0,
    Busy = 2,
}

#[zero]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Level {
    Low = 1,
    High = 2,
}

/// Two internal padding bytes sit between `mode` and `count`.
#[zero]
#[derive(Debug)]
pub struct Leaf {
    pub flag: bool,
    pub mode: Mode,
    pub count: u32,
}

#[zero]
pub struct Block<'a> {
    pub id: u8,
    pub leaf: Leaf,
    pub leaves: [Leaf; 3],
    pub level: Option<Level>,
    #[zero(capacity = 8, len_type = u8)]
    pub text: &'a str,
    #[zero(capacity = 8)]
    pub c_text: &'a CStr,
    #[zero(capacity = 4, len_type = u8)]
    pub wide: &'a U16Str,
    #[zero(capacity = 5)]
    pub wide_c: &'a U16CStr,
    #[zero(capacity = 6)]
    pub names: [&'a CStr; 3],
    pub tail: u16,
}

const LEAD: usize = 8;
const TRAIL: usize = 8;
const SIZE: usize = Block::SCHEMA_SIZE;
const POISON: u8 = 0xa5;
const GUARD: u8 = 0xcc;

/// A `Block` between guard bytes, aligned for the root.
#[repr(align(8))]
struct Storage([u8; LEAD + SIZE + TRAIL]);

impl Storage {
    fn record(&self) -> &[u8] {
        &self.0[LEAD..LEAD + SIZE]
    }

    fn record_mut(&mut self) -> &mut [u8] {
        &mut self.0[LEAD..LEAD + SIZE]
    }
}

fn field(name: &str) -> &'static zero_schema::FieldDescriptor {
    Block::LAYOUT
        .fields()
        .iter()
        .find(|field| field.name() == name)
        .expect("declared field")
}

/// The field's span in `Storage` coordinates.
fn span(name: &str) -> Range<usize> {
    let field = field(name);
    LEAD + field.offset()..LEAD + field.offset() + field.size()
}

/// One array element's span in `Storage` coordinates.
fn element_span(name: &str, index: usize) -> Range<usize> {
    let field = field(name);
    let FieldKind::Array(array) = field.kind() else {
        panic!("{name} is a fixed array");
    };
    let start = LEAD + field.offset() + index * array.stride();
    start..start + array.stride()
}

fn poison(storage: &mut Storage, range: Range<usize>) -> usize {
    storage.0[range.clone()].fill(POISON);
    range.len()
}

/// The non-empty padding ranges of a layout, relative to the start of its record.
/// The descriptor also lists empty ranges between adjacent fields.
fn padding(layout: &'static LayoutDescriptor) -> impl Iterator<Item = Range<usize>> {
    layout
        .padding()
        .iter()
        .map(|range| range.start()..range.end())
        .filter(|range| !range.is_empty())
}

/// A valid, fully populated `Block` whose ignored bytes are nonzero: padding is
/// poisoned and every string was set long and then short, which keeps the tail.
fn populated() -> Storage {
    const { assert!(Block::ZERO_IS_VALID) };
    let mut storage = Storage([GUARD; LEAD + SIZE + TRAIL]);
    storage.record_mut().fill(0);
    {
        let mut block = Block::access_mut(storage.record_mut()).expect("zero-valid root");
        block.id_mut().set(0x11).unwrap();
        {
            let mut leaf = block.leaf_mut();
            leaf.flag_mut().set(true).unwrap();
            leaf.mode_mut().set(Mode::Busy).unwrap();
            leaf.count_mut().set(0x2222_2222).unwrap();
        }
        {
            let mut leaves = block.leaves_mut();
            for index in 0..3 {
                let mut leaf = leaves.get_mut(index).expect("declared element");
                leaf.flag_mut().set(true).unwrap();
                leaf.mode_mut().set(Mode::Busy).unwrap();
                leaf.count_mut().set(0x3333_3333).unwrap();
            }
        }
        block.level_mut().set(Some(Level::High)).unwrap();
        block.text_mut().set("abcdefgh").unwrap();
        block.text_mut().set("ab").unwrap();
        block.c_text_mut().set(c"abcdefg").unwrap();
        block.c_text_mut().set(c"ab").unwrap();
        block
            .wide_mut()
            .set(U16Str::from_slice(&[1, 2, 3, 4]))
            .unwrap();
        block.wide_mut().set(U16Str::from_slice(&[9])).unwrap();
        block
            .wide_c_mut()
            .set(U16CStr::from_slice(&[1, 2, 3, 4, 0]).unwrap())
            .unwrap();
        block
            .wide_c_mut()
            .set(U16CStr::from_slice(&[9, 0]).unwrap())
            .unwrap();
        {
            let mut names = block.names_mut();
            for index in 0..3 {
                let mut name = names.get_mut(index).expect("declared element");
                name.set(c"abcde").unwrap();
                name.set(c"x").unwrap();
            }
        }
        block.tail_mut().set(0x4444).unwrap();
    }

    // Padding is ignored storage, so poisoning it keeps the record valid.
    let mut poisoned = 0;
    for range in padding(Block::LAYOUT) {
        poisoned += poison(&mut storage, LEAD + range.start..LEAD + range.end);
    }
    for record in [span("leaf")]
        .into_iter()
        .chain((0..3).map(|index| element_span("leaves", index)))
    {
        for range in padding(Leaf::LAYOUT) {
            poisoned += poison(
                &mut storage,
                record.start + range.start..record.start + range.end,
            );
        }
    }
    assert!(poisoned > 0, "fixture has padding to poison");
    assert!(Block::access(storage.record()).is_ok());
    storage
}

/// Asserts that exactly `cleared` became zero and every other byte, including the
/// guard bytes around the record, is unchanged.
fn assert_cleared_exactly(before: &Storage, after: &Storage, cleared: Range<usize>) {
    assert!(
        before.0[cleared.clone()].iter().any(|byte| *byte != 0),
        "the span held data before the clear"
    );
    for index in 0..before.0.len() {
        if cleared.contains(&index) {
            assert_eq!(after.0[index], 0, "byte {index} inside the span");
        } else {
            assert_eq!(
                after.0[index], before.0[index],
                "byte {index} outside the span changed"
            );
        }
    }
}

/// Clears one string field through its handle and checks the byte contract.
/// `behind` is the offset in the field of the first byte behind the active value.
fn clear_field(name: &str, behind: usize, clear: impl FnOnce(&mut BlockMut<'_>)) -> Storage {
    let mut storage = populated();
    let before = Storage(storage.0);
    let range = span(name);
    assert!(
        before.0[range.start + behind..range.end]
            .iter()
            .any(|byte| *byte != 0),
        "{name} keeps data behind its active value after `set`"
    );
    {
        let mut block = Block::access_mut(storage.record_mut()).unwrap();
        clear(&mut block);
    }
    assert_cleared_exactly(&before, &storage, range);
    storage
}

#[test]
fn record_clear_zeroes_the_complete_span_including_padding_and_nothing_beyond() {
    let mut storage = populated();
    let before = Storage(storage.0);
    {
        let (mut block, rest) =
            Block::access_mut_prefix(&mut storage.0[LEAD..]).expect("populated root is valid");
        assert_eq!(rest.len(), TRAIL);
        block.clear();
    }

    assert_cleared_exactly(&before, &storage, LEAD..LEAD + SIZE);
    let view = Block::access(storage.record()).expect("a cleared zero-valid record stays valid");
    assert_eq!(
        (view.id(), view.tail(), view.level()),
        (0, 0, None),
        "scalars read zero and Option fields are absent"
    );
    assert_eq!(
        (view.leaf().flag(), view.leaf().mode(), view.leaf().count()),
        (false, Mode::Idle, 0)
    );
    for index in 0..3 {
        let leaf = view.leaves().get(index).unwrap();
        assert_eq!(
            (leaf.flag(), leaf.mode(), leaf.count()),
            (false, Mode::Idle, 0)
        );
        assert!(view.names().get(index).unwrap().to_bytes().is_empty());
    }
    assert_eq!(view.text(), "");
    assert!(view.c_text().to_bytes().is_empty());
    assert!(view.wide().is_empty());
    assert!(view.wide_c().is_empty());
}

#[test]
fn string_clear_zeroes_prefix_capacity_and_the_tail_behind_the_value_for_every_form() {
    // Length-prefixed UTF-8: one prefix byte, then two active bytes.
    let storage = clear_field("text", 1 + 2, |block| block.text_mut().clear());
    assert_eq!(Block::access(storage.record()).unwrap().text(), "");

    // Terminated C string: the tail sits behind the terminator.
    let storage = clear_field("c_text", 3, |block| block.c_text_mut().clear());
    assert!(
        Block::access(storage.record())
            .unwrap()
            .c_text()
            .to_bytes()
            .is_empty()
    );

    // Length-prefixed wide string: prefix, alignment gap, one active unit.
    let storage = clear_field("wide", 2 + 2, |block| block.wide_mut().clear());
    assert!(Block::access(storage.record()).unwrap().wide().is_empty());

    // Terminated wide string: one active unit and its terminator.
    let storage = clear_field("wide_c", 2 * 2, |block| block.wide_c_mut().clear());
    assert!(Block::access(storage.record()).unwrap().wide_c().is_empty());
}

#[test]
fn string_clear_on_an_array_element_leaves_the_other_elements_alone() {
    let mut storage = populated();
    let before = Storage(storage.0);
    let range = element_span("names", 1);
    assert!(
        before.0[range.start + 2..range.end]
            .iter()
            .any(|byte| *byte != 0),
        "the element keeps data behind its terminator after `set`"
    );
    {
        let mut block = Block::access_mut(storage.record_mut()).unwrap();
        let mut names = block.names_mut();
        let mut name = names.get_mut(1).expect("declared element");
        name.clear();
        assert!(name.get().to_bytes().is_empty());
    }

    assert_cleared_exactly(&before, &storage, range);
    let view = Block::access(storage.record()).unwrap();
    assert_eq!(view.names().get(0).unwrap().to_bytes(), b"x");
    assert_eq!(view.names().get(2).unwrap().to_bytes(), b"x");
}

#[test]
fn a_cleared_string_accepts_a_new_value() {
    let mut storage = populated();
    let mut block = Block::access_mut(storage.record_mut()).unwrap();
    let mut text = block.text_mut();
    text.clear();
    text.set("full capacity").expect_err("capacity is still 8");
    text.set("12345678")
        .expect("all capacity is writable again");
    assert_eq!(text.get(), "12345678");
}

#[test]
fn record_clear_on_a_nested_record_and_an_array_element_stays_inside_that_record() {
    let mut storage = populated();
    let before = Storage(storage.0);
    let leaf_span = span("leaf");
    assert_eq!(leaf_span.len(), Leaf::SCHEMA_SIZE);
    for range in padding(Leaf::LAYOUT) {
        assert!(
            before.0[leaf_span.start + range.start..leaf_span.start + range.end]
                .iter()
                .all(|byte| *byte == POISON),
            "the nested record's internal padding starts poisoned"
        );
    }
    {
        let mut block = Block::access_mut(storage.record_mut()).unwrap();
        block.leaf_mut().clear();
    }
    assert_cleared_exactly(&before, &storage, leaf_span);

    let mut storage = populated();
    let before = Storage(storage.0);
    let element = element_span("leaves", 1);
    assert_eq!(element.len(), Leaf::SCHEMA_SIZE);
    {
        let mut block = Block::access_mut(storage.record_mut()).unwrap();
        let mut leaves = block.leaves_mut();
        leaves.get_mut(1).expect("declared element").clear();
    }
    assert_cleared_exactly(&before, &storage, element);
    let view = Block::access(storage.record()).unwrap();
    assert_eq!(view.leaves().get(0).unwrap().count(), 0x3333_3333);
    assert_eq!(view.leaves().get(1).unwrap().count(), 0);
    assert_eq!(view.leaves().get(2).unwrap().count(), 0x3333_3333);
}

// The same operations across a crate boundary: `DirectChild` and `OptionalChild` are
// generated in another crate, and `Option<OptionalChild>` is what keeps this root
// zero-valid even though `OptionalChild` has no zero value.
#[zero]
pub struct ForeignRoot {
    pub before: u8,
    pub direct: DirectChild,
    pub many: [DirectChild; 2],
    pub optional: Option<OptionalChild>,
    pub after: u8,
}

fn foreign_storage() -> schema_buffer!(ForeignRoot) {
    const { assert!(ForeignRoot::ZERO_IS_VALID) };
    let mut storage = make_schema_buffer!(ForeignRoot);
    {
        let mut root = ForeignRoot::access_mut(storage.as_bytes_mut()).expect("zero-valid root");
        root.before_mut().set(0x61).unwrap();
        root.direct_mut().valid_mut().set(true).unwrap();
        root.direct_mut().value_mut().set(0x1234).unwrap();
        root.many_mut()
            .get_mut(1)
            .unwrap()
            .value_mut()
            .set(0x5678)
            .unwrap();
        root.optional_mut()
            .set(Some(OptionalChild {
                code: OptionalCode::Two,
                payload: 9,
            }))
            .unwrap();
        root.after_mut().set(0x62).unwrap();
    }
    storage
}

fn foreign_field(name: &str) -> Range<usize> {
    let field = ForeignRoot::LAYOUT
        .fields()
        .iter()
        .find(|field| field.name() == name)
        .expect("declared field");
    field.offset()..field.offset() + field.size()
}

/// Asserts that exactly `cleared` became zero in `after`.
fn assert_foreign_cleared_exactly(before: &[u8], after: &[u8], cleared: Range<usize>) {
    assert!(before[cleared.clone()].iter().any(|byte| *byte != 0));
    for (index, (old, new)) in before.iter().zip(after).enumerate() {
        if cleared.contains(&index) {
            assert_eq!(*new, 0, "byte {index} inside the cleared record");
        } else {
            assert_eq!(new, old, "byte {index} outside the cleared record changed");
        }
    }
}

#[test]
fn clear_reaches_records_generated_in_another_crate() {
    let before = foreign_storage();
    let before = before.as_bytes();

    let mut storage = foreign_storage();
    ForeignRoot::access_mut(storage.as_bytes_mut())
        .unwrap()
        .direct_mut()
        .clear();
    let direct = foreign_field("direct");
    assert_eq!(direct.len(), DirectChild::SCHEMA_SIZE);
    assert_foreign_cleared_exactly(before, storage.as_bytes(), direct);

    let mut storage = foreign_storage();
    ForeignRoot::access_mut(storage.as_bytes_mut())
        .unwrap()
        .many_mut()
        .get_mut(1)
        .unwrap()
        .clear();
    let many = foreign_field("many");
    let element = many.start + DirectChild::SCHEMA_SIZE..many.end;
    assert_eq!(element.len(), DirectChild::SCHEMA_SIZE);
    assert_foreign_cleared_exactly(before, storage.as_bytes(), element);

    let mut storage = foreign_storage();
    ForeignRoot::access_mut(storage.as_bytes_mut())
        .unwrap()
        .clear();
    assert!(storage.as_bytes().iter().all(|byte| *byte == 0));
    let view = ForeignRoot::access(storage.as_bytes()).unwrap();
    assert!(view.optional().is_none());
    assert!(!view.direct().valid());
}

#[zero]
pub struct Holder<T>
where
    T: WireTypeSupport
        + SchemaPatchType
        + for<'view> LogicalSchema<'view>
        + core::fmt::Debug
        + 'static,
{
    pub marker: u8,
    pub child: T,
}

#[test]
fn clear_follows_a_generic_childs_zero_validity() {
    const { assert!(Holder::<Leaf>::ZERO_IS_VALID) };
    let mut storage = make_schema_buffer!(Holder<Leaf>);
    {
        let mut holder = Holder::<Leaf>::access_mut(storage.as_bytes_mut()).unwrap();
        holder.marker_mut().set(7).unwrap();
        holder.child_mut().count_mut().set(9).unwrap();
        holder.child_mut().clear();
    }
    let view = Holder::<Leaf>::access(storage.as_bytes()).unwrap();
    assert_eq!((view.marker(), view.child().count()), (7, 0));

    Holder::<Leaf>::access_mut(storage.as_bytes_mut())
        .unwrap()
        .clear();
    assert!(storage.as_bytes().iter().all(|byte| *byte == 0));
}

// A tagged payload keeps its record clearable only when the zero tag is claimed and
// every variant payload is zero-valid; `Message` satisfies both.
#[zero]
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    Empty = 0,
    Data = 1,
}

#[zero]
pub struct Reading {
    pub value: u32,
}

#[zero]
pub enum Message {
    #[zero(tag = Kind::Empty)]
    Empty,
    #[zero(tag = Kind::Data)]
    Data(Reading),
}

#[zero]
pub struct Envelope {
    pub kind: Kind,
    #[zero(tag_field = kind)]
    pub message: Message,
    pub after: u8,
}

#[test]
fn clear_resets_the_external_tag_and_its_payload_together() {
    const { assert!(Envelope::ZERO_IS_VALID) };
    let mut storage = make_schema_buffer!(Envelope);
    let fields = Envelope::LAYOUT.fields();
    let (kind, message) = (fields[0], fields[1]);
    {
        let bytes = storage.as_bytes_mut();
        bytes[kind.offset()] = Kind::Data as u8;
        bytes[message.offset()..message.offset() + 4]
            .copy_from_slice(&0x0102_0304_u32.to_ne_bytes());
        for padding in Envelope::LAYOUT.padding() {
            bytes[padding.start()..padding.end()].fill(POISON);
        }
    }
    {
        let view = Envelope::access(storage.as_bytes()).expect("Data is selected");
        assert_eq!(view.message().tag(), Kind::Data);
        assert_eq!(view.message().data().unwrap().value(), 0x0102_0304);
    }

    Envelope::access_mut(storage.as_bytes_mut())
        .unwrap()
        .clear();
    assert!(storage.as_bytes().iter().all(|byte| *byte == 0));
    let view = Envelope::access(storage.as_bytes()).expect("the zero tag selects Empty");
    assert_eq!(view.kind(), Kind::Empty);
    assert_eq!(view.message().tag(), Kind::Empty);
    assert!(view.message().empty().is_some());
}
