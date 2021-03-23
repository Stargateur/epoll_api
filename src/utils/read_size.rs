#[repr(transparent)]
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct ReadSize {
    inner: usize,
}

impl ReadSize {
    pub const MIN: Self = Self { inner: 1 };
    pub const MAX: Self = Self { inner: usize::MAX };
    pub const DEFAULT: Self = Self { inner: 4096 };

    pub const fn new(n: usize) -> Result<Self, usize> {
        if Self::in_range(n) {
            Ok(unsafe { Self::new_unchecked(n) })
        } else {
            Err(n)
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

impl From<usize> for ReadSize {
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

impl Default for ReadSize {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Into<usize> for ReadSize {
    fn into(self) -> usize {
        self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::ReadSize;

    fn read_size_new(read_size: usize) {
        assert_eq!(Ok(ReadSize { inner: read_size }), ReadSize::new(read_size));
    }

    fn read_size_new_error(read_size: usize) {
        assert_eq!(Err(read_size), ReadSize::new(read_size));
    }

    #[test]
    fn new_zero() {
        read_size_new_error(0);
    }

    #[test]
    fn new_one() {
        read_size_new(1);
    }

    #[test]
    fn new_two() {
        read_size_new(2);
    }

    #[test]
    fn new_max() {
        read_size_new(usize::MAX);
    }

    #[test]
    fn min() {
        assert_eq!(ReadSize::new(1), Ok(ReadSize::MIN));
    }

    #[test]
    fn max() {
        assert_eq!(ReadSize::new(usize::MAX), Ok(ReadSize::MAX));
    }

    #[test]
    fn saturate_max() {
        assert_eq!(ReadSize::new(usize::MAX), Ok(ReadSize::MAX));
    }

    #[test]
    fn default() {
        assert_eq!(ReadSize::default(), ReadSize::DEFAULT);
    }

    #[test]
    fn one() {
        assert_eq!(Ok(ReadSize::from(1)), ReadSize::new(1));
    }

    fn read_size_new_unchecked(read_size: usize) {
        assert_eq!(ReadSize { inner: read_size }, unsafe {
            ReadSize::new_unchecked(read_size)
        });
    }

    #[test]
    fn new_unchecked_one() {
        read_size_new_unchecked(1);
    }

    #[test]
    fn new_unchecked_two() {
        read_size_new_unchecked(2);
    }

    #[test]
    fn new_unchecked_max() {
        read_size_new_unchecked(usize::MAX);
    }

    #[test]
    fn into_min() {
        assert_eq!(Into::<usize>::into(ReadSize::MIN), 1);
    }

    #[test]
    fn into_max() {
        assert_eq!(Into::<usize>::into(ReadSize::MAX), usize::MAX);
    }

    #[test]
    fn into_default() {
        assert_eq!(Into::<usize>::into(ReadSize::DEFAULT), 4096);
    }
}
