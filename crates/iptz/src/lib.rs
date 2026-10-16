#![doc = include_str!("../README.md")]

use std::net::Ipv4Addr;

/// The primary data structure. A map of IPv4 addresses to IANA timezones.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Table {
    names: Vec<String>,
    #[cfg_attr(feature = "serde", serde(with = "delta"))]
    starts: Vec<u32>,
    zones: Vec<u16>,
}

impl Table {
    /// Returns the IANA timezone matching the provided `ip`, or `None` if unknown.
    pub fn lookup(&self, ip: Ipv4Addr) -> Option<&str> {
        let key = u32::from(ip);
        let i = self.starts.partition_point(|&start| start <= key).checked_sub(1)?;
        let name = self.names.get(usize::from(self.zones[i]))?;
        (!name.is_empty()).then_some(name.as_str())
    }

    /// Number of ranges in the table.
    pub fn len(&self) -> usize {
        self.starts.len()
    }

    /// Whether the table has no ranges.
    pub fn is_empty(&self) -> bool {
        self.starts.is_empty()
    }

    /// The unique, known zone names the table can return.
    pub fn zone_names(&self) -> impl Iterator<Item = &str> {
        self.names.iter().map(String::as_str).filter(|n| !n.is_empty())
    }
}

/// In order to make our encoded size efficient, we store the `Vec<u32>` data as
/// deltas / gaps between its byte values rather than 4 entire bytes. Our range
/// mostly starts with big numbers, but the rest are small increments on top of
/// those. Further, the varint format packs each delta into 1-2 bytes rather than
/// the full 4.
///
/// In testing, it reduced the compressed artifact size by ~45% (from 1.47 MB to
/// 0.81MB).
///
/// Usually consumed by applying `#[serde(with = "delta")]` to a field.
#[cfg(feature = "serde")]
mod delta {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(starts: &[u32], serializer: S) -> Result<S::Ok, S::Error> {
        let mut prev = 0u32;
        let gaps = starts
            .iter()
            .map(|&start| {
                let gap = start.wrapping_sub(prev);
                prev = start;
                gap
            })
            .collect::<Vec<u32>>();
        gaps.serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<u32>, D::Error> {
        let gaps = Vec::deserialize(deserializer)?;
        let mut acc = 0u32;
        Ok(gaps
            .into_iter()
            .map(|gap| {
                acc = acc.wrapping_add(gap);
                acc
            })
            .collect())
    }
}

#[cfg(feature = "codec")]
impl Table {
    /// Decodes an IPTZ container produced by [`Table::encode`].
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        use std::io::Read;

        let mut inflated = Vec::new();
        flate2::read::DeflateDecoder::new(bytes).read_to_end(&mut inflated)?;
        Ok(postcard::from_bytes(&inflated)?)
    }

    /// Encodes the table to an IPTZ container, backed by `postcard` serialization
    /// and then compressed by DEFLATE.
    pub fn encode(&self) -> Result<Vec<u8>, Error> {
        use std::io::Write;

        let serialized = postcard::to_stdvec(self)?;
        let mut encoder =
            flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::best());
        encoder.write_all(&serialized)?;
        Ok(encoder.finish()?)
    }
}

#[cfg(feature = "generate")]
impl Table {
    /// Builds a table provided unsorted ranges (start IP, zone index) and a list
    /// of zone names.
    ///
    /// It sorts by start IP, groups ranges that share the same zones, and orders
    /// by concentration of ranges per zone, so that hot zones are the earliest
    /// in the table.
    pub fn from_ranges(names: Vec<String>, mut ranges: Vec<(u32, u16)>) -> Self {
        ranges.sort_by_key(|&(ip, _)| ip);

        let mut starts = Vec::with_capacity(ranges.len());
        let mut zones = Vec::with_capacity(ranges.len());
        for (ip, zone) in ranges {
            if zones.last() != Some(&zone) {
                starts.push(ip);
                zones.push(zone);
            }
        }

        let (names, zones) = arrange_by_frequency(names, zones);
        Self { names, starts, zones }
    }
}

/// Rearranges `names` so that the most referenced zones are the earliest in the
/// `Vec`. Updates the `zones` to match.
#[cfg(feature = "generate")]
fn arrange_by_frequency(names: Vec<String>, zones: Vec<u16>) -> (Vec<String>, Vec<u16>) {
    let mut freq = vec![0u64; names.len()];
    for &z in &zones {
        freq[usize::from(z)] += 1;
    }

    let mut order = (0..names.len()).collect::<Vec<usize>>();
    order.sort_by(|&a, &b| freq[b].cmp(&freq[a]).then(a.cmp(&b)));

    let mut remap = vec![0u16; names.len()];
    let mut reordered = Vec::with_capacity(names.len());
    for (new_idx, &old_idx) in order.iter().enumerate() {
        remap[old_idx] = new_idx as u16;
        reordered.push(names[old_idx].clone());
    }

    let zones = zones.iter().map(|&z| remap[usize::from(z)]).collect();
    (reordered, zones)
}

/// An error raised while encoding or decoding a [`Table`].
#[cfg(feature = "codec")]
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// DEFLATE compression or decompression failed.
    #[error("iptz compression error: {0}")]
    Io(#[from] std::io::Error),

    /// `postcard` serialization or deserialization failed.
    #[error("iptz serialization error: {0}")]
    Postcard(#[from] postcard::Error),
}

#[cfg(all(test, feature = "generate"))]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_looks_up() {
        let names = vec![String::new(), "Asia/Tokyo".to_string(), "Etc/GMT".to_string()];

        let ranges = vec![
            (0, 0),   // 0.0.0.0 = unknown
            (50, 1),  // Tokyo
            (60, 1),  // should merge
            (100, 2), // GMT
            (10, 0),  // should also merge
        ];

        let table = Table::from_ranges(names, ranges);
        let decoded = Table::decode(&table.encode().unwrap()).unwrap();

        assert_eq!(decoded.len(), 3);
        assert_eq!(decoded.lookup(Ipv4Addr::from(0)), None);
        assert_eq!(decoded.lookup(Ipv4Addr::from(49)), None);
        assert_eq!(decoded.lookup(Ipv4Addr::from(50)), Some("Asia/Tokyo"));
        assert_eq!(decoded.lookup(Ipv4Addr::from(99)), Some("Asia/Tokyo"));
        assert_eq!(decoded.lookup(Ipv4Addr::from(100)), Some("Etc/GMT"));
        assert_eq!(decoded.lookup(Ipv4Addr::from(u32::MAX)), Some("Etc/GMT"));
    }

    #[test]
    fn frequency_order_gives_hot_zones_small_indices() {
        let names = vec![String::new(), "Rare".to_string(), "Etc/GMT".to_string()];
        let ranges = vec![(0, 2), (10, 1), (20, 2), (30, 1), (40, 2)];
        let table = Table::from_ranges(names, ranges);

        assert_eq!(table.names[usize::from(table.zones[0])], "Etc/GMT");
        assert_eq!(table.names[0], "Etc/GMT");
    }
}
