#[repr(transparent)]
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct MaxEvents {
    inner: usize,
}

impl MaxEvents {
    pub const MIN: Self = Self { inner: 1 };
    pub const MAX: Self = Self {
        inner: libc::c_int::MAX as usize,
    };
    pub const DEFAULT: Self = Self { inner: 64 };

    pub const fn new(n: usize) -> Result<Self, usize> {
        if Self::in_range(n) {
            Err(n)
        } else {
            Ok(unsafe { Self::new_unchecked(n) })
        }
    }

    /// # Safety
    ///
    /// only safe if assert_eq!(Self::in_range(n), true)
    pub const unsafe fn new_unchecked(inner: usize) -> Self {
        Self { inner }
    }

    pub const fn in_range(n: usize) -> bool {
        Self::MIN.inner <= n && n <= Self::MAX.inner
    }
}

impl From<usize> for MaxEvents {
    fn from(n: usize) -> Self {
        if n < Self::MIN.inner {
            Self::default()
        } else if n > Self::MAX.inner {
            Self::MAX
        } else {
            unsafe { Self::new_unchecked(n) }
        }
    }
}

impl Default for MaxEvents {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Into<usize> for MaxEvents {
    fn into(self) -> usize {
        self.inner
    }
}
