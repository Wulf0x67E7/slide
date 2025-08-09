use crate::{search_buffer::SearchBuffer, util::SliceExt as _};
use std::{borrow::Cow, fmt::Debug, hash::Hash, iter, ops::Range};

#[derive(Debug)]
pub struct Config {
    /// Maximum size of the search window. Default: 2^24
    pub max_buffer_len: usize,
    /// Range of accepted match lengths. Default: 1..usize::MAX
    ///
    /// Raising the minimum can exponentially speed up scanning over the search window,
    /// while also exponentially increasing potential keys in the cache.
    ///
    /// Lowering the maximum limits the size of the lookahead window.
    pub match_lengths: Range<usize>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            max_buffer_len: 0x1000000,
            match_lengths: 1..usize::MAX,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Item<'a, T>
where
    [T]: ToOwned,
    <[T] as ToOwned>::Owned: Debug,
{
    Raw(Cow<'a, [T]>),
    Ref(Range<usize>),
}
impl<'a, T> From<&'a [T]> for Item<'a, T>
where
    [T]: ToOwned,
    <[T] as ToOwned>::Owned: Debug,
{
    fn from(value: &'a [T]) -> Self {
        Self::Raw(Cow::Borrowed(value))
    }
}
impl<'a, T> From<Range<usize>> for Item<'a, T>
where
    [T]: ToOwned,
    <[T] as ToOwned>::Owned: Debug,
{
    fn from(value: Range<usize>) -> Self {
        Self::Ref(value)
    }
}

pub fn find_back_refs<T: Debug + Copy + Eq + Hash, const N: usize>(
    mut data: &[T],
    config: Config,
) -> impl Iterator<Item = Item<'_, T>>
where
    [T]: ToOwned,
    <[T] as ToOwned>::Owned: Debug,
{
    let mut search_buffer = SearchBuffer::<T, N>::new();
    let mut raw_len: usize = 0;
    let mut back_ref: Option<Range<usize>> = None;
    iter::from_fn(move || {
        loop {
            if raw_len > 0 {
                let item = Item::Raw(Cow::Borrowed(&data[..raw_len]));
                data = &data[raw_len..];
                raw_len = 0;
                return Some(item);
            } else if let Some(back_ref) = back_ref.take() {
                data = &data[back_ref.len()..];
                return Some(Item::from(back_ref));
            } else if data.is_empty() {
                return None;
            }
            while let data @ [head, ..] = data[raw_len..].get_clamped(0..config.match_lengths.end) {
                if let Some(range) = search_buffer.find_longest_match(data)
                    && range.len() >= config.match_lengths.start
                {
                    let spare_len = config.max_buffer_len - search_buffer.len();
                    let mid = range.end.min(range.start + spare_len);
                    search_buffer.extend_from_within(range.start..mid);
                    search_buffer
                        .slide_from_within(mid..range.end)
                        .for_each(drop);
                    back_ref = Some(range);
                    break;
                } else {
                    if search_buffer.len() < config.max_buffer_len {
                        search_buffer.push(*head);
                    } else {
                        search_buffer.step(*head);
                    }
                    raw_len += 1;
                }
            }
        }
    })
}

//pub fn from_back_refs<T: Copy + Eq + Hash, const N: usize>(
//    mut data: &[T],
//    config: Config,
//) -> impl Iterator<Item = (usize, Range<usize>)> {
//    todo!()
//}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_back_refs() {
        let data = b"vwabcdeabcabcabcxvw";
        let refs = super::find_back_refs::<_, 2>(
            data,
            Config {
                max_buffer_len: 8,
                match_lengths: 0..usize::MAX,
            },
        )
        .inspect(|item| println!("{item:?}"))
        .take(10)
        .collect::<Vec<_>>();
        assert_eq!(
            refs,
            vec![
                Item::from(b"vwabcde".as_slice()),
                Item::from(2..5),
                Item::from(7..13),
                Item::from(b"xvw".as_slice())
            ]
        );
    }
}
