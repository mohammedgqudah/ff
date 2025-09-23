use std::{ops::Range, path::PathBuf};

/// format: (offset, number of blocks (length), linear|error, <dev> <dev offset>)
///
/// # Example
/// Map 10 blocks at offset 5 in the mapped device to 10 blocks in /dev/test at
/// offset 3
/// (5, 10, "linear", "/dev/test 3")
type Segment = (u64, u64, String, String);

/// Build a DM table that passes everything through linearly except a bad block ranges.
pub fn dm_table_for_bad_range(
    device: PathBuf,
    total_blocks: u64,
    bad: Option<&[Range<u64>]>,
) -> Vec<Segment> {
    let device = device.to_string_lossy();
    let sector_size = 512;
    let fs_block_size = 1024;
    assert!(sector_size > 0 && fs_block_size > 0);
    assert!(
        fs_block_size % sector_size == 0,
        "fs_block_size must be a multiple of sector_size"
    );

    let Some(bad) = bad else {
        return vec![(0, total_blocks, "linear".into(), format!("{device} 0"))];
    };
    assert!(
        bad.len() != 0,
        "the list of bad ranges cannot be empty, pass None instead"
    );

    let mut table = Vec::with_capacity(3);

    let mut bad = bad.to_vec();
    bad.sort_by_key(|r| r.start);

    let bad_start = bad.first().unwrap().start;
    let bad_end = bad.last().unwrap().end;

    let mut linear_start = 0;
    let mut alloc_lin_dev = |len: u64| {
        let ret = format!("{device} {}", linear_start);
        linear_start += len;
        ret
    };

    // map [0 .. bad_start) to the start of `device`
    if bad_start != 0 {
        table.push((0, bad_start, "linear".into(), alloc_lin_dev(bad_start)));
    }

    let mut last_range = bad.first().unwrap().to_owned();

    for r in bad {
        assert!(
            r.start <= total_blocks,
            "the start of the bad range cannot be more than the total number of blocks"
        );
        assert!(
            r.end <= total_blocks,
            "the end of the bad range cannot be more than the total number of blocks"
        );
        assert!(
            r.start < r.end,
            "the start of the bad range is not less than the end"
        );

        if last_range.start != r.start && last_range.end != r.start {
            table.push((
                last_range.end,
                r.start - last_range.end, // length
                "linear".into(),
                alloc_lin_dev(r.start - last_range.end),
            ));
        }
        // map [r.start .. r.end) to an `error` segment.
        table.push((
            r.start,
            r.end - r.start, // length
            "error".into(),
            String::new(),
        ));

        last_range = r;
    }

    // map [bad_end .. total) `device` after the `error` segments
    if bad_end != total_blocks {
        table.push((
            bad_end,
            total_blocks - bad_end,
            "linear".into(),
            alloc_lin_dev(total_blocks - bad_end),
        ));
    }

    table
}

#[cfg(test)]
mod test {
    use super::dm_table_for_bad_range;

    #[test]
    pub fn it_creates_a_full_linear_table() {
        let total_blocks = 15000;
        let table = dm_table_for_bad_range("/dev/test".into(), total_blocks, None);
        assert_eq!(
            table,
            vec![(
                0,
                total_blocks,
                "linear".to_string(),
                "/dev/test 0".to_string()
            )]
        )
    }

    #[test]
    pub fn it_creates_a_table_with_an_error_segment() {
        let total_blocks = 15000;
        let table = dm_table_for_bad_range("/dev/test".into(), total_blocks, Some(&[10..12]));
        assert_eq!(
            table,
            vec![
                // 10 blocks at 0, -> linear
                (0, 10, "linear".to_string(), "/dev/test 0".to_string()),
                // 2 blocks at 10 (blk 10 and 11) -> error
                (10, 2, "error".to_string(), "".to_string()),
                // the rest of the blocks at 12 -> linear
                (
                    12,
                    total_blocks - 12,
                    "linear".to_string(),
                    "/dev/test 10".to_string()
                ),
            ]
        )
    }

    #[test]
    fn error_at_start() {
        let total_blocks = 100;
        let table = dm_table_for_bad_range("/dev/test".into(), total_blocks, Some(&[0..5]));
        assert_eq!(
            table,
            vec![
                (0, 5, "error".into(), "".into()),
                (5, 95, "linear".into(), "/dev/test 0".into()),
            ]
        );
    }

    #[test]
    fn error_at_end() {
        let total_blocks = 100;
        let table = dm_table_for_bad_range("/dev/test".into(), total_blocks, Some(&[90..100]));
        assert_eq!(
            table,
            vec![
                (0, 90, "linear".into(), "/dev/test 0".into()),
                (90, 10, "error".into(), "".into()),
            ]
        );
    }

    #[test]
    fn whole_device_error() {
        let t = 100;
        let table = dm_table_for_bad_range("/dev/test".into(), t, Some(&[0..100]));
        assert_eq!(table, vec![(0, 100, "error".into(), "".into()),]);
    }

    #[test]
    fn error_near_end() {
        let total_blocks = 100;
        let table = dm_table_for_bad_range("/dev/test".into(), total_blocks, Some(&[90..99]));
        assert_eq!(
            table,
            vec![
                (0, 90, "linear".into(), "/dev/test 0".into()),
                (90, 9, "error".into(), "".into()),
                (99, 1, "linear".into(), "/dev/test 90".into()),
            ]
        );
    }

    #[test]
    fn error_at_start_and_end() {
        let total_blocks = 100;
        let table =
            dm_table_for_bad_range("/dev/test".into(), total_blocks, Some(&[0..5, 90..100]));
        assert_eq!(
            table,
            vec![
                (0, 5, "error".into(), "".into()),
                (5, 85, "linear".into(), "/dev/test 0".into()),
                (90, 10, "error".into(), "".into()),
            ]
        );
    }
    #[test]
    fn error_holes_in_middle() {
        let total_blocks = 100;
        let table =
            dm_table_for_bad_range("/dev/test".into(), total_blocks, Some(&[20..25, 60..61]));
        assert_eq!(
            table,
            vec![
                (0, 20, "linear".into(), "/dev/test 0".into()),
                (20, 5, "error".into(), "".into()),
                (25, 35, "linear".into(), "/dev/test 20".into()),
                (60, 1, "error".into(), "".into()),
                (61, 100 - 61, "linear".into(), "/dev/test 55".into()),
            ]
        );
    }

    #[test]
    fn error_holes_in_middle_and_end() {
        let total_blocks = 100;
        let table = dm_table_for_bad_range(
            "/dev/test".into(),
            total_blocks,
            Some(&[20..25, 60..61, 90..100]),
        );
        assert_eq!(
            table,
            vec![
                (0, 20, "linear".into(), "/dev/test 0".into()),
                (20, 5, "error".into(), "".into()),
                (25, 35, "linear".into(), "/dev/test 20".into()),
                (60, 1, "error".into(), "".into()),
                (61, 29, "linear".into(), "/dev/test 55".into()),
                (90, 10, "error".into(), "".into()),
            ]
        );
    }
}
