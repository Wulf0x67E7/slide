use std::{
    collections::HashMap,
    hash::Hash,
    ops::{Index, Range},
};

use smallvec::SmallVec;

use crate::Slide;

pub struct SearchBuffer<T, const N: usize> {
    values: Slide<T>,
    offsets: Slide<usize>,
    heads: HashMap<[T; N], usize>,
    offset: usize,
}
impl<T, const N: usize> Default for SearchBuffer<T, N> {
    fn default() -> Self {
        Self {
            values: Default::default(),
            offsets: Default::default(),
            heads: Default::default(),
            offset: 1,
        }
    }
}
impl<T: Copy + Eq + Hash, const N: usize> FromIterator<T> for SearchBuffer<T, N> {
    fn from_iter<Iter: IntoIterator<Item = T>>(iter: Iter) -> Self {
        let mut ret = Self::default();
        ret.extend(iter);
        ret
    }
}
impl<T: Copy + Eq + Hash, const N: usize> Extend<T> for SearchBuffer<T, N> {
    fn extend<Iter: IntoIterator<Item = T>>(&mut self, iter: Iter) {
        self.values.extend(iter);
        self.extend_offsets();
    }
}

impl<T: Copy + Eq + Hash, const N: usize> SearchBuffer<T, N> {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn len(&self) -> usize {
        debug_assert_eq!(
            self.values.len().saturating_sub(N.saturating_sub(1)),
            self.offsets.len()
        );
        self.values.len()
    }
    pub fn start(&self) -> usize {
        self.offset - 1
    }
    pub fn end(&self) -> usize {
        self.start() + self.len()
    }
    pub fn range(&self) -> Range<usize> {
        self.start()..self.end()
    }
    pub fn push(&mut self, val: T) {
        self.values.push(val);
        self.extend_offsets();
    }
    pub fn pop(&mut self) -> Option<T> {
        self.values.pop().inspect(|_| {
            self.offsets.pop().unwrap();
            self.offset += 1
        })
    }
    pub fn step(&mut self, val: T) -> T {
        if let Some(ret) = self.pop() {
            self.push(val);
            ret
        } else {
            val
        }
    }
    pub fn drain(
        &mut self,
        n: usize,
    ) -> impl ExactSizeIterator<Item = T> + DoubleEndedIterator<Item = T> {
        let ret = self.values.drain(0..n);
        self.offsets.drain(0..ret.len()).for_each(drop);
        self.offset += ret.len();
        ret
    }
    pub fn slide(&mut self, iter: impl IntoIterator<Item = T>) -> impl Iterator<Item = T> {
        iter.into_iter().map(|val| self.step(val))
    }
    pub fn push_step(&mut self, val: T, max_len: usize) -> Option<T> {
        if self.len() < max_len {
            self.push(val);
            None
        } else {
            Some(self.step(val))
        }
    }
    pub fn push_step_from_within(&mut self, index: usize, max_len: usize) -> Option<T> {
        self.push_step(self[index], max_len)
    }
    pub fn extend_slide(
        &mut self,
        iter: impl IntoIterator<Item = T>,
        max_len: usize,
    ) -> impl Iterator<Item = T> {
        let mut iter = iter.into_iter();
        if self.len() < max_len {
            self.extend((&mut iter).take(max_len - self.len()));
        }
        self.slide(iter)
    }
    pub fn extend_slide_from_within(
        &mut self,
        mut index: Range<usize>,
        max_len: usize,
    ) -> impl Iterator<Item = T> {
        let spare_len = self.len().saturating_sub(max_len);
        if spare_len > 0 {
            let start = index.start;
            index.start = index.end.min(index.start + spare_len);
            self.extend_from_within(start..index.start);
        }
        self.slide_from_within(index)
    }
    fn extend_offsets(&mut self) {
        while let base = self.offsets.len()
            && base < self.values.len()
            && let Some(window) = self.values[base..].first_chunk_mut::<N>().copied()
        {
            self.offsets.push(
                self.heads
                    .insert(window, base + self.offset)
                    .unwrap_or_default(),
            );
        }
    }
    fn get_match<const SKIP_N: bool>(&self, base: usize, arr: &[T]) -> Range<usize> {
        let skip = if SKIP_N {
            debug_assert!(
                self.values[base..base + N]
                    .into_iter()
                    .zip(&arr[..N])
                    .all(|(a, b)| a == b)
            );
            N
        } else {
            0
        };
        let start = base + self.start();
        start
            ..start
                + self.values[base..]
                    .iter()
                    .chain(arr)
                    .copied()
                    .zip(arr.iter().copied())
                    .skip(skip)
                    .take_while(|(a, b)| a == b)
                    .count()
                + skip
    }
    pub fn find_longest_match(&self, arr: &[T]) -> Option<Range<usize>> {
        let mut max = (self.len().saturating_sub(N)..self.len())
            .into_iter()
            .map(|base| self.get_match::<false>(base, arr))
            .max_by_key(Range::len)?;
        'ret: {
            let Some(mut next) = arr
                .first_chunk::<N>()
                .and_then(|head| self.heads.get(head))
                .and_then(|next| next.checked_sub(self.offset))
            else {
                break 'ret;
            };
            while max.len() < arr.len() {
                let candidate = self.get_match::<true>(next, arr);
                if candidate.len() > max.len() {
                    max = candidate;
                }
                let Some(_next) = self.offsets[next].checked_sub(self.offset) else {
                    break 'ret;
                };
                next = _next;
            }
        }
        debug_assert!(max.len() <= arr.len());
        (max.len() >= N).then_some(max)
    }
    pub fn push_from_within(&mut self, index: usize) {
        self.push(self[index]);
    }
    pub fn extend_from_within(&mut self, mut index: Range<usize>) {
        assert!(
            self.range().contains(&index.start),
            "The value of index.start ({index:?}) is out of bounds of the SearchBuffer ({range:?})",
            range = self.range()
        );
        while !index.is_empty() {
            let _index = index.start..index.end.min(self.end());
            index.end -= _index.len();
            self.extend(SmallVec::<[_; 256]>::from_iter(
                self[_index].iter().copied(),
            ));
        }
    }
    pub fn step_from_within(&mut self, index: usize) -> T {
        self.step(self[index])
    }
    pub fn slide_from_within(&mut self, index: Range<usize>) -> impl Iterator<Item = T> {
        assert!(
            self.range().contains(&index.start),
            "The value of index.start ({index:?}) is out of bounds of the SearchBuffer ({range:?})",
            range = self.range()
        );
        index.into_iter().map(|index| self.step_from_within(index))
    }
}

impl<T, const N: usize> Index<usize> for SearchBuffer<T, N> {
    type Output = T;
    fn index(&self, index: usize) -> &Self::Output {
        &self.values[index + 1 - self.offset]
    }
}
impl<T, const N: usize> Index<Range<usize>> for SearchBuffer<T, N> {
    type Output = [T];
    fn index(&self, index: Range<usize>) -> &Self::Output {
        &self.values[index.start + 1 - self.offset..index.end + 1 - self.offset]
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default() {
        let sb: SearchBuffer<char, 2> = SearchBuffer::default();
        assert_eq!(&*sb.values, []);
        assert_eq!(&*sb.offsets, []);
        assert_eq!(sb.heads, HashMap::default());
        assert_eq!(sb.offset, 1);
        assert_eq!(sb.len(), 0);
        assert_eq!(sb.find_longest_match(&['a', 'b']), None);
    }

    #[test]
    fn extend() {
        let mut sb: SearchBuffer<char, 2> = SearchBuffer::default();
        sb.extend(['a', 'b', 'c']);
        assert_eq!(&*sb.values, ['a', 'b', 'c']);
        assert_eq!(&*sb.offsets, [0, 0]);
        assert_eq!(
            &sb.heads,
            &HashMap::from_iter([(['a', 'b'], 1), (['b', 'c'], 2),])
        );
        sb.extend_from_within(0..2);
        assert_eq!(&*sb.values, ['a', 'b', 'c', 'a', 'b']);
        assert_eq!(&*sb.offsets, [0, 0, 0, 1]);
        assert_eq!(
            &sb.heads,
            &HashMap::from_iter([(['a', 'b'], 4), (['b', 'c'], 2), (['c', 'a'], 3)])
        );
        sb.extend_from_within(3..8);
        assert_eq!(
            &*sb.values,
            ['a', 'b', 'c', 'a', 'b', 'a', 'b', 'a', 'b', 'a']
        );
    }

    #[test]
    fn index() {
        let mut sb: SearchBuffer<char, 2> =
            SearchBuffer::from_iter(['a', 'b', 'c', 'a', 'b', 'c', 'd']);
        assert_eq!(sb[0..3], ['a', 'b', 'c']);
        assert_eq!(sb[4..7], ['b', 'c', 'd']);
        sb.drain(2).for_each(drop);
        assert_eq!(sb[4..7], ['b', 'c', 'd']);
    }

    #[test]
    fn find_longest_match() {
        let mut sb: SearchBuffer<char, 2> =
            SearchBuffer::from_iter(['a', 'b', 'c', 'a', 'b', 'c', 'd']);
        assert_eq!(sb.find_longest_match(&['f', 'a', 'b', 'c']), None);
        assert_eq!(sb.find_longest_match(&['a', 'b', 'c', 'e']), Some(3..6));
        assert_eq!(sb.find_longest_match(&['a', 'b', 'c', 'a']), Some(0..4));
        assert_eq!(
            sb.find_longest_match(&['c', 'd', 'c', 'd', 'c', 'd', 'e']),
            Some(5..11)
        );
        sb.drain(3).for_each(drop);
        assert_eq!(sb.find_longest_match(&['c', 'a', 'b', 'c']), None);
        assert_eq!(sb.find_longest_match(&['a', 'b', 'c', 'e']), Some(3..6));
        assert_eq!(sb.find_longest_match(&['a', 'b', 'c', 'a']), Some(3..6));
        assert_eq!(
            sb.find_longest_match(&['c', 'd', 'c', 'd', 'c', 'd', 'e']),
            Some(5..11)
        );
        assert_eq!(sb.find_longest_match(&['d', 'd', 'd', 'd']), Some(6..10));
    }
}
