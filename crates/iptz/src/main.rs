//! `iptz-gen` - builds an IPTZ asset from a CSV of IP ranges mapped to lat-lon.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read};
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use csv::{ReaderBuilder, StringRecord};
use iptz::Table;
use tzf_rs::DefaultFinder;

const COL_IP_START: usize = 0;
const COL_LAT: usize = 6;
const COL_LON: usize = 7;

type Ranges = Vec<(u32, u16)>;

/// The CSV source. A file or a standard input stream.
enum Input {
    File(File),
    Stdin(io::StdinLock<'static>),
}

impl Read for Input {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Input::File(file) => file.read(buf),
            Input::Stdin(stdin) => stdin.read(buf),
        }
    }
}

fn main() -> Result<()> {
    let mut args = pico_args::Arguments::from_env();
    let default_out = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/iptz.bin");

    let csv: Option<PathBuf> = args.opt_value_from_str("--csv")?;
    let out: PathBuf = args.opt_value_from_str("--out")?.unwrap_or(default_out);

    let rest = args.finish();
    if !rest.is_empty() {
        bail!("usage: iptz-gen [--csv <path>] [--out <path>]");
    }

    let reader = match &csv {
        None => Input::Stdin(io::stdin().lock()),
        Some(path) => Input::File(
            File::open(path).with_context(|| format!("opening {}", path.display()))?,
        ),
    };

    let table = ZoneResolver::read_table(reader)?;
    let bytes = table.encode()?;
    std::fs::write(&out, &bytes).with_context(|| format!("writing {}", out.display()))?;

    eprintln!(
        "wrote {} ({} zones), {} entries, {:.2} MB",
        out.display(),
        table.zone_names().count(),
        table.len(),
        bytes.len() as f64 / 1_048_576.0,
    );
    Ok(())
}

/// Resolves geographical coordinates to timezone IDs. Zones are interned and lookups are cached.
struct ZoneResolver {
    finder: DefaultFinder,
    names: Vec<String>,
    index: HashMap<String, u16>,
    cache: HashMap<(i32, i32), u16>,
}

impl ZoneResolver {
    fn new() -> Self {
        Self {
            finder: DefaultFinder::new(),
            names: vec![String::new()],
            index: HashMap::from([(String::new(), 0)]),
            cache: HashMap::new(),
        }
    }

    fn read_table(reader: impl Read) -> Result<Table> {
        eprintln!("resolving coordinates...");
        let mut resolver = Self::new();
        let mut ranges: Ranges = Vec::new();

        let mut csv = ReaderBuilder::new().has_headers(false).from_reader(reader);
        let mut record = StringRecord::new();
        while csv.read_record(&mut record)? {
            if let Some(entry) = resolver.parse_row(&record) {
                ranges.push(entry);
            }
        }

        Ok(Table::from_ranges(resolver.names, ranges))
    }

    fn parse_row(&mut self, record: &StringRecord) -> Option<(u32, u16)> {
        let ip = record.get(COL_IP_START)?.parse::<Ipv4Addr>().ok()?    ; // reject IPv6
        let (lat, lon) = (record.get(COL_LAT)?, record.get(COL_LON)?);

        let zone = match (lat.parse::<f64>(), lon.parse::<f64>()) {
            (Ok(lat), Ok(lon)) => self.resolve(lat, lon),
            _ => 0, // unknown coordinates
        };

        Some((u32::from(ip), zone))
    }

    fn resolve(&mut self, lat: f64, lon: f64) -> u16 {
        let key = ((lat * 100.0).round() as i32, (lon * 100.0).round() as i32);
        if let Some(&idx) = self.cache.get(&key) {
            return idx;
        }

        // tzf-rs expects (lon, lat)
        let name = self
            .finder
            .get_tz_name(f64::from(key.1) / 100.0, f64::from(key.0) / 100.0)
            .to_owned();

        let idx = match self.index.get(&name) {
            Some(&idx) => idx,
            None => {
                let idx = self.names.len() as u16;
                self.index.insert(name.clone(), idx);
                self.names.push(name);
                idx
            }
        };

        self.cache.insert(key, idx);
        idx
    }
}
