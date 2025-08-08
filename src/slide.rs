use std::{
    mem::{MaybeUninit, replace, transmute},
    ops::{Deref, DerefMut, Range},
};

pub struct Slide<T> {
    data: Box<[MaybeUninit<T>]>,
    start: usize,
    end: usize,
}
impl<T> Default for Slide<T> {
    fn default() -> Self {
        Self {
            data: Box::default(),
            start: 0,
            end: 0,
        }
    }
}
impl<T: Clone> Clone for Slide<T> {
    fn clone(&self) -> Self {
        let mut ret = Self::default();
        ret.clone_from(self);
        ret
    }
    fn clone_from(&mut self, source: &Self) {
        self.clear();
        self.extend(source.iter().cloned());
    }
}
impl<T> FromIterator<T> for Slide<T> {
    fn from_iter<TT: IntoIterator<Item = T>>(source: TT) -> Self {
        let mut ret = Self::new();
        ret.extend(source);
        ret
    }
}
impl<T> Slide<T> {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
    pub fn len(&self) -> usize {
        self.end - self.start
    }
    pub fn capacity(&self) -> usize {
        self.data.len()
    }
    pub fn tail_capacity(&self) -> usize {
        self.capacity() - self.end
    }
    pub fn push(&mut self, val: T) {
        if self.tail_capacity() == 0 {
            self.ensure_capacity(self.len() + 1);
        }
        self.data[self.end] = MaybeUninit::new(val);
        self.end += 1;
    }
    pub fn pop(&mut self) -> Option<T> {
        if self.is_empty() {
            None
        } else {
            let idx = self.start;
            self.start += 1;
            if self.is_empty() {
                self.start = 0;
                self.end = 0;
            }
            unsafe { Some(replace(&mut self.data[idx], MaybeUninit::uninit()).assume_init()) }
        }
    }
    pub fn step(&mut self, val: T) -> T {
        if let Some(ret) = self.pop() {
            self.push(val);
            ret
        } else {
            val
        }
    }
    pub fn remove(&mut self, idx: usize) -> Option<T> {
        let len = self.len();
        if idx < len {
            Some(self.drain(idx..idx + 1).next().unwrap())
        } else {
            None
        }
    }
    pub fn swap_remove(&mut self, idx: usize) -> Option<T> {
        let len = self.len();
        if idx < len {
            self.swap(idx, len - 1);
            Some(self.drain(len - 1..len).next().unwrap())
        } else {
            None
        }
    }
    pub fn clear(&mut self) {
        self.drain(0..self.len()).for_each(drop);
    }
    pub fn drain(
        &mut self,
        mut range: Range<usize>,
    ) -> impl ExactSizeIterator<Item = T> + DoubleEndedIterator<Item = T> {
        let len = self.len();
        assert!(
            range.start <= range.end && range.end <= len,
            "Range<usize> ({range:?}) provided to Slide::drain is invalid or out of bounds of this Slide ({:?}).",
            0..len
        );
        let window = self.deref_mut();
        if range.start < len - range.end {
            if range.start > 0 {
                window[..range.end].rotate_right(range.len());
            }
            range = self.start..self.start + range.len();
            self.start = range.end;
        } else {
            if range.start < len {
                window[range.start..].rotate_left(range.len());
            }
            range = self.end - range.len()..self.end;
            self.end = range.start;
        }
        if self.len() == 0 {
            self.start = 0;
            self.end = 0;
        }
        // Safety: all elements in range were previously part of window and are therefore still both valid and initialized.
        self.data[range]
            .iter_mut()
            .map(|x| unsafe { replace(x, MaybeUninit::uninit()).assume_init() })
    }
    pub fn slide(&mut self, iter: impl IntoIterator<Item = T>) -> impl Iterator<Item = T> {
        iter.into_iter().map(|val| self.step(val))
    }
    fn ensure_capacity(&mut self, new_capacity: usize) {
        let len = self.len();
        let new_capacity = new_capacity.max(len);
        if new_capacity > self.tail_capacity() + len {
            let new_capacity = new_capacity
                .checked_add(new_capacity / 2)
                .map(usize::next_power_of_two)
                .filter(|&x| x != 0)
                .expect("Encountered usize integer overflow calculating new capacity.");
            if new_capacity != self.capacity() {
                let mut old = replace(&mut self.data, {
                    Vec::from_iter((0..new_capacity).map(|_| MaybeUninit::uninit()))
                        .into_boxed_slice()
                });
                self.data[..len].swap_with_slice(&mut old[self.start..self.end]);
            } else {
                for x in 0..len {
                    self.data[x] = replace(&mut self.data[self.start + x], MaybeUninit::uninit());
                }
            }
            self.start = 0;
            self.end = len;
        }
    }
}
impl<T> Extend<T> for Slide<T> {
    fn extend<Iter: IntoIterator<Item = T>>(&mut self, iter: Iter) {
        let source = iter.into_iter();
        self.ensure_capacity(self.len() + source.size_hint().0);
        for val in source {
            if self.tail_capacity() == 0 {
                self.ensure_capacity(self.len() + 1);
            }
            self.data[self.end] = MaybeUninit::new(val);
            self.end += 1;
        }
    }
}
impl<T> Deref for Slide<T> {
    type Target = [T];
    fn deref(&self) -> &Self::Target {
        // Safety: All values start..end are valid and initialized
        unsafe { transmute(&self.data[self.start..self.end]) }
    }
}
impl<T> DerefMut for Slide<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // Safety: All values start..end are valid and initialized
        unsafe { transmute(&mut self.data[self.start..self.end]) }
    }
}
impl<T> Drop for Slide<T> {
    fn drop(&mut self) {
        self.clear();
    }
}
impl<T: std::fmt::Debug> std::fmt::Debug for Slide<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let slide: &[T] = self.deref();
        f.debug_struct("Slide").field("data", &slide).finish()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use quickcheck_macros::quickcheck;

    #[test]
    fn default() {
        let mut slide = Slide::<()>::new();
        assert_eq!(slide.capacity(), 0);
        assert_eq!(slide.tail_capacity(), 0);
        assert!(slide.is_empty());
        assert_eq!(slide.len(), 0);
        assert_eq!(&*slide, &[]);
        assert_eq!(slide.pop(), None);
    }
    #[test]
    fn push() {
        let mut slide = Slide::from_iter(Some(42));
        slide.push(24);
        slide.extend([4, 20]);
        assert_eq!(slide.capacity(), 4);
        assert_eq!(slide.tail_capacity(), 0);
        assert!(!slide.is_empty());
        assert_eq!(slide.len(), 4);
        assert_eq!(&*slide, &[42, 24, 4, 20]);
    }
    #[test]
    fn pop_back() {
        let mut slide = Slide::from_iter([42, 24, 4, 20]);
        let center: Vec<_> = slide.drain(1..3).collect();
        assert_eq!(center, [24, 4]);
        assert_eq!(slide.capacity(), 8);
        assert_eq!(slide.tail_capacity(), 6);
        assert!(!slide.is_empty());
        assert_eq!(slide.len(), 2);
        assert_eq!(&*slide, &[42, 20]);
        assert_eq!(slide.pop(), Some(42));
        assert_eq!(slide.pop(), Some(20));
    }
    #[test]
    fn pop_front() {
        let mut slide = Slide::from_iter([42, 24, 4, 20, 240]);
        let center: Vec<_> = slide.drain(1..3).collect();
        assert_eq!(center, [24, 4]);
        assert_eq!(slide.capacity(), 8);
        assert_eq!(slide.tail_capacity(), 3);
        assert!(!slide.is_empty());
        assert_eq!(slide.len(), 3);
        assert_eq!(&*slide, &[42, 20, 240]);
        assert_eq!(slide.pop(), Some(42));
        assert_eq!(slide.pop(), Some(20));
        assert_eq!(slide.pop(), Some(240));
    }
    #[test]
    fn shrink() {
        let mut slide = Slide::from_iter(0..16);
        assert_eq!(slide.len(), 16);
        assert_eq!(slide.capacity(), 32);
        assert_eq!(slide.tail_capacity(), 16);
        for x in 0..16 {
            slide.pop();
            slide.push(x);
        }
        assert_eq!(slide.len(), 16);
        assert_eq!(slide.capacity(), 32);
        assert_eq!(slide.tail_capacity(), 0);
        slide.drain(0..15).count();
        assert_eq!(slide.len(), 1);
        assert_eq!(slide.capacity(), 32);
        assert_eq!(slide.tail_capacity(), 0);
        slide.push(16);
        assert_eq!(slide.len(), 2);
        assert_eq!(slide.capacity(), 4);
        assert_eq!(slide.tail_capacity(), 2);
    }
    #[test]
    fn drop() {
        struct Foo<'a>(&'a std::cell::RefCell<usize>);
        impl<'a> Drop for Foo<'a> {
            fn drop(&mut self) {
                *self.0.borrow_mut() += 1;
            }
        }
        let count = std::cell::RefCell::default();
        let _ = Slide::from_iter((0..128).map(|_| Foo(&count)));
        assert_eq!(*count.borrow(), 128);
    }
    #[quickcheck]
    fn fuzz(drain: Vec<Range<u8>>) {
        struct Foo<'a>(usize, &'a std::cell::RefCell<usize>);
        impl<'a> Drop for Foo<'a> {
            fn drop(&mut self) {
                *self.1.borrow_mut() += 1;
            }
        }
        let counter = std::cell::RefCell::default();
        let mut count = 0;
        {
            let mut removed = 0;
            let mut slide = Slide::new();
            for r in drain {
                let r = r.start.min(r.end)..r.end.max(r.start);
                slide.extend((slide.len()..u8::MAX as usize).map(|_| {
                    let ret = count;
                    count += 1;
                    Foo(ret, &counter)
                }));
                let drain: Vec<_> = slide.drain(r.start as usize..r.end as usize).collect();
                drain.windows(2).for_each(|w| {
                    assert_eq!(w[0].0.cmp(&w[1].0), std::cmp::Ordering::Less);
                });
                assert_eq!(drain.len(), r.len());
                removed += drain.iter().map(|Foo(x, _)| x).sum::<usize>();
                slide.windows(2).for_each(|w| {
                    assert_eq!(w[0].0.cmp(&w[1].0), std::cmp::Ordering::Less);
                });
                assert_eq!(
                    slide.iter().map(|Foo(x, _)| x).sum::<usize>(),
                    (count - 1) * count / 2 - removed
                );
            }
        }
        assert_eq!(count, *counter.borrow());
    }
}
