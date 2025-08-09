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
impl<T, const N: usize> From<[T; N]> for Item<'static, T>
where
    [T]: ToOwned,
    <[T] as ToOwned>::Owned: Debug,
{
    fn from(value: [T; N]) -> Self {
        Self::Raw(Cow::Owned(value.to_owned()))
    }
}
impl<'a, T, const N: usize> From<&'a [T; N]> for Item<'a, T>
where
    [T]: ToOwned,
    <[T] as ToOwned>::Owned: Debug,
{
    fn from(value: &'a [T; N]) -> Self {
        Self::Raw(Cow::Borrowed(value))
    }
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
impl<'a, T> Item<'a, T>
where
    [T]: ToOwned,
    <[T] as ToOwned>::Owned: Debug,
{
    fn len(&self) -> usize {
        match self {
            Item::Raw(cow) => cow.len(),
            Item::Ref(range) => range.len(),
        }
    }
}

pub fn to_back_refs<T: Debug + Copy + Eq + Hash, const N: usize>(
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
            // Return items already found in previous call/iteration.
            if raw_len > 0 {
                let item = Item::from(&data[..raw_len]);
                data = &data[raw_len..];
                raw_len = 0;
                return Some(item);
            } else if let Some(back_ref) = back_ref.take() {
                data = &data[back_ref.len()..];
                return Some(Item::from(back_ref));
            } else if data.is_empty() {
                return None;
            }
            // Keep pushing/sliding in values popped of data until valid match is found.
            while let data @ [head, ..] = data[raw_len..].get_clamped(0..config.match_lengths.end) {
                if let Some(range) = search_buffer.find_longest_match(data)
                    && range.len() >= config.match_lengths.start
                {
                    search_buffer
                        .extend_slide(
                            data[..range.len()].into_iter().copied(),
                            config.max_buffer_len,
                        )
                        .for_each(drop);
                    back_ref = Some(range);
                    break;
                } else {
                    search_buffer.push_step(*head, config.max_buffer_len);
                    raw_len += 1;
                }
            }
        }
    })
}

pub fn from_back_refs<'a, T: 'a + Debug + Copy + Eq + Hash, const N: usize>(
    items: impl IntoIterator<Item = Item<'a, T>>,
    config: Config,
) -> impl Iterator<Item = T> {
    let mut search_buffer = SearchBuffer::<T, N>::new();
    items.into_iter().flat_map(move |item| {
        let len = item.len();
        match item {
            Item::Raw(raw) => {
                search_buffer
                    .extend_slide(raw.into_owned(), config.max_buffer_len)
                    .for_each(drop);
            }
            Item::Ref(index) => {
                search_buffer
                    .extend_slide_from_within(index, config.max_buffer_len)
                    .for_each(drop);
            }
        };
        search_buffer[search_buffer.end() - len..search_buffer.end()].to_owned()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_back_refs() {
        let data = b"vwabcdeabcabcabcxvw";
        let items = super::to_back_refs::<_, 2>(
            data,
            Config {
                max_buffer_len: 8,
                match_lengths: 0..usize::MAX,
            },
        )
        .take(5)
        .collect::<Vec<_>>();
        assert_eq!(
            items,
            vec![
                Item::from(b"vwabcde"),
                Item::from(2..5),
                Item::from(7..13),
                Item::from(b"xvw")
            ]
        );
    }
    #[test]
    fn from_back_refs() {
        let items = [
            Item::from(b"vwabcde"),
            Item::from(2..5),
            Item::from(7..13),
            Item::from(b"xvw"),
        ];
        let data = super::from_back_refs::<_, 2>(
            items,
            Config {
                max_buffer_len: 8,
                match_lengths: 0..usize::MAX,
            },
        )
        .collect::<Box<[_]>>();
        assert_eq!(data.iter().as_slice(), b"vwabcdeabcabcabcxvw".as_slice());
    }
}
