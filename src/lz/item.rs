use std::{fmt::Debug, marker::PhantomData, num::NonZero, ops::Range};

use serde::{
    Deserialize, Serialize,
    de::{Error, Visitor},
    ser::SerializeTuple as _,
};
use smallvec::SmallVec;

#[derive(PartialEq, Eq, Debug)]
pub enum Item<T> {
    Raw(SmallVec<[T; 256]>),
    Ref { back: NonZero<usize>, len: usize },
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
impl<T> From<(Range<usize>, usize)> for Item<T> {
    fn from((index, end): (Range<usize>, usize)) -> Self {
        Self::Ref {
            back: NonZero::try_from(end - index.start).unwrap(),
            len: index.len(),
        }
    }
}
impl<T> Item<T> {
    pub fn back(&self) -> usize {
        match self {
            Item::Raw(_) => 0,
            Item::Ref { back, len: _ } => (*back).into(),
        }
    }
    pub fn len(&self) -> usize {
        match self {
            Item::Raw(raw) => raw.len(),
            Item::Ref { back: _, len } => *len,
        }
    }
    pub fn as_raw(&self) -> Option<&[T]> {
        match self {
            Item::Raw(raw) => Some(&raw),
            Item::Ref { .. } => None,
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
            Item::Ref { back, len } => {
                s.serialize_element(back)?;
                s.serialize_element(len)?;
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
                let back: usize = seq
                    .next_element()?
                    .ok_or_else(|| A::Error::missing_field("start"))?;
                let len: usize = seq
                    .next_element()?
                    .ok_or_else(|| A::Error::missing_field("len"))?;
                if let Ok(back) = NonZero::try_from(back) {
                    Ok(Item::Ref { back, len })
                } else {
                    let mut raw: SmallVec<[T; 256]> = SmallVec::with_capacity(len);
                    for x in 0..len {
                        let value = seq
                            .next_element()?
                            .ok_or_else(|| A::Error::invalid_length(x, &self))?;
                        raw.push(value);
                    }
                    Ok(Item::Raw(raw))
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

    //#[test]
    //fn fuzz_case() {
    //    let items: Vec<Item<u8>> = Vec::from([
    //        Item::Ref(10194334..10194431),
    //        Item::Ref(0..10), // Ref with start == 0 caused error
    //        Item::from([109, 111, 122, 105, 108, 108, 97]),
    //        Item::Ref(17..139),
    //    ]);
    //    let items_encoded =
    //        Vec::from_iter(items.iter().map(|item| postcard::to_stdvec(&item).unwrap()));
    //    let items_decoded = Vec::from_iter(
    //        items_encoded
    //            .iter()
    //            .map(|bytes| postcard::from_bytes::<Item<u8>>(bytes).unwrap()),
    //    );
    //    assert_eq!(items, items_decoded);
    //}

    #[quickcheck]
    fn fuzz(index: Vec<Range<u8>>) {
        fn normalize(Range { start, end }: Range<u8>) -> Range<usize> {
            start.min(end) as usize..end.max(start.saturating_add(1)) as usize
        }
        for index in index.into_iter().map(normalize) {
            let item = if index.start % 2 == 0 {
                Item::Raw(vec![index.start; index.len()].into())
            } else {
                Item::Ref {
                    back: NonZero::try_from(index.start).unwrap(),
                    len: index.len(),
                }
            };
            let encoded = postcard::to_stdvec(&item).unwrap();
            let (decoded, residue) = postcard::take_from_bytes(&encoded).unwrap();
            assert_eq!(residue, &[]);
            assert_eq!(item, decoded);
        }
    }
}
