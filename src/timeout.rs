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

    pub const fn new(n: libc::c_int) -> Result<Self, libc::c_int> {
        if Self::in_range(n) {
            Err(n)
        } else {
            Ok(unsafe { Self::new_unchecked(n) })
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
        Self::INFINITE
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
mod tests_timeout {
    use crate::TimeOut;

    fn timeout_new(timeout: libc::c_int) {
        let result = TimeOut::new(timeout);

        assert_eq!(Ok(TimeOut { inner: timeout }), result);
    }

    #[test]
    fn timeout_new_zero() {
        timeout_new(0);
    }

    #[test]
    fn timeout_new_one() {
        timeout_new(1);
    }

    #[test]
    fn timeout_new_minus_one() {
        timeout_new(-1);
    }

    #[test]
    #[should_panic]
    fn timeout_new_minus_two() {
        timeout_new(-2);
    }

    #[test]
    fn timeout_new_c_int_max() {
        timeout_new(libc::c_int::MAX);
    }

    #[test]
    fn timeout_max() {
        assert_eq!(
            TimeOut {
                inner: libc::c_int::MAX
            },
            TimeOut::MAX
        );
    }

    #[test]
    fn timeout_infine() {
        assert_eq!(TimeOut { inner: -1 }, TimeOut::INFINITE);
    }

    #[test]
    fn timeout_instant() {
        assert_eq!(TimeOut { inner: 0 }, TimeOut::INSTANT);
    }

    fn timeout_into(
        timeout: libc::c_int,
        expected: libc::c_int,
    ) {
        let result: TimeOut = timeout.into();

        assert_eq!(result, TimeOut { inner: expected });
    }

    #[test]
    #[should_panic]
    fn timeout_minus_two() {
        timeout_into(-2, -1);
    }
}
