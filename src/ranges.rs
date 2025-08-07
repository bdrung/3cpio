// Copyright (C) 2025, Benjamin Drung <bdrung@posteo.de>
// SPDX-License-Identifier: ISC

use std::num::ParseIntError;
use std::ops::{RangeFrom, RangeInclusive, RangeTo};
use std::str::FromStr;

#[derive(Debug, PartialEq)]
struct Range {
    start: Option<i32>,
    end: Option<i32>,
}

impl Range {
    fn new(start: Option<i32>, end: Option<i32>) -> Self {
        Self { start, end }
    }

    fn contains(&self, item: &i32) -> bool {
        if let Some(start) = self.start {
            if *item < start {
                return false;
            }
        }
        if let Some(end) = self.end {
            if *item > end {
                return false;
            }
        }
        true
    }

    fn has_more(&self, item: &i32) -> bool {
        self.end.is_none_or(|end| end > *item)
    }
}

impl From<RangeInclusive<i32>> for Range {
    fn from(item: RangeInclusive<i32>) -> Self {
        Self {
            start: Some(*item.start()),
            end: Some(*item.end()),
        }
    }
}

impl From<RangeFrom<i32>> for Range {
    fn from(item: RangeFrom<i32>) -> Self {
        Self {
            start: Some(item.start),
            end: None,
        }
    }
}

impl From<RangeTo<i32>> for Range {
    fn from(item: RangeTo<i32>) -> Self {
        Self {
            start: None,
            end: Some(item.end),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Ranges(Vec<Range>);

impl Ranges {
    #[cfg(test)]
    fn new(ranges: Vec<Range>) -> Self {
        Self(ranges)
    }

    pub fn contains(&self, item: &i32) -> bool {
        for range in &self.0 {
            if range.contains(item) {
                return true;
            }
        }
        false
    }

    pub fn has_more(&self, item: &i32) -> bool {
        for range in &self.0 {
            if range.has_more(item) {
                return true;
            }
        }
        false
    }
}

impl FromStr for Ranges {
    type Err = ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut ranges = Vec::new();
        for range_str in s.split(",") {
            if let Some((start, end)) = range_str.split_once("-") {
                let start = if start.is_empty() {
                    None
                } else {
                    Some(start.parse()?)
                };
                let end = if end.is_empty() {
                    None
                } else {
                    Some(end.parse()?)
                };
                ranges.push(Range::new(start, end));
            } else {
                let start = range_str.parse()?;
                ranges.push(Range::new(Some(start), Some(start)));
            }
        }
        Ok(Self(ranges))
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn test_parse_ranges_error_single() {
        for s in ["str", "1-str", "str-5"] {
            let got = s.parse::<Ranges>().unwrap_err();
            assert_eq!(got.to_string(), "invalid digit found in string");
        }
    }

    #[test]
    fn test_parse_ranges_single() {
        assert_eq!("3".parse::<Ranges>(), Ok(Ranges::new(vec![(3..=3).into()])))
    }

    #[test]
    fn test_parse_ranges_range() {
        assert_eq!(
            "2-4".parse::<Ranges>(),
            Ok(Ranges::new(vec![(2..=4).into()]))
        )
    }

    #[test]
    fn test_parse_ranges_multiple() {
        assert_eq!(
            "1,3-5".parse::<Ranges>(),
            Ok(Ranges::new(vec![(1..=1).into(), (3..=5).into()]))
        )
    }

    #[test]
    fn test_parse_ranges_open_end() {
        assert_eq!("2-".parse::<Ranges>(), Ok(Ranges::new(vec![(2..).into()])))
    }

    #[test]
    fn test_parse_ranges_open_start() {
        assert_eq!("-4".parse::<Ranges>(), Ok(Ranges::new(vec![(..4).into()])))
    }

    #[test]
    fn test_ranges_contains() {
        let ranges = "1-3,5".parse::<Ranges>().unwrap();
        assert!(ranges.contains(&2));
        assert!(!ranges.contains(&4));
    }

    #[test]
    fn test_ranges_has_more() {
        let ranges = "4-5,7,-2".parse::<Ranges>().unwrap();
        assert!(ranges.has_more(&6));
        assert!(!ranges.has_more(&7));
    }
}
