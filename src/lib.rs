mod slide;
pub use slide::*;
pub mod lz;
pub mod search_buffer;
pub mod util;

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;

    #[test]
    fn nop() {}
}
