//! Uniforms: named values a shader reads as members of the generated `Customs` struct.
//!
//! Each [`Type`] has two representations that must agree on size and alignment: its WGSL type in
//! the `Customs` declaration and the bytes written to the GPU uniform buffer. A `Float` followed
//! by a `Col4` is laid out like this, because a `vec4<f32>` starts at a 16-byte-aligned offset:
//!
//! ```text
//! byte       0       4       8      12      16      20      24      28      32
//!            +-------+-------+-------+-------+-------+-------+-------+-------+
//! contents   | float |  pad  |  pad  |  pad  |   R   |   G   |   B   |   A   |
//!            +-------+-------+-------+-------+-------+-------+-------+-------+
//! ```
//!
//! A three-component vector has size 12 but alignment 16, so a following scalar can occupy byte
//! offset 12. See `docs/architecture/uniform-buffer-layout.md`.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A uniform's value, which also fixes its WGSL type.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum Type {
    Int(i32),
    Float(f32),
    VecFloat2((f32, f32)),
    VecFloat3((f32, f32, f32)),
    Col3((f32, f32, f32)),
    VecFloat4((f32, f32, f32, f32)),
    Col4((f32, f32, f32, f32)),
    VecInt2((i32, i32)),
    VecInt3((i32, i32, i32)),
    VecInt4((i32, i32, i32, i32)),
}

/// One named uniform. [`Uniforms`] guarantees its name is usable in WGSL.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Uniform {
    name: String,
    pub value: Type,
}

/// The ordered set of user uniforms, with unique, valid names.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(try_from = "Vec<Uniform>", into = "Vec<Uniform>")]
pub struct Uniforms(Vec<Uniform>);

/// Why a name can't be used for a uniform.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum Error {
    #[error("a uniform needs a name")]
    Empty,
    #[error("`{0}` is not a valid WGSL name")]
    Invalid(String),
    #[error("`{0}` is a WGSL keyword")]
    Keyword(String),
    #[error("`{0}` is built in")]
    Reserved(String),
    #[error("there already is a uniform named `{0}`")]
    Duplicate(String),
}

/// Names the program declares itself.
const RESERVED: [&str; 1] = ["time"];

/// WGSL and WESL keywords, and the type names a member could be confused with.
const KEYWORDS: [&str; 38] = [
    "alias",
    "array",
    "as",
    "bool",
    "break",
    "case",
    "const",
    "const_assert",
    "continue",
    "continuing",
    "default",
    "diagnostic",
    "discard",
    "else",
    "enable",
    "f16",
    "f32",
    "false",
    "fn",
    "for",
    "i32",
    "if",
    "import",
    "let",
    "loop",
    "override",
    "package",
    "requires",
    "return",
    "struct",
    "super",
    "switch",
    "true",
    "u32",
    "var",
    "vec2",
    "vec4",
    "while",
];

impl Type {
    pub const ALL: [Type; 10] = [
        Type::Int(0),
        Type::Float(0.0),
        Type::VecFloat2((0.0, 0.0)),
        Type::VecFloat3((0.0, 0.0, 0.0)),
        Type::Col3((0.5, 0.5, 0.5)),
        Type::VecFloat4((0.0, 0.0, 0.0, 0.0)),
        Type::Col4((0.5, 0.5, 0.5, 1.0)),
        Type::VecInt2((0, 0)),
        Type::VecInt3((0, 0, 0)),
        Type::VecInt4((0, 0, 0, 0)),
    ];

    /// The WGSL type of a member holding this value.
    pub fn wgsl(self) -> &'static str {
        match self {
            Self::Int(_) => "i32",
            Self::Float(_) => "f32",
            Self::VecFloat2(_) => "vec2<f32>",
            Self::VecFloat3(_) | Self::Col3(_) => "vec3<f32>",
            Self::VecFloat4(_) | Self::Col4(_) => "vec4<f32>",
            Self::VecInt2(_) => "vec2<i32>",
            Self::VecInt3(_) => "vec3<i32>",
            Self::VecInt4(_) => "vec4<i32>",
        }
    }

    /// The payload size, excluding padding around it.
    fn size(self) -> usize {
        match self {
            Self::Int(_) | Self::Float(_) => 4,
            Self::VecFloat2(_) | Self::VecInt2(_) => 8,
            Self::VecFloat3(_) | Self::Col3(_) | Self::VecInt3(_) => 12,
            Self::VecFloat4(_) | Self::Col4(_) | Self::VecInt4(_) => 16,
        }
    }

    /// The alignment of this member's offset in a WGSL struct.
    fn alignment(self) -> usize {
        match self {
            Self::Int(_) | Self::Float(_) => 4,
            Self::VecFloat2(_) | Self::VecInt2(_) => 8,
            Self::VecFloat3(_)
            | Self::Col3(_)
            | Self::VecInt3(_)
            | Self::VecFloat4(_)
            | Self::Col4(_)
            | Self::VecInt4(_) => 16,
        }
    }

    /// Appends the payload in WGSL's little-endian representation, without padding.
    fn write(self, bytes: &mut Vec<u8>) {
        let mut floats = |values: &[f32]| {
            values
                .iter()
                .for_each(|value| bytes.extend_from_slice(&value.to_le_bytes()))
        };
        match self {
            Self::Float(value) => floats(&[value]),
            Self::VecFloat2((x, y)) => floats(&[x, y]),
            Self::VecFloat3((x, y, z)) | Self::Col3((x, y, z)) => floats(&[x, y, z]),
            Self::VecFloat4((x, y, z, w)) | Self::Col4((x, y, z, w)) => floats(&[x, y, z, w]),
            Self::Int(value) => ints(bytes, &[value]),
            Self::VecInt2((x, y)) => ints(bytes, &[x, y]),
            Self::VecInt3((x, y, z)) => ints(bytes, &[x, y, z]),
            Self::VecInt4((x, y, z, w)) => ints(bytes, &[x, y, z, w]),
        }
    }
}

fn ints(bytes: &mut Vec<u8>, values: &[i32]) {
    values
        .iter()
        .for_each(|value| bytes.extend_from_slice(&value.to_le_bytes()));
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Int(_) => "Int",
            Self::Float(_) => "Float",
            Self::VecFloat2(_) => "VecFloat2",
            Self::VecFloat3(_) => "VecFloat3",
            Self::Col3(_) => "Col3",
            Self::VecFloat4(_) => "VecFloat4",
            Self::Col4(_) => "Col4",
            Self::VecInt2(_) => "VecInt2",
            Self::VecInt3(_) => "VecInt3",
            Self::VecInt4(_) => "VecInt4",
        })
    }
}

impl Uniform {
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl Uniforms {
    /// Whether `name` can be added, and why not.
    pub fn check(&self, name: &str) -> Result<(), Error> {
        let mut chars = name.chars();
        let Some(first) = chars.next() else {
            return Err(Error::Empty);
        };
        let identifier = (first.is_ascii_alphabetic() || first == '_')
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
            && name != "_"
            && !name.starts_with("__");
        if !identifier {
            return Err(Error::Invalid(name.to_owned()));
        }
        if KEYWORDS.contains(&name) {
            return Err(Error::Keyword(name.to_owned()));
        }
        if RESERVED.contains(&name) {
            return Err(Error::Reserved(name.to_owned()));
        }
        if self.0.iter().any(|uniform| uniform.name == name) {
            return Err(Error::Duplicate(name.to_owned()));
        }
        Ok(())
    }

    /// Appends a uniform named `name`.
    pub fn add(&mut self, name: &str, value: Type) -> Result<(), Error> {
        self.check(name)?;
        self.0.push(Uniform {
            name: name.to_owned(),
            value,
        });
        Ok(())
    }

    /// Removes the uniform named `name`, if there is one.
    pub fn remove(&mut self, name: &str) {
        self.0.retain(|uniform| uniform.name != name);
    }

    /// Sets the value of the uniform named `name`, if there is one.
    pub fn set(&mut self, name: &str, value: Type) {
        if let Some(uniform) = self.0.iter_mut().find(|uniform| uniform.name == name) {
            uniform.value = value;
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &Uniform> {
        self.0.iter()
    }
}

impl TryFrom<Vec<Uniform>> for Uniforms {
    type Error = Error;

    fn try_from(list: Vec<Uniform>) -> Result<Self, Error> {
        let mut uniforms = Self::default();
        for uniform in list {
            uniforms.add(&uniform.name, uniform.value)?;
        }
        Ok(uniforms)
    }
}

impl From<Uniforms> for Vec<Uniform> {
    fn from(uniforms: Uniforms) -> Self {
        uniforms.0
    }
}

/// The bytes of a struct with `members` in order, padded as WGSL lays it out.
pub(crate) fn pack(members: impl IntoIterator<Item = Type>) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut struct_alignment = 1;

    for value in members {
        let alignment = value.alignment();
        struct_alignment = struct_alignment.max(alignment);

        bytes.resize(align_up(bytes.len(), alignment), 0);
        let offset = bytes.len();
        value.write(&mut bytes);
        debug_assert_eq!(bytes.len() - offset, value.size());
    }

    bytes.resize(align_up(bytes.len(), struct_alignment), 0);
    bytes
}

fn align_up(offset: usize, alignment: usize) -> usize {
    debug_assert!(alignment.is_power_of_two());
    (offset + alignment - 1) & !(alignment - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_f32(bytes: &[u8], offset: usize) -> f32 {
        f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    #[test]
    fn nothing_packs_to_no_bytes() {
        assert!(pack([]).is_empty());
    }

    #[test]
    fn col3_has_trailing_struct_padding() {
        let bytes = pack([Type::Col3((1.0, 2.0, 3.0))]);

        assert_eq!(bytes.len(), 16);
        assert_eq!(read_f32(&bytes, 0), 1.0);
        assert_eq!(read_f32(&bytes, 4), 2.0);
        assert_eq!(read_f32(&bytes, 8), 3.0);
        assert_eq!(&bytes[12..16], &[0; 4]);
    }

    #[test]
    fn col4_occupies_exactly_sixteen_bytes() {
        let bytes = pack([Type::Col4((1.0, 2.0, 3.0, 4.0))]);

        assert_eq!(bytes.len(), 16);
        assert_eq!(read_f32(&bytes, 12), 4.0);
    }

    #[test]
    fn col4_is_aligned_after_a_scalar() {
        let bytes = pack([Type::Float(5.0), Type::Col4((1.0, 2.0, 3.0, 4.0))]);

        assert_eq!(bytes.len(), 32);
        assert_eq!(read_f32(&bytes, 0), 5.0);
        assert_eq!(&bytes[4..16], &[0; 12]);
        assert_eq!(read_f32(&bytes, 16), 1.0);
        assert_eq!(read_f32(&bytes, 28), 4.0);
    }

    #[test]
    fn scalar_uses_the_remaining_space_after_col3() {
        let bytes = pack([Type::Col3((1.0, 2.0, 3.0)), Type::Float(4.0)]);

        assert_eq!(bytes.len(), 16);
        assert_eq!(read_f32(&bytes, 8), 3.0);
        assert_eq!(read_f32(&bytes, 12), 4.0);
    }

    #[test]
    fn mixed_values_use_member_and_struct_alignment() {
        let bytes = pack([
            Type::VecFloat2((1.0, 2.0)),
            Type::Float(3.0),
            Type::VecFloat3((4.0, 5.0, 6.0)),
        ]);

        assert_eq!(bytes.len(), 32);
        assert_eq!(read_f32(&bytes, 0), 1.0);
        assert_eq!(read_f32(&bytes, 4), 2.0);
        assert_eq!(read_f32(&bytes, 8), 3.0);
        assert_eq!(&bytes[12..16], &[0; 4]);
        assert_eq!(read_f32(&bytes, 16), 4.0);
        assert_eq!(read_f32(&bytes, 24), 6.0);
        assert_eq!(&bytes[28..32], &[0; 4]);
    }

    #[test]
    fn every_type_reports_and_writes_its_expected_layout() {
        let cases = [
            (Type::Int(1), 4, 4),
            (Type::Float(1.0), 4, 4),
            (Type::VecFloat2((1.0, 2.0)), 8, 8),
            (Type::VecFloat3((1.0, 2.0, 3.0)), 12, 16),
            (Type::Col3((1.0, 2.0, 3.0)), 12, 16),
            (Type::VecFloat4((1.0, 2.0, 3.0, 4.0)), 16, 16),
            (Type::Col4((1.0, 2.0, 3.0, 4.0)), 16, 16),
            (Type::VecInt2((1, 2)), 8, 8),
            (Type::VecInt3((1, 2, 3)), 12, 16),
            (Type::VecInt4((1, 2, 3, 4)), 16, 16),
        ];

        for (value, size, alignment) in cases {
            let mut payload = Vec::new();
            value.write(&mut payload);

            assert_eq!(value.size(), size);
            assert_eq!(value.alignment(), alignment);
            assert_eq!(payload.len(), size);
        }
    }

    #[test]
    fn names_must_be_new_valid_wgsl_identifiers() {
        let mut uniforms = Uniforms::default();
        uniforms.add("radius", Type::Float(1.0)).unwrap();

        assert_eq!(uniforms.check(""), Err(Error::Empty));
        assert_eq!(uniforms.check("2d"), Err(Error::Invalid("2d".into())));
        assert_eq!(
            uniforms.check("my radius"),
            Err(Error::Invalid("my radius".into()))
        );
        assert_eq!(uniforms.check("__x"), Err(Error::Invalid("__x".into())));
        assert_eq!(uniforms.check("fn"), Err(Error::Keyword("fn".into())));
        assert_eq!(
            uniforms.check("import"),
            Err(Error::Keyword("import".into()))
        );
        assert_eq!(uniforms.check("time"), Err(Error::Reserved("time".into())));
        assert_eq!(
            uniforms.check("radius"),
            Err(Error::Duplicate("radius".into()))
        );
        assert_eq!(uniforms.check("_leaves2"), Ok(()));
    }

    #[test]
    fn uniforms_that_break_the_rules_do_not_deserialize() {
        let json = r#"[{"name":"a","value":{"Float":1.0}},{"name":"a","value":{"Int":2}}]"#;

        assert!(serde_json::from_str::<Uniforms>(json).is_err());
    }
}
