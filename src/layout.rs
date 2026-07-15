//! Immutable diagnostic metadata for compiler-derived schema layouts.

/// Describes the complete wire layout of a schema type.
///
/// This descriptor is diagnostic metadata only. It does not validate or access
/// backing storage; generated access code uses compiler-checked wire layouts
/// and checked byte ranges directly.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LayoutDescriptor {
    name: &'static str,
    kind: TypeKind,
    size: usize,
    align: usize,
    stride: usize,
    padding: &'static [ByteRange],
    fields: &'static [FieldDescriptor],
    enum_values: &'static [EnumValueDescriptor],
    variants: &'static [VariantDescriptor],
}

impl LayoutDescriptor {
    /// Constructs diagnostic layout metadata for generated code.
    #[doc(hidden)]
    #[allow(clippy::too_many_arguments)]
    pub const fn __new(
        name: &'static str,
        kind: TypeKind,
        size: usize,
        align: usize,
        stride: usize,
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

    /// Returns compiler-derived gaps between fields and trailing storage.
    ///
    /// Padding is never a validation or initialization policy.
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
    /// A logical tagged payload declaration.
    ///
    /// Its physical tag belongs to a containing
    /// [`FieldKind::ExternalTaggedUnion`] field, not to this descriptor.
    TaggedUnion,
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
    optional: bool,
}

impl FieldDescriptor {
    /// Constructs field metadata for generated code.
    #[doc(hidden)]
    pub const fn __new(
        name: &'static str,
        declaration_index: usize,
        offset: usize,
        size: usize,
        align: usize,
        kind: FieldKind,
        optional: bool,
    ) -> Self {
        Self {
            name,
            declaration_index,
            offset,
            size,
            align,
            kind,
            optional,
        }
    }

    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Returns the source declaration order, independent of physical offset.
    pub const fn declaration_index(&self) -> usize {
        self.declaration_index
    }

    /// Returns the compiler-derived byte offset in the owning wire type.
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

    /// Whether this field uses the zero-sentinel optional protocol.
    ///
    /// Its `offset..offset + size` storage span, including field-local
    /// alignment padding, is presence-significant. `kind` remains the inner
    /// field kind.
    pub const fn is_optional(&self) -> bool {
        self.optional
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
    ScalarEnum {
        layout: &'static LayoutDescriptor,
    },
    String(StringDescriptor),
    FixedBytes {
        length: usize,
    },
    Array(ArrayDescriptor),
    Schema {
        layout: &'static LayoutDescriptor,
    },
    ExternalTaggedUnion {
        payload: &'static LayoutDescriptor,
        tag: ExternalTagDescriptor,
    },
}

/// Describes the element storage of a fixed-size array.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ArrayElementKind {
    Primitive {
        primitive: PrimitiveKind,
        endian: Endian,
    },
    Bool,
    ScalarEnum {
        layout: &'static LayoutDescriptor,
    },
    Schema {
        layout: &'static LayoutDescriptor,
    },
}

/// Describes a fixed-size array field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArrayDescriptor {
    element: ArrayElementKind,
    length: usize,
    stride: usize,
}

impl ArrayDescriptor {
    /// Constructs array metadata for generated code.
    #[doc(hidden)]
    pub const fn __new(element: ArrayElementKind, length: usize, stride: usize) -> Self {
        Self {
            element,
            length,
            stride,
        }
    }

    pub const fn element(&self) -> ArrayElementKind {
        self.element
    }

    /// Returns the fixed array length (`N`).
    pub const fn length(&self) -> usize {
        self.length
    }

    /// Returns the compiler-derived wire stride of one element.
    pub const fn stride(&self) -> usize {
        self.stride
    }
}

/// Describes the sibling scalar-enum field that externally tags a payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExternalTagDescriptor {
    field_name: &'static str,
    offset: usize,
    layout: &'static LayoutDescriptor,
}

impl ExternalTagDescriptor {
    /// Constructs external-tag metadata for generated code.
    #[doc(hidden)]
    pub const fn __new(
        field_name: &'static str,
        offset: usize,
        layout: &'static LayoutDescriptor,
    ) -> Self {
        Self {
            field_name,
            offset,
            layout,
        }
    }

    pub const fn field_name(&self) -> &'static str {
        self.field_name
    }

    /// Returns the compiler-derived sibling-field offset in the parent wire.
    pub const fn offset(&self) -> usize {
        self.offset
    }

    pub const fn layout(&self) -> &'static LayoutDescriptor {
        self.layout
    }
}

/// Describes string storage within a field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StringDescriptor {
    encoding: StringEncoding,
    capacity: usize,
    length: Option<LengthDescriptor>,
    data_offset: usize,
}

impl StringDescriptor {
    /// Constructs string metadata for generated code.
    #[doc(hidden)]
    pub const fn __new(
        encoding: StringEncoding,
        capacity: usize,
        length: Option<LengthDescriptor>,
        data_offset: usize,
    ) -> Self {
        Self {
            encoding,
            capacity,
            length,
            data_offset,
        }
    }

    pub const fn encoding(&self) -> StringEncoding {
        self.encoding
    }

    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    /// Returns the optional length-prefix representation, endian, and offset.
    pub const fn length(&self) -> Option<LengthDescriptor> {
        self.length
    }

    pub const fn data_offset(&self) -> usize {
        self.data_offset
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
    /// Constructs length-prefix metadata for generated code.
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
    /// Constructs scalar-enum value metadata for generated code.
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

/// Describes one tagged-payload variant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VariantDescriptor {
    name: &'static str,
    raw_tag: u64,
    payload: &'static LayoutDescriptor,
    payload_size: usize,
    payload_align: usize,
}

impl VariantDescriptor {
    /// Constructs variant metadata for generated code.
    ///
    /// Unit variants use their generated nonzero private payload layout.
    #[doc(hidden)]
    pub const fn __new(
        name: &'static str,
        raw_tag: u64,
        payload: &'static LayoutDescriptor,
        payload_size: usize,
        payload_align: usize,
    ) -> Self {
        assert!(payload_size != 0, "tagged payload metadata must be nonzero");
        assert!(
            payload_align != 0,
            "tagged payload metadata must be aligned"
        );
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

    pub const fn payload(&self) -> &'static LayoutDescriptor {
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

#[cfg(test)]
mod tests {
    use super::*;

    const SCALAR_ENUM: LayoutDescriptor = LayoutDescriptor::__new(
        "State",
        TypeKind::ScalarEnum {
            repr: IntegerRepr::U16,
            endian: Endian::Big,
        },
        2,
        2,
        2,
        &[],
        &[],
        &[
            EnumValueDescriptor::__new("Ready", 7),
            EnumValueDescriptor::__new("Stopped", 9),
        ],
        &[],
    );
    const UNIT_PAYLOAD: LayoutDescriptor =
        LayoutDescriptor::__new("NonePayload", TypeKind::Struct, 1, 1, 1, &[], &[], &[], &[]);
    const PAYLOAD_RECORD: LayoutDescriptor =
        LayoutDescriptor::__new("DataPayload", TypeKind::Struct, 4, 4, 4, &[], &[], &[], &[]);
    const VARIANTS: [VariantDescriptor; 2] = [
        VariantDescriptor::__new("None", 0, &UNIT_PAYLOAD, 1, 1),
        VariantDescriptor::__new("Data", 7, &PAYLOAD_RECORD, 4, 4),
    ];
    const PAYLOAD: LayoutDescriptor = LayoutDescriptor::__new(
        "Payload",
        TypeKind::TaggedUnion,
        4,
        4,
        4,
        &[],
        &[],
        &[],
        &VARIANTS,
    );
    const LENGTH: LengthDescriptor = LengthDescriptor::__new(LengthRepr::U8, Endian::Little, 0);
    const STRING: StringDescriptor =
        StringDescriptor::__new(StringEncoding::Utf8, 8, Some(LENGTH), 1);
    const ARRAY: ArrayDescriptor = ArrayDescriptor::__new(
        ArrayElementKind::Primitive {
            primitive: PrimitiveKind::U32,
            endian: Endian::Native,
        },
        3,
        4,
    );
    const TAG: ExternalTagDescriptor = ExternalTagDescriptor::__new("state", 5, &SCALAR_ENUM);
    const FIELDS: [FieldDescriptor; 8] = [
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
            false,
        ),
        FieldDescriptor::__new("flag", 1, 4, 1, 1, FieldKind::Bool, false),
        FieldDescriptor::__new(
            "state",
            2,
            5,
            2,
            1,
            FieldKind::ScalarEnum {
                layout: &SCALAR_ENUM,
            },
            true,
        ),
        FieldDescriptor::__new("name", 3, 7, 9, 1, FieldKind::String(STRING), false),
        FieldDescriptor::__new(
            "bytes",
            4,
            16,
            3,
            1,
            FieldKind::FixedBytes { length: 3 },
            false,
        ),
        FieldDescriptor::__new("samples", 5, 20, 12, 4, FieldKind::Array(ARRAY), false),
        FieldDescriptor::__new(
            "child",
            6,
            32,
            4,
            4,
            FieldKind::Schema {
                layout: &PAYLOAD_RECORD,
            },
            false,
        ),
        FieldDescriptor::__new(
            "payload",
            7,
            36,
            4,
            4,
            FieldKind::ExternalTaggedUnion {
                payload: &PAYLOAD,
                tag: TAG,
            },
            false,
        ),
    ];
    const PADDING: [ByteRange; 2] = [ByteRange::__new(19, 20), ByteRange::__new(40, 44)];
    const ROOT: LayoutDescriptor = LayoutDescriptor::__new(
        "Root",
        TypeKind::Struct,
        44,
        4,
        44,
        &PADDING,
        &FIELDS,
        &[],
        &[],
    );

    #[test]
    fn constructors_expose_every_descriptor_form() {
        assert_eq!(ROOT.name(), "Root");
        assert_eq!(ROOT.kind(), TypeKind::Struct);
        assert_eq!((ROOT.size(), ROOT.align(), ROOT.stride()), (44, 4, 44));
        assert_eq!(ROOT.padding(), &PADDING);
        assert!(ROOT.enum_values().is_empty());
        assert!(ROOT.variants().is_empty());

        let primitive = ROOT.fields()[0];
        assert_eq!(primitive.declaration_index(), 0);
        assert_eq!(
            (primitive.offset(), primitive.size(), primitive.align()),
            (0, 4, 4)
        );
        assert!(matches!(
            primitive.kind(),
            FieldKind::Primitive {
                primitive: PrimitiveKind::U32,
                endian: Endian::Native,
            }
        ));
        assert_eq!(ROOT.fields()[1].kind(), FieldKind::Bool);
        assert!(!primitive.is_optional());
        assert!(!ROOT.fields()[1].is_optional());
        assert!(ROOT.fields()[2].is_optional());
        assert_eq!(
            ROOT.fields()[2].kind(),
            FieldKind::ScalarEnum {
                layout: &SCALAR_ENUM,
            }
        );
        assert_eq!(ROOT.fields()[3].kind(), FieldKind::String(STRING));
        assert_eq!(ROOT.fields()[4].kind(), FieldKind::FixedBytes { length: 3 });
        assert_eq!(ROOT.fields()[5].kind(), FieldKind::Array(ARRAY));
        assert_eq!(
            ROOT.fields()[6].kind(),
            FieldKind::Schema {
                layout: &PAYLOAD_RECORD,
            }
        );

        let FieldKind::ExternalTaggedUnion { payload, tag } = ROOT.fields()[7].kind() else {
            panic!("expected external tagged payload metadata");
        };
        assert_eq!(payload, &PAYLOAD);
        assert_eq!(tag.field_name(), "state");
        assert_eq!(tag.offset(), 5);
        assert_eq!(tag.layout(), &SCALAR_ENUM);

        assert_eq!(STRING.encoding(), StringEncoding::Utf8);
        assert_eq!(STRING.capacity(), 8);
        assert_eq!(STRING.length(), Some(LENGTH));
        assert_eq!(STRING.data_offset(), 1);
        assert_eq!(LENGTH.repr(), LengthRepr::U8);
        assert_eq!(LENGTH.endian(), Endian::Little);
        assert_eq!(LENGTH.offset(), 0);

        assert_eq!(
            ARRAY.element(),
            ArrayElementKind::Primitive {
                primitive: PrimitiveKind::U32,
                endian: Endian::Native,
            }
        );
        assert_eq!(ARRAY.length(), 3);
        assert_eq!(ARRAY.stride(), 4);
        assert_eq!(SCALAR_ENUM.enum_values()[0].name(), "Ready");
        assert_eq!(SCALAR_ENUM.enum_values()[1].raw_value(), 9);
    }

    #[test]
    fn array_element_descriptor_forms_are_available() {
        let primitive = ArrayDescriptor::__new(
            ArrayElementKind::Primitive {
                primitive: PrimitiveKind::I16,
                endian: Endian::Big,
            },
            2,
            2,
        );
        let boolean = ArrayDescriptor::__new(ArrayElementKind::Bool, 3, 1);
        let scalar_enum = ArrayDescriptor::__new(
            ArrayElementKind::ScalarEnum {
                layout: &SCALAR_ENUM,
            },
            4,
            2,
        );
        let schema = ArrayDescriptor::__new(
            ArrayElementKind::Schema {
                layout: &PAYLOAD_RECORD,
            },
            5,
            4,
        );

        assert!(matches!(
            primitive.element(),
            ArrayElementKind::Primitive {
                primitive: PrimitiveKind::I16,
                endian: Endian::Big,
            }
        ));
        assert_eq!(boolean.element(), ArrayElementKind::Bool);
        assert_eq!(
            scalar_enum.element(),
            ArrayElementKind::ScalarEnum {
                layout: &SCALAR_ENUM,
            }
        );
        assert_eq!(
            schema.element(),
            ArrayElementKind::Schema {
                layout: &PAYLOAD_RECORD,
            }
        );
        assert_eq!(
            [
                primitive.length(),
                boolean.length(),
                scalar_enum.length(),
                schema.length(),
            ],
            [2, 3, 4, 5]
        );
        assert_eq!(
            [
                primitive.stride(),
                boolean.stride(),
                scalar_enum.stride(),
                schema.stride(),
            ],
            [2, 1, 2, 4]
        );
    }

    #[test]
    fn tagged_payload_metadata_has_no_physical_tag_storage() {
        assert_eq!(PAYLOAD.kind(), TypeKind::TaggedUnion);
        let unit = PAYLOAD.variants()[0];
        let data = PAYLOAD.variants()[1];
        assert_eq!((unit.name(), unit.raw_tag()), ("None", 0));
        assert_eq!(unit.payload(), &UNIT_PAYLOAD);
        assert_eq!((unit.payload_size(), unit.payload_align()), (1, 1));
        assert_eq!((data.name(), data.raw_tag()), ("Data", 7));
        assert_eq!(data.payload(), &PAYLOAD_RECORD);
        assert_eq!((data.payload_size(), data.payload_align()), (4, 4));
    }

    #[test]
    fn metadata_does_not_participate_in_byte_access() {
        const DIAGNOSTIC_ONLY: LayoutDescriptor = LayoutDescriptor::__new(
            "DiagnosticOnly",
            TypeKind::Struct,
            1,
            1,
            1,
            &[ByteRange::__new(8, 3)],
            &[FieldDescriptor::__new(
                "outside",
                0,
                usize::MAX,
                1,
                1,
                FieldKind::FixedBytes { length: 1 },
                false,
            )],
            &[],
            &[],
        );

        let bytes = [0xa5_u8];
        let field = DIAGNOSTIC_ONLY.fields()[0];
        assert_eq!(field.offset(), usize::MAX);
        assert_eq!(DIAGNOSTIC_ONLY.padding()[0], ByteRange::__new(8, 3));
        assert_eq!(bytes, [0xa5]);
    }

    #[test]
    fn byte_range_constructor_does_not_validate() {
        let reversed = ByteRange::__new(9, 3);
        assert_eq!((reversed.start(), reversed.end()), (9, 3));
    }

    #[test]
    fn every_scalar_metadata_enum_variant_is_available() {
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
                StringEncoding::U16C,
            ]
            .len(),
            4
        );
    }
}
