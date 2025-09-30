use anyhow::{Context, Result, ensure};
use std::ops::RangeInclusive;

/// Parse argument as a range.
///
/// # Examples
/// ```rust
/// use ff::args::parse_as_range;
///
/// assert_eq!(parse_as_range("10").unwrap(), 10..11);
/// assert_eq!(parse_as_range("10-15").unwrap(), 10..15); // [10, 15)
/// ```
pub fn parse_as_range<S: AsRef<str>>(range: S) -> Result<RangeInclusive<u64>> {
    let mut parts = range.as_ref().splitn(2, '-');
    let first = parts.next().expect("split has at least one item");
    let first = first.parse::<u64>().context(format!(
        "{first} is not a number. `{}` is not a valid range",
        range.as_ref()
    ))?;

    Ok(match parts.next() {
        Some(second) => {
            ensure!(
                !second.is_empty(),
                "missing range end. `{}` is not a valid range",
                range.as_ref()
            );
            let second = second.parse::<u64>().context(format!(
                "{second} is not a number. `{}` is not a valid range",
                range.as_ref()
            ))?;
            first..=second
        }
        None => first..=first,
    })
}

#[cfg(test)]
mod test {
    use super::parse_as_range;

    #[test]
    fn test_parser() {
        assert_eq!(parse_as_range("10").unwrap(), 10..=10);
        assert_eq!(parse_as_range("20-27").unwrap(), 20..=27);
        assert_eq!(parse_as_range("0-1").unwrap(), 0..=1);
    }
}
