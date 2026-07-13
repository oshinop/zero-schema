use proc_macro2::Span;
use syn::{Expr, ExprRange, Generics, Ident, Lifetime, Path, Type, Visibility};

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum SchemaKind {
    Struct,
    ScalarEnum,
    TaggedEnum,
}
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum Endian {
    Native,
    Little,
    Big,
}
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum Tail {
    Ignore,
    Zero,
}
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum Padding {
    Ignore,
    Zero,
}
#[derive(Clone, Default)]
pub struct ContainerOptionSpans {
    pub runtime: Option<Span>,
    pub endian: Option<Span>,
    pub align: Option<Span>,
    pub padding: Option<Span>,
    pub tail: Option<Span>,
    pub tag: Option<Span>,
    pub borrow: Option<Span>,
    pub validate_with: Option<Span>,
}
#[derive(Clone)]
pub struct ContainerOptions {
    pub runtime: Option<Path>,
    pub endian: Endian,
    pub align: Option<u32>,
    pub padding: Padding,
    pub tail: Tail,
    pub tag: Option<Path>,
    pub borrow: Option<Lifetime>,
    pub validate_with: Option<Path>,
    pub spans: ContainerOptionSpans,
}
impl Default for ContainerOptions {
    fn default() -> Self {
        Self {
            runtime: None,
            endian: Endian::Native,
            align: None,
            padding: Padding::Ignore,
            tail: Tail::Ignore,
            tag: None,
            borrow: None,
            validate_with: None,
            spans: ContainerOptionSpans::default(),
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
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
impl PrimitiveKind {
    pub const fn wire_size(self) -> u32 {
        match self {
            Self::U8 | Self::I8 => 1,
            Self::U16 | Self::I16 => 2,
            Self::U32 | Self::I32 | Self::F32 => 4,
            Self::U64 | Self::I64 | Self::F64 => 8,
        }
    }
}
#[derive(Clone)]
pub enum FieldKind {
    Primitive(PrimitiveKind),
    Bool,
    Utf8,
    CStr,
    U16Str,
    U16CStr,
    FixedBytes(Expr),
    Schema,
}
#[derive(Clone, Default)]
pub struct FieldOptionSpans {
    pub endian: Option<Span>,
    pub align: Option<Span>,
    pub capacity: Option<Span>,
    pub len_type: Option<Span>,
    pub tail: Option<Span>,
    pub tag_field: Option<Span>,
    pub validate_with: Option<Span>,
    pub range: Option<Span>,
    pub must_equal: Option<Span>,
}
#[derive(Clone, Default)]
pub struct FieldOptions {
    pub endian: Option<Endian>,
    pub align: Option<u32>,
    pub capacity: Option<u32>,
    pub len_type: Option<Ident>,
    pub tail: Option<Tail>,
    pub tag_field: Option<Ident>,
    pub validate_with: Option<Path>,
    pub range: Option<ExprRange>,
    pub must_equal: Option<Expr>,
    pub spans: FieldOptionSpans,
}
#[derive(Clone)]
pub struct ResolvedFieldOptions {
    pub endian: Endian,
    pub tail: Tail,
    pub length_repr: Option<Ident>,
    pub target_endian_check: Option<Endian>,
}
#[expect(
    dead_code,
    reason = "visibility is consumed by the pending struct generator"
)]
#[derive(Clone)]
pub struct FieldIr {
    pub ident: Ident,
    pub visibility: Visibility,
    pub original_type: Type,
    pub type_span: Span,
    pub kind: FieldKind,
    pub options: FieldOptions,
    pub resolved: ResolvedFieldOptions,
    pub external_tag_link: Option<usize>,
}

#[derive(Clone)]
pub enum VariantShape {
    Unit,
    Newtype(Box<Type>),
}
#[derive(Clone)]
pub struct VariantIr {
    pub ident: Ident,
    pub shape: VariantShape,
    pub discriminant: Option<Expr>,
    pub tag: Option<Path>,
    pub span: Span,
    pub tag_span: Option<Span>,
}

#[derive(Clone)]
pub struct GeneratedNames {
    pub module: Ident,
    pub wire: Ident,
    pub decode_error: Ident,
    pub encode_error: Ident,
}
#[derive(Clone)]
pub enum ObligationKind {
    Schema,
    Decode,
    Encode,
    ScalarEnum,
    ScalarWire,
    TaggedUnion,
    DecodeTaggedUnion,
    Validator,
    Layout,
    Tail,
    Padding,
    WholeInput,
    ExternalTag,
    WideTarget(Endian),
}
#[derive(Clone)]
pub struct Obligation {
    pub kind: ObligationKind,
    pub span: Span,
    pub ty: Option<Type>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StringHelper {
    Utf8,
    CStr,
    U16Str,
    U16CStr,
}
#[derive(Clone)]
pub enum SymbolicSize {
    Type(Type),
    Fixed(u32),
    Expr(Expr),
    String {
        helper: StringHelper,
        capacity: u32,
        unit_size: u32,
        length_size: Option<u32>,
    },
}
impl From<u32> for SymbolicSize {
    fn from(value: u32) -> Self {
        Self::Fixed(value)
    }
}
impl From<Expr> for SymbolicSize {
    fn from(value: Expr) -> Self {
        Self::Expr(value)
    }
}
#[derive(Clone)]
pub enum LayoutExpr {
    Fixed(u32),
    TypeSize(Type),
    TypeAlign(Type),
    FieldSize(usize),
    FieldAlign(usize),
    AlignUp(Box<LayoutExpr>, Box<LayoutExpr>),
    Add(Box<LayoutExpr>, Box<LayoutExpr>),
    Max(Vec<LayoutExpr>),
}
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum CheckedLayoutOp {
    AlignUp,
    Add,
}
#[derive(Clone)]
pub struct LayoutCheck {
    pub op: CheckedLayoutOp,
    pub expression: LayoutExpr,
    pub span: Span,
}
#[derive(Clone)]
pub struct LayoutField {
    pub field_index: usize,
    pub size: SymbolicSize,
    pub align: Option<u32>,
    pub offset: LayoutExpr,
    pub stride: LayoutExpr,
}
#[derive(Clone, Default)]
pub struct LayoutPlan {
    pub fields: Vec<LayoutField>,
    pub root_align: Option<u32>,
    pub wide_checks: Vec<(usize, Endian)>,
    pub aggregate_size: Option<LayoutExpr>,
    pub aggregate_align: Option<LayoutExpr>,
    pub aggregate_stride: Option<LayoutExpr>,
    pub tagged_payload_size: Option<LayoutExpr>,
    pub tagged_payload_align: Option<LayoutExpr>,
    pub checked: Vec<LayoutCheck>,
}
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum ErrorCase {
    Layout,
    InvalidBool,
    LengthOutOfBounds,
    InvalidUtf8,
    MissingNul,
    NonZeroTail,
    NonZeroPadding,
    Nested,
    UnknownUnionTag,
    UnknownScalarValue,
    CapacityExceeded,
    TagMismatch,
    RangeViolation,
    MustEqualViolation,
    Custom,
}
#[derive(Clone, Default)]
pub struct ErrorShape {
    pub decode: Vec<ErrorCase>,
    pub encode: Vec<ErrorCase>,
}
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum RuntimePathSource {
    Explicit,
    ResolveDirectDependency,
}
#[derive(Clone)]
pub struct PathResolution {
    /// Runtime path as written at declaration scope.
    pub parent_runtime_path: Option<Path>,
    /// Runtime path frozen for use from the generated child module.
    pub hidden_runtime_path: Option<Path>,
    pub runtime_source: RuntimePathSource,
}
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum PathRebase {
    Preserve,
    RebaseOneLevel,
    RewriteSchemaSelf,
}
#[derive(Clone)]
pub struct MovedPath {
    pub path: Path,
    pub strategy: PathRebase,
    pub span: Span,
}
#[derive(Clone)]
pub struct VisibilityPlan {
    /// Visibility of the generated child module at declaration scope.
    pub module: Visibility,
    /// Visibility used by items emitted inside that child module.
    pub support: Visibility,
}
#[derive(Clone)]
pub struct LifetimeModel {
    pub source: Lifetime,
    pub outlives: Vec<(Lifetime, Lifetime)>,
}

#[derive(Clone)]
pub struct SchemaIr {
    pub kind: SchemaKind,
    pub ident: Ident,
    pub visibility: Visibility,
    pub original_generics: Generics,
    pub cleaned_generics: Generics,
    pub borrow_lifetime: Option<Lifetime>,
    pub source_lifetime: Lifetime,
    pub scalar_repr: Option<Ident>,
    pub options: ContainerOptions,
    pub fields: Vec<FieldIr>,
    pub variants: Vec<VariantIr>,
    pub generated_names: GeneratedNames,
    pub obligations: Vec<Obligation>,
    pub layout_plan: LayoutPlan,
    pub error_shape: ErrorShape,
    pub path_resolution: PathResolution,
    pub moved_paths: Vec<MovedPath>,
    pub visibility_plan: VisibilityPlan,
    pub external_tag_graph: Vec<Option<usize>>,
    pub poisoned: bool,
}
