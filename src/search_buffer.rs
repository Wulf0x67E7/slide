use std::{
    collections::HashMap,
    hash::{BuildHasher, Hash, RandomState},
    iter,
    ops::{Index, Range},
};

use smallvec::SmallVec;

use crate::Slide;

pub struct SearchBuffer<T, const N: usize, S = RandomState> {
    values: Slide<T>,
    offsets: Slide<usize>,
    heads: HashMap<[T; N], usize, S>,
    offset: usize,
}
impl<T, const N: usize, S: Default> Default for SearchBuffer<T, N, S> {
    fn default() -> Self {
        Self::with_hasher(S::default())
    }
}
impl<T: Copy + Eq + Hash, const N: usize, S: Default + BuildHasher> FromIterator<T>
    for SearchBuffer<T, N, S>
{
    fn from_iter<Iter: IntoIterator<Item = T>>(iter: Iter) -> Self {
        let mut ret = Self::default();
        ret.extend(iter);
        ret
    }
}
impl<T: Copy + Eq + Hash, const N: usize, S: BuildHasher> Extend<T> for SearchBuffer<T, N, S> {
    fn extend<Iter: IntoIterator<Item = T>>(&mut self, iter: Iter) {
        self.values.extend(iter);
        self.extend_offsets();
    }
}
impl<T, const N: usize, S> SearchBuffer<T, N, S> {
    pub fn new() -> Self
    where
        S: Default,
    {
        Self::default()
    }
    pub fn with_hasher(hash_builder: S) -> Self {
        Self {
            values: Default::default(),
            offsets: Default::default(),
            heads: HashMap::with_hasher(hash_builder),
            offset: 1,
        }
    }
}
impl<T: Copy + Eq + Hash, const N: usize, S: BuildHasher> SearchBuffer<T, N, S> {
    pub fn is_empty(&self) -> bool {
        self.len() == 0
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
        self.offsets
            .drain(0..ret.len().min(self.offsets.len()))
            .for_each(drop);
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
        let mut base = self.offsets.len();
        let bases = SmallVec::<[_; 256]>::from_iter(iter::from_fn(|| {
            if base < self.values.len()
                && let Some(window) = self.values[base..].first_chunk_mut::<N>().copied()
            {
                let ret = Some((window, base));
                base += 1;
                ret
            } else {
                None
            }
        }));
        let offsets = SmallVec::<[_; 256]>::from_iter(bases.into_iter().map(|(window, base)| {
            self.heads
                .insert(window, base + self.offset)
                .unwrap_or_default()
        }));
        self.offsets.extend(offsets);
    }
    fn get_match<const SKIP_N: bool>(
        &self,
        base: usize,
        arr: &[T],
        min_len: usize,
    ) -> Option<Range<usize>> {
        if min_len >= arr.len() || N >= arr.len() {
            return None;
        }
        // check that [values[..], arr[..]][index] == arr[arr_index]
        let check = |(index, arr_index): (usize, usize)| {
            self.values
                .get(index)
                .or_else(|| arr.get(index - self.values.len()))
                .and_then(|v| arr.get(arr_index).map(|a| (v, a)))
                .is_some_and(|(a, b)| a == b)
        };
        // count how long [values[..], arr[..]][index] == arr[arr_base..]
        let count = |(index, arr_base): (Range<usize>, usize)| {
            index
                .into_iter()
                .zip(arr_base..)
                .map(check)
                .take_while(bool::clone)
                .count()
        };
        let skip = if SKIP_N {
            debug_assert_eq!(count((base..base + N, 0)), N);
            N
        } else {
            0
        };
        // If check at min_len doesn't exist or doesn't match, candidate must be shorter.
        // We can therefore disregard it without a full count.
        if check((base + min_len, min_len))
            && let len = count((base + skip..usize::MAX, skip)) + skip
            && len > min_len
        {
            let start = base + self.start();
            Some(start..start + len)
        } else {
            None
        }
    }

    pub fn find_longest_match(&self, arr: &[T]) -> Option<Range<usize>> {
        self.find_longest_match_by(arr, |_max, _candidate| Ok(false))
    }

    pub fn find_longest_match_by(
        &self,
        arr: &[T],
        mut predicate: impl FnMut(Option<Range<usize>>, Range<usize>) -> Result<bool, bool>,
    ) -> Option<Range<usize>> {
        if N >= arr.len() {
            return None;
        }
        let mut max = (self.len().saturating_sub(N)..self.len())
            .into_iter()
            .flat_map(|base| self.get_match::<false>(base, arr, N))
            .max_by_key(Range::len);
        'ret: {
            let Some(mut next) = arr
                .first_chunk::<N>()
                .and_then(|head| self.heads.get(head))
                .and_then(|next| next.checked_sub(self.offset))
            else {
                break 'ret;
            };
            while let max_len = max.as_ref().map(Range::len).unwrap_or_default()
                && max_len < arr.len()
            {
                if let Some(candidate) = self.get_match::<true>(next, arr, max_len) {
                    match predicate(max.clone(), candidate.clone()) {
                        Ok(done) => {
                            max = Some(candidate);
                            if done {
                                break 'ret;
                            }
                        }
                        Err(done) => {
                            if done {
                                break 'ret;
                            }
                        }
                    }
                }
                let Some(_next) = self.offsets[next].checked_sub(self.offset) else {
                    break 'ret;
                };
                next = _next;
            }
        }
        debug_assert!(max.as_ref().map(Range::len).unwrap_or_default() <= arr.len());
        max
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
    pub fn to_values(self) -> Box<[T]> {
        self.values.to_vec().into_boxed_slice()
    }
}

impl<T, const N: usize, S> Index<usize> for SearchBuffer<T, N, S> {
    type Output = T;
    fn index(&self, index: usize) -> &Self::Output {
        &self.values[index + 1 - self.offset]
    }
}
impl<T, const N: usize, S> Index<Range<usize>> for SearchBuffer<T, N, S> {
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
