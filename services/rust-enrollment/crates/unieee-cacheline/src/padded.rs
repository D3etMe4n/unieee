use std::fmt;
use std::ops::{Deref, DerefMut};

pub const CACHE_LINE_SIZE: usize = 64;

#[repr(align(64))]
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub struct CachePadded<T>(T);

const _: () = assert!(core::mem::align_of::<CachePadded<u8>>() == CACHE_LINE_SIZE);
const _: () = assert!(core::mem::size_of::<CachePadded<u8>>() == CACHE_LINE_SIZE);

impl<T> CachePadded<T> {
    #[must_use]
    pub const fn new(value: T) -> Self {
        Self(value)
    }

    #[must_use]
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> Deref for CachePadded<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for CachePadded<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> AsRef<T> for CachePadded<T> {
    fn as_ref(&self) -> &T {
        &self.0
    }
}

impl<T> AsMut<T> for CachePadded<T> {
    fn as_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T: fmt::Debug> fmt::Debug for CachePadded<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("CachePadded").field(&self.0).finish()
    }
}
