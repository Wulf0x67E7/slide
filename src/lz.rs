use crate::search_buffer::SearchBuffer;
use std::{hash::Hash, ops::Range};

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

pub fn find_back_refs<T: Copy + Eq + Hash, const N: usize>(
    mut data: &[T],
    config: Config,
) -> impl Iterator<Item = (usize, Range<usize>)> {
    let mut search_buffer = SearchBuffer::<T, N>::new();
    std::iter::from_fn(move || {
        let mut skipped = 0;
        let range = loop {
            if data.is_empty() {
                if skipped == 0 {
                    return None;
                } else {
                    break search_buffer.end()..search_buffer.end();
                }
            } else if let Some(range) =
                search_buffer.find_longest_match(&data[..data.len().min(config.match_lengths.end)])
                && range.len() >= config.match_lengths.start
            {
                let spare_len = config.max_buffer_len - search_buffer.len();
                let mid = range.end.min(range.start + spare_len);
                search_buffer.extend_from_within(range.start..mid);
                search_buffer
                    .slide_from_within(mid..range.end)
                    .for_each(drop);

                data = &data[range.len()..];
                break range;
            } else {
                skipped += 1;
                if search_buffer.len() < config.max_buffer_len {
                    search_buffer.push(data[0]);
                } else {
                    search_buffer.step(data[0]);
                }
                data = &data[1..];
            }
        };
        Some((skipped, range))
    })
}

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
        .collect::<Vec<_>>();
        assert_eq!(refs, vec![(7, 2..5), (0, 7..13), (3, 19..19)]);
    }
}
