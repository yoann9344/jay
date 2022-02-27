use crate::dbus::{
    DbusError, DbusType, DynamicType, Formatter, Parser, TY_ARRAY, TY_BOOLEAN, TY_BYTE, TY_DOUBLE,
    TY_INT16, TY_INT32, TY_INT64, TY_OBJECT_PATH, TY_SIGNATURE, TY_STRING, TY_UINT16, TY_UINT32,
    TY_UINT64, TY_UNIX_FD, TY_VARIANT,
};
use crate::utils::aligned::{AlignedF64, AlignedI64, AlignedU64};
use std::borrow::Cow;
use std::ops::Deref;
use std::rc::Rc;
use uapi::{OwnedFd, Packed, Pod};

unsafe impl<'a> DbusType<'a> for u8 {
    const ALIGNMENT: usize = 1;
    const IS_POD: bool = true;

    fn write_signature(w: &mut Vec<u8>) {
        w.push(TY_BYTE);
    }

    fn marshal(&self, fmt: &mut Formatter) {
        fmt.write_packed(self);
    }

    fn unmarshal(parser: &mut Parser<'a>) -> Result<Self, DbusError> {
        parser.read_pod()
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct Bool(u32);
pub const FALSE: Bool = Bool(0);
pub const TRUE: Bool = Bool(1);

unsafe impl Pod for Bool {}
unsafe impl Packed for Bool {}

unsafe impl<'a> DbusType<'a> for Bool {
    const ALIGNMENT: usize = 4;
    const IS_POD: bool = true;

    fn write_signature(w: &mut Vec<u8>) {
        w.push(TY_BOOLEAN);
    }

    fn marshal(&self, fmt: &mut Formatter) {
        fmt.write_packed(self);
    }

    fn unmarshal(parser: &mut Parser<'a>) -> Result<Self, DbusError> {
        parser.read_bool()
    }
}

unsafe impl<'a> DbusType<'a> for i16 {
    const ALIGNMENT: usize = 2;
    const IS_POD: bool = true;

    fn write_signature(w: &mut Vec<u8>) {
        w.push(TY_INT16);
    }

    fn marshal(&self, fmt: &mut Formatter) {
        fmt.write_packed(self);
    }

    fn unmarshal(parser: &mut Parser<'a>) -> Result<Self, DbusError> {
        parser.read_pod()
    }
}

unsafe impl<'a> DbusType<'a> for u16 {
    const ALIGNMENT: usize = 2;
    const IS_POD: bool = true;

    fn write_signature(w: &mut Vec<u8>) {
        w.push(TY_UINT16)
    }

    fn marshal(&self, fmt: &mut Formatter) {
        fmt.write_packed(self);
    }

    fn unmarshal(parser: &mut Parser<'a>) -> Result<Self, DbusError> {
        parser.read_pod()
    }
}

unsafe impl<'a> DbusType<'a> for i32 {
    const ALIGNMENT: usize = 4;
    const IS_POD: bool = true;

    fn write_signature(w: &mut Vec<u8>) {
        w.push(TY_INT32)
    }

    fn marshal(&self, fmt: &mut Formatter) {
        fmt.write_packed(self);
    }

    fn unmarshal(parser: &mut Parser<'a>) -> Result<Self, DbusError> {
        parser.read_pod()
    }
}

unsafe impl<'a> DbusType<'a> for u32 {
    const ALIGNMENT: usize = 4;
    const IS_POD: bool = true;

    fn write_signature(w: &mut Vec<u8>) {
        w.push(TY_UINT32)
    }

    fn marshal(&self, fmt: &mut Formatter) {
        fmt.write_packed(self);
    }

    fn unmarshal(parser: &mut Parser<'a>) -> Result<Self, DbusError> {
        parser.read_pod()
    }
}

unsafe impl<'a> DbusType<'a> for AlignedI64 {
    const ALIGNMENT: usize = 8;
    const IS_POD: bool = true;

    fn write_signature(w: &mut Vec<u8>) {
        w.push(TY_INT64)
    }

    fn marshal(&self, fmt: &mut Formatter) {
        fmt.write_packed(self);
    }

    fn unmarshal(parser: &mut Parser<'a>) -> Result<Self, DbusError> {
        parser.read_pod()
    }
}

unsafe impl<'a> DbusType<'a> for AlignedU64 {
    const ALIGNMENT: usize = 8;
    const IS_POD: bool = true;

    fn write_signature(w: &mut Vec<u8>) {
        w.push(TY_UINT64)
    }

    fn marshal(&self, fmt: &mut Formatter) {
        fmt.write_packed(self);
    }

    fn unmarshal(parser: &mut Parser<'a>) -> Result<Self, DbusError> {
        parser.read_pod()
    }
}

unsafe impl<'a> DbusType<'a> for AlignedF64 {
    const ALIGNMENT: usize = 8;
    const IS_POD: bool = true;

    fn write_signature(w: &mut Vec<u8>) {
        w.push(TY_DOUBLE)
    }

    fn marshal(&self, fmt: &mut Formatter) {
        fmt.write_packed(self);
    }

    fn unmarshal(parser: &mut Parser<'a>) -> Result<Self, DbusError> {
        parser.read_pod()
    }
}

unsafe impl<'a> DbusType<'a> for Cow<'a, str> {
    const ALIGNMENT: usize = 4;
    const IS_POD: bool = false;

    fn write_signature(w: &mut Vec<u8>) {
        w.push(TY_STRING)
    }

    fn marshal(&self, fmt: &mut Formatter) {
        fmt.write_str(self);
    }

    fn unmarshal(parser: &mut Parser<'a>) -> Result<Self, DbusError> {
        parser.read_string()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Signature<'a>(pub Cow<'a, str>);

unsafe impl<'a> DbusType<'a> for Signature<'a> {
    const ALIGNMENT: usize = 1;
    const IS_POD: bool = false;

    fn write_signature(w: &mut Vec<u8>) {
        w.push(TY_SIGNATURE)
    }

    fn marshal(&self, fmt: &mut Formatter) {
        fmt.write_signature(self.0.as_bytes());
    }

    fn unmarshal(parser: &mut Parser<'a>) -> Result<Self, DbusError> {
        parser.read_signature()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObjectPath<'a>(pub Cow<'a, str>);

unsafe impl<'a> DbusType<'a> for ObjectPath<'a> {
    const ALIGNMENT: usize = 4;
    const IS_POD: bool = false;

    fn write_signature(w: &mut Vec<u8>) {
        w.push(TY_OBJECT_PATH)
    }

    fn marshal(&self, fmt: &mut Formatter) {
        fmt.write_str(&self.0);
    }

    fn unmarshal(parser: &mut Parser<'a>) -> Result<Self, DbusError> {
        parser.read_object_path()
    }
}

unsafe impl<'a, T: DbusType<'a>> DbusType<'a> for Cow<'a, [T]> {
    const ALIGNMENT: usize = 4;
    const IS_POD: bool = false;

    fn write_signature(w: &mut Vec<u8>) {
        w.push(TY_ARRAY);
        T::write_signature(w);
    }

    fn marshal(&self, fmt: &mut Formatter) {
        fmt.write_array(self);
    }

    fn unmarshal(parser: &mut Parser<'a>) -> Result<Self, DbusError> {
        parser.read_array()
    }

    fn num_fds(&self) -> u32 {
        let mut res = 0;
        for t in self.deref() {
            res += t.num_fds();
        }
        res
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DictEntry<K, V> {
    pub key: K,
    pub value: V,
}

unsafe impl<'a, K: DbusType<'a>, V: DbusType<'a>> DbusType<'a> for DictEntry<K, V> {
    const ALIGNMENT: usize = 8;
    const IS_POD: bool = false;

    fn write_signature(w: &mut Vec<u8>) {
        w.push(b'{');
        K::write_signature(w);
        V::write_signature(w);
        w.push(b'}');
    }

    fn marshal(&self, fmt: &mut Formatter) {
        fmt.pad_to(8);
        self.key.marshal(fmt);
        self.value.marshal(fmt);
    }

    fn unmarshal(parser: &mut Parser<'a>) -> Result<Self, DbusError> {
        parser.align_to(8)?;
        Ok(Self {
            key: K::unmarshal(parser)?,
            value: V::unmarshal(parser)?,
        })
    }
}

macro_rules! tuple {
    ($($p:ident),*) => {
        #[allow(non_snake_case)]
        unsafe impl<'a, $($p: DbusType<'a>),*> DbusType<'a> for ($($p,)*) {
            const ALIGNMENT: usize = 8;
            const IS_POD: bool = false;

            fn write_signature(w: &mut Vec<u8>) {
                w.push(b'(');
                $(
                    $p::write_signature(w);
                )*
                w.push(b')');
            }

            fn marshal(&self, fmt: &mut Formatter) {
                let ($($p,)*) = self;
                fmt.pad_to(8);
                $(
                    $p.marshal(fmt);
                )*
            }

            fn unmarshal(parser: &mut Parser<'a>) -> Result<Self, DbusError> {
                parser.align_to(8)?;
                Ok(($($p::unmarshal(parser)?,)*))
            }

            fn num_fds(&self) -> u32 {
                let mut res = 0;
                let ($($p,)*) = self;
                $(
                    res += $p.num_fds();
                )*
                res
            }
        }
    }
}

tuple!(A);
tuple!(A, B);
tuple!(A, B, C);
tuple!(A, B, C, D);
tuple!(A, B, C, D, E);
tuple!(A, B, C, D, E, F);
tuple!(A, B, C, D, E, F, G);
tuple!(A, B, C, D, E, F, G, H);

#[derive(Clone, Debug)]
pub enum Variant<'a> {
    U8(u8),
    Bool(Bool),
    I16(i16),
    U16(u16),
    I32(i32),
    U32(u32),
    I64(AlignedI64),
    U64(AlignedU64),
    F64(AlignedF64),
    String(Cow<'a, str>),
    ObjectPath(ObjectPath<'a>),
    Signature(Signature<'a>),
    Variant(Box<Variant<'a>>),
    Fd(Rc<OwnedFd>),
    Array(DynamicType, Vec<Variant<'a>>),
    DictEntry(Box<Variant<'a>>, Box<Variant<'a>>),
    Struct(Vec<Variant<'a>>),
}

impl<'a> Variant<'a> {
    pub fn into_string(self) -> Result<Cow<'a, str>, DbusError> {
        match self {
            Variant::String(s) => Ok(s),
            _ => Err(DbusError::InvalidVariantType),
        }
    }

    pub fn into_object_path(self) -> Result<ObjectPath<'a>, DbusError> {
        match self {
            Variant::ObjectPath(s) => Ok(s),
            _ => Err(DbusError::InvalidVariantType),
        }
    }

    pub fn into_signature(self) -> Result<Signature<'a>, DbusError> {
        match self {
            Variant::Signature(s) => Ok(s),
            _ => Err(DbusError::InvalidVariantType),
        }
    }

    pub fn into_u32(self) -> Result<u32, DbusError> {
        match self {
            Variant::U32(s) => Ok(s),
            _ => Err(DbusError::InvalidVariantType),
        }
    }

    pub fn write_signature(&self, w: &mut Vec<u8>) {
        let c = match self {
            Variant::U8(..) => TY_BYTE,
            Variant::Bool(..) => TY_BOOLEAN,
            Variant::I16(..) => TY_INT16,
            Variant::U16(..) => TY_UINT16,
            Variant::I32(..) => TY_INT32,
            Variant::U32(..) => TY_UINT32,
            Variant::I64(..) => TY_INT64,
            Variant::U64(..) => TY_UINT64,
            Variant::F64(..) => TY_DOUBLE,
            Variant::String(..) => TY_STRING,
            Variant::ObjectPath(..) => TY_OBJECT_PATH,
            Variant::Signature(..) => TY_SIGNATURE,
            Variant::Variant(..) => TY_VARIANT,
            Variant::Fd(..) => TY_UNIX_FD,
            Variant::Array(el, _) => {
                w.push(TY_ARRAY);
                el.write_signature(w);
                return;
            }
            Variant::DictEntry(k, v) => {
                w.push(b'{');
                k.write_signature(w);
                v.write_signature(w);
                w.push(b'}');
                return;
            }
            Variant::Struct(f) => {
                w.push(b'(');
                for f in f {
                    f.write_signature(w);
                }
                w.push(b')');
                return;
            }
        };
        w.push(c);
    }
}

unsafe impl<'a> DbusType<'a> for Variant<'a> {
    const ALIGNMENT: usize = 1;
    const IS_POD: bool = false;

    fn write_signature(w: &mut Vec<u8>) {
        w.push(TY_VARIANT);
    }

    fn marshal(&self, fmt: &mut Formatter) {
        fmt.write_variant(self);
    }

    fn unmarshal(parser: &mut Parser<'a>) -> Result<Self, DbusError> {
        parser.read_variant()
    }

    fn num_fds(&self) -> u32 {
        match self {
            Variant::U8(_) => 0,
            Variant::Bool(_) => 0,
            Variant::I16(_) => 0,
            Variant::U16(_) => 0,
            Variant::I32(_) => 0,
            Variant::U32(_) => 0,
            Variant::I64(_) => 0,
            Variant::U64(_) => 0,
            Variant::F64(_) => 0,
            Variant::String(_) => 0,
            Variant::ObjectPath(_) => 0,
            Variant::Signature(_) => 0,
            Variant::Variant(v) => v.num_fds(),
            Variant::Array(_, a) => a.iter().map(|e| e.num_fds()).sum(),
            Variant::DictEntry(k, v) => k.num_fds() + v.num_fds(),
            Variant::Struct(f) => f.iter().map(|f| f.num_fds()).sum(),
            Variant::Fd(_) => 1,
        }
    }
}
