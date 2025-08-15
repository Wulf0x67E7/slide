use std::{fmt::Debug, marker::PhantomData, ops::Range};

use serde::{
    Deserialize, Serialize,
    de::{Error, Visitor},
    ser::SerializeTuple as _,
};
use smallvec::SmallVec;

#[derive(PartialEq, Eq, Debug)]
pub enum Item<T> {
    Raw(SmallVec<[T; 256]>),
    Ref(Range<usize>),
}
impl<T, const N: usize> From<[T; N]> for Item<T> {
    fn from(value: [T; N]) -> Self {
        Self::Raw(SmallVec::from_iter(value))
    }
}
impl<T> From<Vec<T>> for Item<T> {
    fn from(value: Vec<T>) -> Self {
        Self::Raw(SmallVec::from_vec(value))
    }
}
impl<T> From<Box<[T]>> for Item<T> {
    fn from(value: Box<[T]>) -> Self {
        Self::Raw(SmallVec::from_vec(value.into()))
    }
}
impl<T: Clone, const N: usize> From<&[T; N]> for Item<T> {
    fn from(value: &[T; N]) -> Self {
        Self::Raw(SmallVec::from_iter(value.iter().cloned()))
    }
}
impl<T: Clone> From<&[T]> for Item<T> {
    fn from(value: &[T]) -> Self {
        Self::Raw(SmallVec::from_iter(value.iter().cloned()))
    }
}
impl<T> From<Range<usize>> for Item<T> {
    fn from(value: Range<usize>) -> Self {
        Self::Ref(value)
    }
}
impl<T> Item<T> {
    pub fn start(&self) -> usize {
        match self {
            Item::Raw(_) => 0,
            Item::Ref(range) => range.start,
        }
    }
    pub fn len(&self) -> usize {
        match self {
            Item::Raw(cow) => cow.len(),
            Item::Ref(range) => range.len(),
        }
    }
    pub fn as_raw(&self) -> Option<&[T]> {
        match self {
            Item::Raw(cow) => Some(&cow),
            Item::Ref(_) => None,
        }
    }
}

impl<T: Serialize> Serialize for Item<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut s = serializer.serialize_tuple(0)?;
        match self {
            Item::Raw(raw) => {
                s.serialize_element(&0)?;
                s.serialize_element(&raw.len())?;
                for value in raw {
                    s.serialize_element(value)?;
                }
            }
            Item::Ref(range) => {
                s.serialize_element(&(range.start + 1))?;
                s.serialize_element(&range.len())?;
            }
        }
        s.end()
    }
}
impl<'a, T: 'a + Copy + Deserialize<'a>> Deserialize<'a> for Item<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'a>,
    {
        struct Vis<'a, T>(PhantomData<&'a T>);
        impl<'a, T: Deserialize<'a>> Visitor<'a> for Vis<'a, T> {
            type Value = Item<T>;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(formatter, "a valid Item")
            }
            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'a>,
            {
                let start: usize = seq
                    .next_element()?
                    .ok_or_else(|| A::Error::missing_field("start"))?;
                let len: usize = seq
                    .next_element()?
                    .ok_or_else(|| A::Error::missing_field("len"))?;
                if start == 0 {
                    let mut raw: SmallVec<[T; 256]> = SmallVec::with_capacity(len);
                    for x in 0..len {
                        let value = seq
                            .next_element()?
                            .ok_or_else(|| A::Error::invalid_length(x, &self))?;
                        raw.push(value);
                    }
                    Ok(Item::Raw(raw))
                } else {
                    Ok(Item::Ref(start - 1..start - 1 + len))
                }
            }
        }
        deserializer.deserialize_tuple(usize::MAX, Vis(PhantomData))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck_macros::quickcheck;

    #[test]
    fn fuzz_case() {
        let items: Vec<Item<u8>> = Vec::from([
            Item::Ref(10194334..10194431),
            Item::Ref(0..10), // Ref with start == 0 caused error
            Item::from([109, 111, 122, 105, 108, 108, 97]),
            Item::Ref(17..139),
        ]);
        let items_encoded =
            Vec::from_iter(items.iter().map(|item| postcard::to_stdvec(&item).unwrap()));
        let items_decoded = Vec::from_iter(
            items_encoded
                .iter()
                .map(|bytes| postcard::from_bytes::<Item<u8>>(bytes).unwrap()),
        );
        assert_eq!(items, items_decoded);
    }

    #[quickcheck]
    fn fuzz(index: Vec<Range<u8>>) {
        fn normalize(Range { start, end }: Range<u8>) -> Range<usize> {
            start.min(end) as usize..end.max(start.saturating_add(1)) as usize
        }
        for index in index.into_iter().map(normalize) {
            let item = if index.start % 2 == 1 {
                Item::Raw(vec![index.start; index.len()].into())
            } else {
                Item::Ref(index)
            };
            let encoded = postcard::to_stdvec(&item).unwrap();
            let (decoded, residue) = postcard::take_from_bytes(&encoded).unwrap();
            assert_eq!(residue, &[]);
            assert_eq!(item, decoded);
        }
    }
}
