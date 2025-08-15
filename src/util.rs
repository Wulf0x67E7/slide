#![allow(dead_code)]
use std::{
    hash::{BuildHasherDefault, Hasher},
    ops::Range,
};

pub trait SliceExt<T> {
    fn get_clamped(&self, index: Range<usize>) -> &[T];
}
impl<T> SliceExt<T> for [T] {
    fn get_clamped(&self, index: Range<usize>) -> &[T] {
        &self[index.start.clamp(0, self.len())..index.end.clamp(0, self.len())]
    }
}

#[derive(Debug, Default)]
pub struct UnHasher(u64);
pub type BuildUnHasher = BuildHasherDefault<UnHasher>;
impl Hasher for UnHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        let (chunks, tail) = bytes.as_chunks::<8>();
        for chunk in chunks.into_iter().copied() {
            self.write_u64(u64::from_ne_bytes(chunk));
        }
        if !tail.is_empty() {
            self.write_u64(u64::from_ne_bytes([(); 8].map({
                let mut tail = tail.into_iter().copied();
                move |()| tail.next().unwrap_or_default()
            })));
        }
    }
    fn write_u64(&mut self, i: u64) {
        self.0 ^= i;
    }
}
