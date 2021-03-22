#[repr(transparent)]
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct TimeOut {
    inner: libc::c_int,
}

impl TimeOut {
    pub const INFINITE: Self = Self { inner: -1 };
    pub const INSTANT: Self = Self { inner: 0 };
    pub const MAX: Self = Self {
        inner: libc::c_int::MAX,
    };
    pub const DEFAULT: Self = Self::INFINITE;

    pub const fn new(n: libc::c_int) -> Result<Self, libc::c_int> {
        if Self::in_range(n) {
            Ok(unsafe { Self::new_unchecked(n) })
        } else {
            Err(n)
        }
    }

    /// # Safety
    ///
    /// only safe if assert_eq!(Self::in_range(n), true)
    pub const unsafe fn new_unchecked(inner: libc::c_int) -> Self {
        Self { inner }
    }

    pub const fn in_range(n: libc::c_int) -> bool {
        Self::INFINITE.inner <= n && n <= Self::MAX.inner
    }
}

impl Default for TimeOut {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Into<libc::c_int> for TimeOut {
    fn into(self) -> libc::c_int {
        self.inner
    }
}

impl From<libc::c_int> for TimeOut {
    fn from(n: libc::c_int) -> Self {
        if n < Self::INFINITE.inner {
            Self::INFINITE
        } else {
            unsafe { Self::new_unchecked(n) }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::TimeOut;

    fn timeout_new(timeout: libc::c_int) {
        assert_eq!(Ok(TimeOut { inner: timeout }), TimeOut::new(timeout));
    }

    fn timeout_new_error(timeout: libc::c_int) {
        assert_eq!(Err(timeout), TimeOut::new(timeout));
    }

    #[test]
    fn new_zero() {
        timeout_new(0);
    }

    #[test]
    fn new_one() {
        timeout_new(1);
    }

    #[test]
    fn new_minus_one() {
        timeout_new(-1);
    }

    #[test]
    fn new_minus_two() {
        timeout_new_error(-2);
    }

    #[test]
    fn new_max() {
        timeout_new(libc::c_int::MAX);
    }

    #[test]
    fn infine() {
        assert_eq!(TimeOut::new(-1), Ok(TimeOut::INFINITE));
    }

    #[test]
    fn instant() {
        assert_eq!(TimeOut::new(0), Ok(TimeOut::INSTANT));
    }

    #[test]
    fn max() {
        assert_eq!(TimeOut::new(libc::c_int::MAX), Ok(TimeOut::MAX));
    }

    #[test]
    fn default() {
        assert_eq!(TimeOut::default(), TimeOut::DEFAULT);
    }

    #[test]
    fn minus_two() {
        assert_eq!(TimeOut::from(-2), TimeOut::INFINITE);
    }

    #[test]
    fn minus_one() {
        assert_eq!(TimeOut::from(-1), TimeOut::INFINITE);
    }

    #[test]
    fn zero() {
        assert_eq!(TimeOut::from(0), TimeOut::INSTANT);
    }

    #[test]
    fn one() {
        assert_eq!(Ok(TimeOut::from(1)), TimeOut::new(1));
    }

    fn timeout_new_unchecked(timeout: libc::c_int) {
        assert_eq!(TimeOut { inner: timeout }, unsafe {
            TimeOut::new_unchecked(timeout)
        });
    }

    #[test]
    fn new_unchecked_zero() {
        timeout_new_unchecked(0);
    }

    #[test]
    fn new_unchecked_one() {
        timeout_new_unchecked(1);
    }

    #[test]
    fn new_unchecked_minus_one() {
        timeout_new_unchecked(-1);
    }

    #[test]
    fn new_unchecked_c_int_max() {
        timeout_new_unchecked(libc::c_int::MAX);
    }

    #[test]
    fn into_infine() {
        assert_eq!(Into::<libc::c_int>::into(TimeOut::INFINITE), -1);
    }

    #[test]
    fn into_max() {
        assert_eq!(Into::<libc::c_int>::into(TimeOut::MAX), libc::c_int::MAX);
    }

    #[test]
    fn into_instant() {
        assert_eq!(Into::<libc::c_int>::into(TimeOut::INSTANT), 0);
    }

    #[test]
    fn into_default() {
        assert_eq!(Into::<libc::c_int>::into(TimeOut::DEFAULT), -1);
    }
}
