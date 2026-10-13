//! Pure segmentation logic: turning a file length into byte ranges and
//! re-splitting ranges as the transfer progresses (IDM-style *dynamic
//! segmentation*).

use crate::types::{Segment, SegmentStatus};

/// Split `total` bytes into up to `connections` contiguous, inclusive ranges.
///
/// The split is as even as possible: the remainder is distributed one byte at
/// a time across the leading segments so the ranges tile `[0, total)` exactly.
/// Requesting more connections than there are bytes is clamped.
pub fn plan(total: u64, connections: u8) -> Vec<Segment> {
    if total == 0 {
        return Vec::new();
    }
    let wanted = (connections.max(1) as u64).min(total);
    let base = total / wanted;
    let remainder = total % wanted;

    let mut segments = Vec::with_capacity(wanted as usize);
    let mut start = 0u64;
    for i in 0..wanted {
        let len = base + u64::from(i < remainder);
        if len == 0 {
            continue;
        }
        let end = start + len - 1;
        segments.push(Segment {
            index: segments.len() as u32,
            start,
            end,
            downloaded: 0,
            status: SegmentStatus::Pending,
        });
        start = end + 1;
    }
    segments
}

/// Re-index segments so `index` matches position in the vector.
pub fn reindex(segments: &mut [Segment]) {
    for (i, seg) in segments.iter_mut().enumerate() {
        seg.index = i as u32;
    }
}

/// Split the largest *unfinished* segment roughly in half, provided it is
/// worth splitting (`>= 2 * min_len`). Returns `true` when a split happened.
///
/// This is how RDM "accelerates" a slow connection: while a download is live,
/// keeping one more connection busy on the biggest remaining chunk usually
/// beats a static plan when server throughput is uneven.
pub fn split_largest(segments: &mut Vec<Segment>, min_len: u64) -> bool {
    let candidate = segments
        .iter()
        .enumerate()
        .filter(|(_, s)| s.status != SegmentStatus::Done && s.remaining() >= min_len.saturating_mul(2))
        .max_by_key(|(_, s)| s.remaining())
        .map(|(i, _)| i);

    let Some(idx) = candidate else {
        return false;
    };

    let seg = segments[idx].clone();
    let remaining = seg.remaining();
    let first_len = remaining / 2;

    // The already-downloaded prefix stays with the first half; the second half
    // becomes a brand new segment starting right after it.
    let split_at = seg.start + seg.downloaded + first_len;
    if split_at <= seg.start + seg.downloaded || split_at > seg.end {
        return false;
    }

    let tail = Segment {
        index: 0,
        start: split_at,
        end: seg.end,
        downloaded: 0,
        status: SegmentStatus::Pending,
    };

    segments[idx].end = split_at - 1;
    segments.insert(idx + 1, tail);
    reindex(segments);
    true
}

/// Total bytes covered by a plan (sanity/accounting helper).
pub fn total_len(segments: &[Segment]) -> u64 {
    segments.iter().map(Segment::len).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_tiles(segments: &[Segment], total: u64) {
        assert_eq!(total_len(segments), total, "segments must cover every byte");
        assert_eq!(
            segments.first().unwrap().start,
            0,
            "plan must start at byte 0"
        );
        assert_eq!(
            segments.last().unwrap().end,
            total - 1,
            "plan must end at the last byte"
        );
        for pair in segments.windows(2) {
            assert_eq!(
                pair[0].end + 1,
                pair[1].start,
                "segments must be contiguous"
            );
        }
    }

    #[test]
    fn empty_total_has_no_segments() {
        assert!(plan(0, 8).is_empty());
    }

    #[test]
    fn even_split() {
        let segs = plan(1000, 4);
        assert_eq!(segs.len(), 4);
        assert!(segs.iter().all(|s| s.len() == 250));
        assert_tiles(&segs, 1000);
    }

    #[test]
    fn remainder_is_distributed() {
        let segs = plan(1001, 4);
        assert_eq!(segs.len(), 4);
        let lens: Vec<u64> = segs.iter().map(Segment::len).collect();
        assert_eq!(lens, vec![251, 250, 250, 250]);
        assert_tiles(&segs, 1001);
    }

    #[test]
    fn clamps_connections_to_byte_count() {
        let segs = plan(3, 8);
        assert_eq!(segs.len(), 3);
        assert_tiles(&segs, 3);
    }

    #[test]
    fn single_connection_is_one_segment() {
        let segs = plan(10_000, 1);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].start, 0);
        assert_eq!(segs[0].end, 9999);
    }

    #[test]
    fn split_largest_keeps_coverage() {
        let mut segs = plan(1000, 2);
        assert!(split_largest(&mut segs, 1));
        assert_eq!(segs.len(), 3);
        assert_tiles(&segs, 1000);
        assert_eq!(segs[0].index, 0);
        assert_eq!(segs[1].index, 1);
        assert_eq!(segs[2].index, 2);
    }

    #[test]
    fn split_respects_downloaded_prefix() {
        let mut segs = plan(1000, 1);
        segs[0].downloaded = 400;
        assert!(split_largest(&mut segs, 10));
        // The already-fetched prefix must not move.
        assert_eq!(segs[0].start + segs[0].downloaded, 400);
        assert_tiles(&segs, 1000);
        assert_eq!(segs[0].downloaded, 400);
    }

    #[test]
    fn split_declines_when_segments_are_small() {
        let mut segs = plan(100, 8);
        assert!(!split_largest(&mut segs, 64));
        assert_eq!(segs.len(), 8);
    }

    #[test]
    fn next_range_advances() {
        let mut seg = Segment {
            index: 0,
            start: 0,
            end: 99,
            downloaded: 0,
            status: SegmentStatus::Pending,
        };
        assert_eq!(seg.next_range(), Some((0, 99)));
        seg.downloaded = 40;
        assert_eq!(seg.next_range(), Some((40, 99)));
        seg.downloaded = 100;
        assert_eq!(seg.next_range(), None);
        assert!(seg.is_done());
        assert_eq!(seg.remaining(), 0);
    }
}
