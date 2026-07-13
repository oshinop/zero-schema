//! Immutable schema layout metadata.

/// Describes the complete wire layout of a schema type.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LayoutDescriptor {
    name: &'static str,
    kind: TypeKind,
    size: usize,
    align: usize,
    stride: usize,
    padding_policy: PaddingPolicy,
    padding: &'static [ByteRange],
    fields: &'static [FieldDescriptor],
    enum_values: &'static [EnumValueDescriptor],
    variants: &'static [VariantDescriptor],
}

impl LayoutDescriptor {
    /// Constructs a layout descriptor for generated code.
    #[doc(hidden)]
    #[allow(clippy::too_many_arguments)]
    pub const fn __new(
        name: &'static str,
        kind: TypeKind,
        size: usize,
        align: usize,
        stride: usize,
        padding_policy: PaddingPolicy,
        padding: &'static [ByteRange],
        fields: &'static [FieldDescriptor],
        enum_values: &'static [EnumValueDescriptor],
        variants: &'static [VariantDescriptor],
    ) -> Self {
        Self {
            name,
            kind,
            size,
            align,
            stride,
            padding_policy,
            padding,
            fields,
            enum_values,
            variants,
        }
    }

    pub const fn name(&self) -> &'static str {
        self.name
    }
    pub const fn kind(&self) -> TypeKind {
        self.kind
    }
    pub const fn size(&self) -> usize {
        self.size
    }
    pub const fn align(&self) -> usize {
        self.align
    }
    pub const fn stride(&self) -> usize {
        self.stride
    }
    pub const fn padding_policy(&self) -> PaddingPolicy {
        self.padding_policy
    }
    pub const fn padding(&self) -> &'static [ByteRange] {
        self.padding
    }
    pub const fn fields(&self) -> &'static [FieldDescriptor] {
        self.fields
    }
    pub const fn enum_values(&self) -> &'static [EnumValueDescriptor] {
        self.enum_values
    }
    pub const fn variants(&self) -> &'static [VariantDescriptor] {
        self.variants
    }
}

/// The top-level form of a schema type.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum TypeKind {
    Struct,
    ScalarEnum {
        repr: IntegerRepr,
        endian: Endian,
    },
    TaggedUnion {
        tag_layout: &'static LayoutDescriptor,
        tag_offset: usize,
        payload_offset: usize,
        payload_size: usize,
        payload_align: usize,
        tail: TailPolicy,
    },
}

/// Describes one declared field and its wire placement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FieldDescriptor {
    name: &'static str,
    declaration_index: usize,
    offset: usize,
    size: usize,
    align: usize,
    kind: FieldKind,
}

impl FieldDescriptor {
    #[doc(hidden)]
    pub const fn __new(
        name: &'static str,
        declaration_index: usize,
        offset: usize,
        size: usize,
        align: usize,
        kind: FieldKind,
    ) -> Self {
        Self {
            name,
            declaration_index,
            offset,
            size,
            align,
            kind,
        }
    }

    pub const fn name(&self) -> &'static str {
        self.name
    }
    pub const fn declaration_index(&self) -> usize {
        self.declaration_index
    }
    pub const fn offset(&self) -> usize {
        self.offset
    }
    pub const fn size(&self) -> usize {
        self.size
    }
    pub const fn align(&self) -> usize {
        self.align
    }
    pub const fn kind(&self) -> FieldKind {
        self.kind
    }
}

/// The logical interpretation of a field's wire storage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum FieldKind {
    Primitive {
        primitive: PrimitiveKind,
        endian: Endian,
    },
    Bool,
    String(StringDescriptor),
    FixedBytes {
        length: usize,
    },
    Schema {
        layout: &'static LayoutDescriptor,
    },
    ExternalTaggedUnion {
        layout: &'static LayoutDescriptor,
        tag_field: &'static str,
    },
}

/// Describes string storage within a field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StringDescriptor {
    encoding: StringEncoding,
    unit_endian: Option<Endian>,
    capacity: usize,
    length: Option<LengthDescriptor>,
    data_offset: usize,
    tail: TailPolicy,
}

impl StringDescriptor {
    #[doc(hidden)]
    pub const fn __new(
        encoding: StringEncoding,
        unit_endian: Option<Endian>,
        capacity: usize,
        length: Option<LengthDescriptor>,
        data_offset: usize,
        tail: TailPolicy,
    ) -> Self {
        Self {
            encoding,
            unit_endian,
            capacity,
            length,
            data_offset,
            tail,
        }
    }

    pub const fn encoding(&self) -> StringEncoding {
        self.encoding
    }
    pub const fn unit_endian(&self) -> Option<Endian> {
        self.unit_endian
    }
    pub const fn capacity(&self) -> usize {
        self.capacity
    }
    pub const fn length(&self) -> Option<LengthDescriptor> {
        self.length
    }
    pub const fn data_offset(&self) -> usize {
        self.data_offset
    }
    pub const fn tail(&self) -> TailPolicy {
        self.tail
    }
}

/// Describes a length prefix within string storage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LengthDescriptor {
    repr: LengthRepr,
    endian: Endian,
    offset: usize,
}

impl LengthDescriptor {
    #[doc(hidden)]
    pub const fn __new(repr: LengthRepr, endian: Endian, offset: usize) -> Self {
        Self {
            repr,
            endian,
            offset,
        }
    }

    pub const fn repr(&self) -> LengthRepr {
        self.repr
    }
    pub const fn endian(&self) -> Endian {
        self.endian
    }
    pub const fn offset(&self) -> usize {
        self.offset
    }
}

/// Describes one declared scalar-enum value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EnumValueDescriptor {
    name: &'static str,
    raw_value: u64,
}

impl EnumValueDescriptor {
    #[doc(hidden)]
    pub const fn __new(name: &'static str, raw_value: u64) -> Self {
        Self { name, raw_value }
    }

    pub const fn name(&self) -> &'static str {
        self.name
    }
    pub const fn raw_value(&self) -> u64 {
        self.raw_value
    }
}

/// Describes one tagged-union variant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VariantDescriptor {
    name: &'static str,
    raw_tag: u64,
    payload: Option<&'static LayoutDescriptor>,
    payload_size: usize,
    payload_align: usize,
}

impl VariantDescriptor {
    #[doc(hidden)]
    pub const fn __new(
        name: &'static str,
        raw_tag: u64,
        payload: Option<&'static LayoutDescriptor>,
        payload_size: usize,
        payload_align: usize,
    ) -> Self {
        Self {
            name,
            raw_tag,
            payload,
            payload_size,
            payload_align,
        }
    }

    pub const fn name(&self) -> &'static str {
        self.name
    }
    pub const fn raw_tag(&self) -> u64 {
        self.raw_tag
    }
    pub const fn payload(&self) -> Option<&'static LayoutDescriptor> {
        self.payload
    }
    pub const fn payload_size(&self) -> usize {
        self.payload_size
    }
    pub const fn payload_align(&self) -> usize {
        self.payload_align
    }
}

/// A half-open byte range relative to its owning wire layout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ByteRange {
    start: usize,
    end: usize,
}

impl ByteRange {
    /// Constructs a range without validating its bounds.
    #[doc(hidden)]
    pub const fn __new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub const fn start(&self) -> usize {
        self.start
    }
    pub const fn end(&self) -> usize {
        self.end
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum PrimitiveKind {
    U8,
    I8,
    U16,
    I16,
    U32,
    I32,
    U64,
    I64,
    F32,
    F64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum IntegerRepr {
    U8,
    U16,
    U32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum LengthRepr {
    U8,
    U16,
    U32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Endian {
    Native,
    Little,
    Big,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum StringEncoding {
    Utf8,
    CBytes,
    U16,
    U16C,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum PaddingPolicy {
    Ignore,
    Zero,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum TailPolicy {
    Ignore,
    Zero,
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHILD: LayoutDescriptor = LayoutDescriptor::__new(
        "Child",
        TypeKind::ScalarEnum {
            repr: IntegerRepr::U16,
            endian: Endian::Big,
        },
        2,
        2,
        2,
        PaddingPolicy::Ignore,
        &[],
        &[],
        &[EnumValueDescriptor::__new("Ready", 7)],
        &[],
    );
    const LENGTH: LengthDescriptor = LengthDescriptor::__new(LengthRepr::U8, Endian::Little, 0);
    const STRING: StringDescriptor = StringDescriptor::__new(
        StringEncoding::Utf8,
        None,
        8,
        Some(LENGTH),
        1,
        TailPolicy::Zero,
    );
    const FIELDS: [FieldDescriptor; 6] = [
        FieldDescriptor::__new(
            "number",
            0,
            0,
            4,
            4,
            FieldKind::Primitive {
                primitive: PrimitiveKind::U32,
                endian: Endian::Native,
            },
        ),
        FieldDescriptor::__new("flag", 1, 4, 1, 1, FieldKind::Bool),
        FieldDescriptor::__new("name", 2, 5, 9, 1, FieldKind::String(STRING)),
        FieldDescriptor::__new("bytes", 3, 14, 3, 1, FieldKind::FixedBytes { length: 3 }),
        FieldDescriptor::__new("child", 4, 18, 2, 2, FieldKind::Schema { layout: &CHILD }),
        FieldDescriptor::__new(
            "payload",
            5,
            20,
            2,
            2,
            FieldKind::ExternalTaggedUnion {
                layout: &CHILD,
                tag_field: "child",
            },
        ),
    ];
    const PADDING: [ByteRange; 1] = [ByteRange::__new(17, 18)];
    const ROOT: LayoutDescriptor = LayoutDescriptor::__new(
        "Root",
        TypeKind::Struct,
        22,
        4,
        24,
        PaddingPolicy::Zero,
        &PADDING,
        &FIELDS,
        &[],
        &[],
    );

    #[test]
    fn record_constructors_expose_every_field() {
        assert_eq!(ROOT.name(), "Root");
        assert_eq!(ROOT.kind(), TypeKind::Struct);
        assert_eq!((ROOT.size(), ROOT.align(), ROOT.stride()), (22, 4, 24));
        assert_eq!(ROOT.padding_policy(), PaddingPolicy::Zero);
        assert_eq!(ROOT.padding(), &PADDING);
        assert_eq!(ROOT.fields(), &FIELDS);
        assert!(ROOT.enum_values().is_empty());
        assert!(ROOT.variants().is_empty());

        let field = ROOT.fields()[2];
        assert_eq!(field.name(), "name");
        assert_eq!(field.declaration_index(), 2);
        assert_eq!((field.offset(), field.size(), field.align()), (5, 9, 1));
        assert_eq!(field.kind(), FieldKind::String(STRING));
        assert_eq!(STRING.encoding(), StringEncoding::Utf8);
        assert_eq!(STRING.unit_endian(), None);
        assert_eq!((STRING.capacity(), STRING.data_offset()), (8, 1));
        assert_eq!(STRING.length(), Some(LENGTH));
        assert_eq!(STRING.tail(), TailPolicy::Zero);
        assert_eq!(LENGTH.repr(), LengthRepr::U8);
        assert_eq!(LENGTH.endian(), Endian::Little);
        assert_eq!(LENGTH.offset(), 0);

        let value = CHILD.enum_values()[0];
        assert_eq!((value.name(), value.raw_value()), ("Ready", 7));
    }

    #[test]
    fn variant_and_tagged_union_metadata_round_trip() {
        const VARIANTS: [VariantDescriptor; 2] = [
            VariantDescriptor::__new("Empty", 0, None, 0, 1),
            VariantDescriptor::__new("Child", 7, Some(&CHILD), 2, 2),
        ];
        const UNION: LayoutDescriptor = LayoutDescriptor::__new(
            "Union",
            TypeKind::TaggedUnion {
                tag_layout: &CHILD,
                tag_offset: 0,
                payload_offset: 2,
                payload_size: 2,
                payload_align: 2,
                tail: TailPolicy::Ignore,
            },
            4,
            2,
            4,
            PaddingPolicy::Ignore,
            &[],
            &[],
            &[],
            &VARIANTS,
        );
        let variant = UNION.variants()[1];
        assert_eq!(variant.name(), "Child");
        assert_eq!(variant.raw_tag(), 7);
        assert_eq!(variant.payload(), Some(&CHILD));
        assert_eq!((variant.payload_size(), variant.payload_align()), (2, 2));
        assert!(UNION.enum_values().is_empty());
        assert!(matches!(
            UNION.kind(),
            TypeKind::TaggedUnion {
                tail: TailPolicy::Ignore,
                ..
            }
        ));
    }

    #[test]
    fn byte_range_constructor_does_not_validate() {
        let reversed = ByteRange::__new(9, 3);
        assert_eq!((reversed.start(), reversed.end()), (9, 3));
    }

    #[test]
    fn every_enum_variant_is_available() {
        let primitives = [
            PrimitiveKind::U8,
            PrimitiveKind::I8,
            PrimitiveKind::U16,
            PrimitiveKind::I16,
            PrimitiveKind::U32,
            PrimitiveKind::I32,
            PrimitiveKind::U64,
            PrimitiveKind::I64,
            PrimitiveKind::F32,
            PrimitiveKind::F64,
        ];
        assert_eq!(primitives.len(), 10);
        assert_eq!(
            [IntegerRepr::U8, IntegerRepr::U16, IntegerRepr::U32].len(),
            3
        );
        assert_eq!([LengthRepr::U8, LengthRepr::U16, LengthRepr::U32].len(), 3);
        assert_eq!([Endian::Native, Endian::Little, Endian::Big].len(), 3);
        assert_eq!(
            [
                StringEncoding::Utf8,
                StringEncoding::CBytes,
                StringEncoding::U16,
                StringEncoding::U16C
            ]
            .len(),
            4
        );
        assert_eq!([PaddingPolicy::Ignore, PaddingPolicy::Zero].len(), 2);
        assert_eq!([TailPolicy::Ignore, TailPolicy::Zero].len(), 2);
    }
}
