mod item;
use crate::{Slide, search_buffer::SearchBuffer};
pub use item::*;
use smallvec::SmallVec;
use std::{fmt::Debug, hash::Hash, iter, ops::Range, usize};
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

pub fn to_items<T: Debug + Copy + Eq + Hash, const N: usize>(
    iter: impl IntoIterator<Item = T>,
    config: Config,
) -> impl Iterator<Item = Item<T>>
where
    [T]: ToOwned,
    <[T] as ToOwned>::Owned: Debug,
{
    let mut iter = iter.into_iter();
    let mut match_window = Slide::new();

    let mut search_buffer = SearchBuffer::<T, N>::new();
    let mut raw_len: usize = 0;
    let mut back_ref: Option<Range<usize>> = None;
    iter::from_fn(move || {
        loop {
            // Return items already found in previous call/iteration.
            if raw_len > 0 {
                let item = Item::Raw(Vec::from_iter(match_window.drain(0..raw_len)).into());
                raw_len = 0;
                return Some(item);
            } else if let Some(back_ref) = back_ref.take() {
                match_window.drain(0..back_ref.len()).for_each(drop);
                return Some(Item::from(back_ref));
            }
            match_window.extend(
                (&mut iter).take(
                    config
                        .match_lengths
                        .end
                        .saturating_sub(match_window.len() + 1),
                ),
            );
            if match_window.is_empty() {
                return None;
            }
            // Keep pushing/sliding in values popped of data until valid match is found.
            while let data @ [head, ..] = &match_window[raw_len..] {
                if let Some(range) = search_buffer.find_longest_match(data)
                    && range.len() >= config.match_lengths.start
                {
                    debug_assert!(range.len() < config.match_lengths.end);
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
                    iter.next().map(|val| match_window.push(val));
                    raw_len += 1;
                }
            }
        }
    })
}

pub fn from_items<T: Debug + Copy + Eq + Hash, const N: usize>(
    items: impl IntoIterator<Item = Item<T>>,
    config: Config,
) -> impl IntoIterator<Item = T> {
    let mut buffer = Slide::<T>::new();
    let mut base = 0;
    items.into_iter().flat_map(move |item| {
        let len = item.len();
        match item {
            Item::Raw(raw) => {
                buffer.extend(raw.into_iter());
            }
            Item::Ref(index) => {
                assert!(len >= config.match_lengths.start);
                assert!(
                    len < config.match_lengths.end,
                    "len {len} >= max_len {max_len}",
                    max_len = config.match_lengths.end
                );
                buffer.extend_from_within(index.start - base..index.end - base);
            }
        };
        let ret = SmallVec::<[T; 0x100]>::from(&buffer[buffer.len() - len..]);
        let over = buffer.len().saturating_sub(config.max_buffer_len);
        if over > 0 {
            buffer.drain(0..over).for_each(drop);
            base += over;
        }
        ret
    })
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn to_items() {
        let data = b"vwabcdeabcabcabcxvw";
        let items = super::to_items::<_, 2>(
            data.into_iter().copied(),
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
    fn from_items() {
        let items = [
            Item::from(b"vwabcde"),
            Item::from(2..5),
            Item::from(7..13),
            Item::from(b"xvw"),
        ];
        let data = super::from_items::<_, 2>(
            items,
            Config {
                max_buffer_len: 8,
                match_lengths: 0..usize::MAX,
            },
        )
        .into_iter()
        .collect::<Box<[_]>>();
        assert_eq!(data.iter().as_slice(), b"vwabcdeabcabcabcxvw".as_slice());
    }
    #[test]
    fn serde_items() {
        let bytes = [
            0, 7, 118, 119, 97, 98, 99, 100, 101, 3, 3, 8, 6, 0, 3, 120, 118, 119,
        ];
        let items = [
            Item::from(b"vwabcde"),
            Item::from(2..5),
            Item::from(7..13),
            Item::from(b"xvw"),
        ];
        let bytes2 = postcard::to_stdvec(&items).unwrap();
        let items2: [Item<u8>; 4] = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(items, items2);
        assert_eq!(bytes.as_slice(), &bytes2);
    }
}
