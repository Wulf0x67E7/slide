use std::ops::Range;

pub trait SliceExt<T> {
    fn get_clamped(&self, index: Range<usize>) -> &[T];
}
impl<T> SliceExt<T> for [T] {
    fn get_clamped(&self, index: Range<usize>) -> &[T] {
        &self[index.start.clamp(0, self.len())..index.end.clamp(0, self.len())]
    }
}
