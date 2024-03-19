use std::{
    fmt::{Debug, Display},
    time::Duration,
};

/// Stores a [`Duration`] in a given precision (in milliseconds) in 2 bytes.
///
/// For instance, for `P = 1000` (1 second), this allows a for a total
/// duration of 18 hours. Values smaller than 1 second are stored as 1 second.
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Default)]
pub struct PicoDuration<const P: u64>(u16);

impl<const P: u64> PicoDuration<P> {
    pub const ZERO: PicoDuration<P> = PicoDuration(0);
    // TODO(alexkirsz) Figure out if MIN should be 0 or 1.
    pub const MIN: PicoDuration<P> = PicoDuration(1);
    pub const MAX: PicoDuration<P> = PicoDuration(u16::MAX);

    pub const fn from_millis(millis: u64) -> Self {
        if millis == 0 {
            return PicoDuration::ZERO;
        }
        if millis <= P {
            return PicoDuration::MIN;
        }
        let value = millis / P;
        if value > u16::MAX as u64 {
            return PicoDuration::MAX;
        }
        PicoDuration(value as u16)
    }

    pub const fn from_secs(secs: u64) -> Self {
        if secs == 0 {
            return PicoDuration::ZERO;
        }
        let secs_precision = P / 1_000;
        if secs <= secs_precision {
            return PicoDuration::MIN;
        }
        let value = secs * 1_000 / P;
        if value > u16::MAX as u64 {
            return PicoDuration::MAX;
        }
        PicoDuration(value as u16)
    }

    pub(self) fn to_duration(self) -> Duration {
        Duration::from_millis(self.0 as u64 * P)
    }
}

impl<const P: u64> From<Duration> for PicoDuration<P> {
    fn from(duration: Duration) -> Self {
        if duration.is_zero() {
            return PicoDuration::ZERO;
        }
        let millis = duration.as_millis();
        if millis <= P as u128 {
            return PicoDuration::MIN;
        }
        (millis / P as u128)
            .try_into()
            .map_or(PicoDuration::MAX, PicoDuration)
    }
}

impl<const P: u64> From<PicoDuration<P>> for Duration {
    fn from(duration: PicoDuration<P>) -> Self {
        duration.to_duration()
    }
}

impl<const P: u64> Display for PicoDuration<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let duration = Duration::from(*self);
        duration.fmt(f)
    }
}

impl<const P: u64> Debug for PicoDuration<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let duration = Duration::from(*self);
        duration.fmt(f)
    }
}

impl<const P: u64> PartialEq<Duration> for PicoDuration<P> {
    fn eq(&self, other: &Duration) -> bool {
        self.to_duration() == *other
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::PicoDuration;

    #[test]
    fn test_1_milli() {
        type Sd = PicoDuration<1>;

        assert_eq!(Sd::from_millis(1), Duration::from_millis(1));
        assert_eq!(Sd::from_millis(42), Duration::from_millis(42));

        assert_eq!(Sd::from_secs(1), Duration::from_secs(1));
        assert_eq!(Sd::from_secs(42), Duration::from_secs(42));

        // 1ms precision can only store up to 65.536s.
        assert_eq!(Sd::from_secs(65), Duration::from_secs(65));
        assert_eq!(Sd::from_secs(66), Sd::MAX);
    }

    #[test]
    fn test_1_sec() {
        type Sd = PicoDuration<1_000>;

        // 1ms precision can't store ms-level variations.
        assert_eq!(Sd::from_millis(1), Sd::MIN);
        assert_eq!(Sd::from_millis(42), Sd::MIN);

        assert_eq!(Sd::from_secs(1), Duration::from_secs(1));
        assert_eq!(Sd::from_secs(42), Duration::from_secs(42));

        // 1s precision can only store up to 65,535s.
        assert_eq!(Sd::from_secs(65535), Duration::from_secs(65535));
        assert_eq!(Sd::from_secs(70000), Sd::MAX);
    }
}
