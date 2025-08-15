mod item;
use crate::{Slide, search_buffer::SearchBuffer};
pub use item::*;
use smallvec::SmallVec;
use std::{
    fmt::Debug,
    hash::{BuildHasher, Hash},
    iter,
    ops::Range,
    usize,
};
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
impl<T: Copy + Eq + Hash, const N: usize, S: BuildHasher> SearchBuffer<T, N, S> {
    pub fn to_items(
        &mut self,
        iter: impl IntoIterator<Item = T>,
        config: Config,
    ) -> impl Iterator<Item = Item<T>> {
        let mut iter = iter.into_iter();
        let mut match_window = Slide::new();
        let search_buffer = self;
        let mut raw_len: usize = 0;
        let mut back_ref: Option<(Range<usize>, usize)> = None;
        iter::from_fn(move || {
            loop {
                // Return items already found in previous call/iteration.
                if raw_len > 0 {
                    let item = Item::Raw(Vec::from_iter(match_window.drain(0..raw_len)).into());
                    raw_len = 0;
                    return Some(item);
                } else if let Some((index, end)) = back_ref.take() {
                    match_window.drain(0..index.len()).for_each(drop);
                    return Some(Item::from((index, end)));
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
                    debug_assert!(data.len() < config.match_lengths.end);
                    if let Some(range) = search_buffer.find_longest_match(data)
                        && range.len() >= config.match_lengths.start
                    {
                        back_ref = Some((range.clone(), search_buffer.end()));
                        search_buffer
                            .extend_slide(
                                data[..range.len()].into_iter().copied(),
                                config.max_buffer_len,
                            )
                            .for_each(drop);
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
}
impl<T: Copy + Eq + Hash> Slide<T> {
    pub fn from_items(
        &mut self,
        items: impl IntoIterator<Item = Item<T>>,
        config: Config,
    ) -> impl IntoIterator<Item = T> {
        let buffer = self;
        items.into_iter().flat_map(move |item| {
            let len = item.len();
            match item {
                Item::Raw(raw) => {
                    buffer.extend(raw.into_iter());
                }
                Item::Ref { back, len } => {
                    debug_assert!(usize::from(back) <= buffer.len());
                    debug_assert!(len >= config.match_lengths.start);
                    debug_assert!(
                        len < config.match_lengths.end,
                        "len {len} >= max_len {max_len}",
                        max_len = config.match_lengths.end
                    );
                    let base = buffer.len() - usize::from(back);
                    buffer.extend_from_within(base..base + len);
                }
            };
            let ret = SmallVec::<[T; 0x100]>::from(&buffer[buffer.len() - len..]);
            let over = buffer.len().saturating_sub(config.max_buffer_len);
            if over > 0 {
                buffer.drain(0..over).for_each(drop);
            }
            ret
        })
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn to_items() {
        let data = b"vwabcdeabcabcabcxvw";
        let items = SearchBuffer::<_, 2>::new()
            .to_items(
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
                Item::from((2..5, 7)),
                Item::from((7..13, 10)),
                Item::from(b"xvw")
            ]
        );
    }
    #[test]
    fn from_items() {
        let items = [
            Item::from(b"vwabcde"),
            Item::from((2..5, 7)),
            Item::from((7..13, 10)),
            Item::from(b"xvw"),
        ];
        let data = Slide::new()
            .from_items(
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
            0, 7, 118, 119, 97, 98, 99, 100, 101, 5, 3, 3, 6, 0, 3, 120, 118, 119,
        ];
        let items = [
            Item::from(b"vwabcde"),
            Item::from((2..5, 7)),
            Item::from((7..13, 10)),
            Item::from(b"xvw"),
        ];
        let bytes2 = postcard::to_stdvec(&items).unwrap();
        let items2: [Item<u8>; 4] = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(items, items2);
        assert_eq!(bytes.as_slice(), &bytes2);
    }
}
