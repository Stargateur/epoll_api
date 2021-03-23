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

#[cfg(test)]
mod tests {
    use super::MaxEvents;

    fn max_events_new(max_events: usize) {
        assert_eq!(
            Ok(MaxEvents { inner: max_events }),
            MaxEvents::new(max_events)
        );
    }

    fn max_events_new_error(max_events: usize) {
        assert_eq!(Err(max_events), MaxEvents::new(max_events));
    }

    #[test]
    fn new_zero() {
        max_events_new_error(0);
    }

    #[test]
    fn new_one() {
        max_events_new(1);
    }

    #[test]
    fn new_two() {
        max_events_new(2);
    }

    #[test]
    fn new_max() {
        max_events_new(libc::c_int::MAX as usize);
    }

    #[test]
    fn new_max_usize() {
        max_events_new_error(usize::MAX);
    }

    #[test]
    fn min() {
        assert_eq!(MaxEvents::new(1), Ok(MaxEvents::MIN));
    }

    #[test]
    fn max() {
        assert_eq!(
            MaxEvents::new(libc::c_int::MAX as usize),
            Ok(MaxEvents::MAX)
        );
    }

    #[test]
    fn default() {
        assert_eq!(MaxEvents::default(), MaxEvents::DEFAULT);
    }

    #[test]
    fn one() {
        assert_eq!(Ok(MaxEvents::from(1)), MaxEvents::new(1));
    }

    #[test]
    fn saturate_max() {
        assert_eq!(MaxEvents::from(usize::MAX), MaxEvents::MAX);
    }

    fn max_events_new_unchecked(max_events: usize) {
        assert_eq!(MaxEvents { inner: max_events }, unsafe {
            MaxEvents::new_unchecked(max_events)
        });
    }

    #[test]
    fn new_unchecked_one() {
        max_events_new_unchecked(1);
    }

    #[test]
    fn new_unchecked_two() {
        max_events_new_unchecked(2);
    }

    #[test]
    fn new_unchecked_max() {
        max_events_new_unchecked(usize::MAX);
    }

    #[test]
    fn into_min() {
        assert_eq!(Into::<usize>::into(MaxEvents::MIN), 1);
    }

    #[test]
    fn into_max() {
        assert_eq!(
            Into::<usize>::into(MaxEvents::MAX),
            libc::c_int::MAX as usize
        );
    }

    #[test]
    fn into_default() {
        assert_eq!(Into::<usize>::into(MaxEvents::DEFAULT), 64);
    }
}
