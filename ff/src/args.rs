use anyhow::{Context, Result, ensure};
use std::ops::RangeInclusive;

/// Parse argument as a range.
///
/// # Examples
/// ```rust
/// use ff::args::parse_as_range;
///
/// assert_eq!(parse_as_range("10").unwrap(), 10..=10);
/// assert_eq!(parse_as_range("10-15").unwrap(), 10..=15); // [10, 15]
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

/// Returns a string representation of the the range `nums`.
///
/// ```rust
/// use ff::args::fmt_ranges;
///
/// assert_eq!(fmt_ranges(&[1, 2, 3, 4]), "1-4");
/// assert_eq!(fmt_ranges(&[1, 2, 3, 4, 9]), "1-4, 9");
/// ```
pub fn fmt_ranges(nums: &[u64]) -> String {
    if nums.is_empty() {
        return String::new();
    }

    let mut ranges = Vec::new();
    let mut start = nums[0];
    let mut end = nums[0];

    for &num in &nums[1..] {
        if num == end + 1 {
            // Extend the current range
            end = num;
        } else {
            // Close current range
            if start == end {
                ranges.push(format!("{}", start));
            } else {
                ranges.push(format!("{}-{}", start, end));
            }
            // Start a new range
            start = num;
            end = num;
        }
    }

    // Push the last range
    if start == end {
        ranges.push(format!("{}", start));
    } else {
        ranges.push(format!("{}-{}", start, end));
    }

    ranges.join(", ")
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
