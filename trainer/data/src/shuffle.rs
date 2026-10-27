//! `data shuffle` and `data interleave`: mixing whole records (games for the chain formats, positions for
//! bulletformat) without decoding them, within a memory budget.
//!
//! Shuffling a set larger than the budget is two passes: every record goes to one of `k` temporary buckets at random
//! (`k` sized so a bucket fits the budget), then each bucket is loaded, shuffled and appended to the output. That is
//! a uniform shuffle of the whole set. Interleaving streams the inputs, taking the next record from an input chosen
//! with probability proportional to the bytes it has left, so the inputs end together and memory stays constant.

use std::{
    fs::File,
    io::{BufRead, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::{
    formats::{Format, bullet, rbinpack, viri},
    rng::Rng,
};

/// Per-record bookkeeping counted against the memory budget, beside the record's bytes.
const RECORD_OVERHEAD: u64 = 40;

pub fn read_record(input: &mut impl BufRead, format: Format) -> Result<Option<Vec<u8>>> {
    match format {
        Format::Viriformat => viri::read_raw(input),
        Format::Rbinpack => rbinpack::read_raw(input),
        Format::Bulletformat => Ok(bullet::read(input)?.map(|r| r.to_vec())),
        Format::Binpack | Format::Pgn => {
            bail!("{} cannot be shuffled record by record: convert it to viriformat first", format.name())
        }
    }
}

#[derive(Debug, Default, Serialize)]
pub struct MixReport {
    pub records: u64,
    pub bytes: u64,
    /// Temporary buckets used (1 when everything fit in memory).
    pub buckets: u64,
}

fn check_inputs(inputs: &[PathBuf], format: Format) -> Result<u64> {
    let mut total = 0;
    for input in inputs {
        let f = Format::from_path(input);
        if f.is_some() && f != Some(format) {
            bail!("{} is not {}: every input must be in the output's format", input.display(), format.name());
        }
        total += std::fs::metadata(input).with_context(|| format!("{}", input.display()))?.len();
    }
    Ok(total)
}

fn open(path: &Path) -> Result<BufReader<File>> {
    Ok(BufReader::with_capacity(1 << 20, File::open(path).with_context(|| format!("open {}", path.display()))?))
}

pub fn shuffle(inputs: &[PathBuf], output: &Path, format: Format, memory: u64, seed: u64) -> Result<MixReport> {
    let total = check_inputs(inputs, format)?;
    let mut rng = Rng::new(seed);
    let mut out = BufWriter::with_capacity(1 << 20, File::create(output)?);
    let mut report = MixReport::default();

    // A rough count of records for the overhead: assume the smallest record of the format.
    let min_record = match format {
        Format::Bulletformat => 32,
        _ => 64,
    };
    let estimated = total + (total / min_record) * RECORD_OVERHEAD;
    let buckets = estimated.div_ceil(memory.max(1)).max(1);
    // Leave headroom for uneven buckets.
    let buckets = if buckets > 1 { buckets + buckets / 4 + 1 } else { 1 };
    report.buckets = buckets;

    if buckets == 1 {
        let mut records = Vec::new();
        for input in inputs {
            let mut r = open(input)?;
            while let Some(rec) = read_record(&mut r, format)? {
                records.push(rec);
            }
        }
        rng.shuffle(&mut records);
        for rec in &records {
            out.write_all(rec)?;
            report.records += 1;
            report.bytes += rec.len() as u64;
        }
        out.flush()?;
        return Ok(report);
    }

    let tmp = output.with_extension("shuffle-tmp");
    std::fs::create_dir_all(&tmp)?;
    let result = (|| -> Result<()> {
        let paths: Vec<PathBuf> = (0..buckets).map(|i| tmp.join(format!("bucket-{i}"))).collect();
        {
            let mut writers = paths
                .iter()
                .map(|p| Ok(BufWriter::with_capacity(1 << 16, File::create(p)?)))
                .collect::<Result<Vec<_>>>()?;
            for input in inputs {
                let mut r = open(input)?;
                while let Some(rec) = read_record(&mut r, format)? {
                    writers[rng.below(buckets) as usize].write_all(&rec)?;
                }
            }
            for w in &mut writers {
                w.flush()?;
            }
        }
        for path in &paths {
            let mut records = Vec::new();
            let mut r = open(path)?;
            while let Some(rec) = read_record(&mut r, format)? {
                records.push(rec);
            }
            drop(r);
            rng.shuffle(&mut records);
            for rec in &records {
                out.write_all(rec)?;
                report.records += 1;
                report.bytes += rec.len() as u64;
            }
            std::fs::remove_file(path)?;
        }
        out.flush()?;
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&tmp);
    result.map(|_| report)
}

pub fn interleave(inputs: &[PathBuf], output: &Path, format: Format, seed: u64) -> Result<MixReport> {
    check_inputs(inputs, format)?;
    let mut rng = Rng::new(seed);
    let mut out = BufWriter::with_capacity(1 << 20, File::create(output)?);
    let mut sources = Vec::new();
    for input in inputs {
        let left = std::fs::metadata(input)?.len();
        if left > 0 {
            sources.push((open(input)?, left));
        }
    }
    let mut report = MixReport { buckets: 1, ..Default::default() };
    while !sources.is_empty() {
        let total: u64 = sources.iter().map(|s| s.1).sum();
        let mut pick = rng.below(total.max(1));
        let mut index = 0;
        while index + 1 < sources.len() && pick >= sources[index].1 {
            pick -= sources[index].1;
            index += 1;
        }
        match read_record(&mut sources[index].0, format)? {
            Some(rec) => {
                sources[index].1 = sources[index].1.saturating_sub(rec.len() as u64).max(1);
                out.write_all(&rec)?;
                report.records += 1;
                report.bytes += rec.len() as u64;
            }
            None => {
                sources.swap_remove(index);
            }
        }
    }
    out.flush()?;
    Ok(report)
}
