// Aggregator module - buffers raw metric samples and produces aggregated documents
//
// MetricBuffer: for metrics with flat numeric fields (LoadAverage, Memory, DiskSpace)
// DockerMetricBuffer: for DockerStats which uses a nested containers array

use std::collections::HashMap;
use bson::{doc, Bson, Document};
use chrono::Utc;

// These fields are stored as plain values rather than {avg, min, max}
// because they are constant within a collection window.
const PASSTHROUGH_FIELDS: &[&str] = &["cpu_cores", "total_mb", "swap_total_mb"];

// ---------------------------------------------------------------------------
// MetricBuffer
// ---------------------------------------------------------------------------

pub struct MetricBuffer {
    samples: Vec<HashMap<String, f64>>,
    last_raw: Option<Document>,
}

impl MetricBuffer {
    pub fn new() -> Self {
        MetricBuffer {
            samples: Vec::new(),
            last_raw: None,
        }
    }

    /// Push a raw collected document into the buffer.
    /// Extracts top-level numeric fields; non-numeric fields (arrays, subdocs) are skipped.
    pub fn push(&mut self, doc: &Document) {
        self.last_raw = Some(doc.clone());

        let mut map = HashMap::new();
        for (key, val) in doc.iter() {
            if key == "node" || key == "timestamp" {
                continue;
            }
            let num = match val {
                Bson::Double(v)  => Some(*v),
                Bson::Int32(v)   => Some(*v as f64),
                Bson::Int64(v)   => Some(*v as f64),
                _                => None,
            };
            if let Some(n) = num {
                map.insert(key.clone(), n);
            }
        }

        if !map.is_empty() {
            self.samples.push(map);
        }
    }

    /// Flush the buffer and return an aggregated document, or None if insufficient data.
    ///
    /// - If 2+ samples with numeric fields: returns aggregated doc with avg/min/max per field
    ///   (passthrough fields stored as plain values preserving their original BSON type).
    /// - If no numeric samples (e.g. DiskSpace): returns the last raw document as-is,
    ///   with an updated timestamp.
    /// - If never collected: returns None.
    pub fn flush(&mut self, node_id: &str) -> Option<Document> {
        if self.samples.len() >= 2 {
            let field_names: Vec<String> = {
                let mut set = std::collections::HashSet::new();
                for s in &self.samples {
                    for k in s.keys() {
                        set.insert(k.clone());
                    }
                }
                let mut v: Vec<String> = set.into_iter().collect();
                v.sort();
                v
            };

            let sample_count = self.samples.len() as i32;
            let mut result = doc! {
                "node": node_id,
                "timestamp": Utc::now(),
                "sample_count": sample_count,
            };

            for field in &field_names {
                let values: Vec<f64> = self.samples.iter()
                    .filter_map(|s| s.get(field).copied())
                    .collect();
                if values.is_empty() {
                    continue;
                }

                if PASSTHROUGH_FIELDS.contains(&field.as_str()) {
                    // Constant field — store first sample value with original BSON type
                    result.insert(field, bson_for_passthrough(field, values[0]));
                } else {
                    let avg = values.iter().sum::<f64>() / values.len() as f64;
                    let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
                    let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                    result.insert(field, doc! { "avg": avg, "min": min, "max": max });
                }
            }

            self.samples.clear();
            self.last_raw = None;
            return Some(result);
        }

        // No numeric samples — fall back to returning the last raw document (e.g. DiskSpace)
        self.samples.clear();
        if let Some(mut raw) = self.last_raw.take() {
            raw.insert("timestamp", Utc::now());
            Some(raw)
        } else {
            None
        }
    }
}

fn bson_for_passthrough(field: &str, value: f64) -> Bson {
    match field {
        "cpu_cores" => Bson::Int32(value as i32),
        _           => Bson::Int64(value as i64), // total_mb, swap_total_mb
    }
}

// ---------------------------------------------------------------------------
// DockerMetricBuffer
// ---------------------------------------------------------------------------

struct ContainerSample {
    id: String,
    cpu_percent: f64,
    memory_used_mb: f64,
    memory_limit_mb: f64,
    memory_percent: f64,
    network_rx_mb: f64,
    network_tx_mb: f64,
    block_read_mb: f64,
    block_write_mb: f64,
}

pub struct DockerMetricBuffer {
    // container name → ordered list of per-tick samples
    container_samples: HashMap<String, Vec<ContainerSample>>,
    last_raw: Option<Document>,
}

impl DockerMetricBuffer {
    pub fn new() -> Self {
        DockerMetricBuffer {
            container_samples: HashMap::new(),
            last_raw: None,
        }
    }

    pub fn push(&mut self, doc: &Document) {
        self.last_raw = Some(doc.clone());

        let containers = match doc.get_array("containers") {
            Ok(arr) => arr,
            Err(_) => return,
        };

        for item in containers {
            let c = match item.as_document() {
                Some(d) => d,
                None => continue,
            };

            let name = match c.get_str("name") {
                Ok(n) => n.to_string(),
                Err(_) => continue,
            };

            let sample = ContainerSample {
                id:               get_str(c, "id"),
                cpu_percent:      get_f64(c, "cpu_percent"),
                memory_used_mb:   get_f64(c, "memory_used_mb"),
                memory_limit_mb:  get_f64(c, "memory_limit_mb"),
                memory_percent:   get_f64(c, "memory_percent"),
                network_rx_mb:    get_f64(c, "network_rx_mb"),
                network_tx_mb:    get_f64(c, "network_tx_mb"),
                block_read_mb:    get_f64(c, "block_read_mb"),
                block_write_mb:   get_f64(c, "block_write_mb"),
            };

            self.container_samples
                .entry(name)
                .or_default()
                .push(sample);
        }
    }

    pub fn flush(&mut self, node_id: &str) -> Option<Document> {
        if self.container_samples.is_empty() {
            return self.last_raw.take().map(|mut raw| {
                raw.insert("timestamp", Utc::now());
                raw
            });
        }

        // sample_count = longest container sample list
        let sample_count = self.container_samples.values()
            .map(|v| v.len())
            .max()
            .unwrap_or(0) as i32;

        let mut container_docs: Vec<Bson> = self.container_samples
            .iter()
            .map(|(name, samples)| {
                let n = samples.len();

                // avg/min/max for variable fields
                let (cpu_avg, cpu_min, cpu_max) = stats(samples.iter().map(|s| s.cpu_percent));
                let (mem_used_avg, mem_used_min, mem_used_max) = stats(samples.iter().map(|s| s.memory_used_mb));
                let (mem_pct_avg, mem_pct_min, mem_pct_max) = stats(samples.iter().map(|s| s.memory_percent));

                // constant per container
                let memory_limit_mb = samples[0].memory_limit_mb;
                let id = samples[0].id.clone();

                // last-sample cumulative counters
                let last = &samples[n - 1];

                Bson::Document(doc! {
                    "id":               id,
                    "name":             name,
                    "memory_limit_mb":  memory_limit_mb,
                    "cpu_percent": {
                        "avg": cpu_avg, "min": cpu_min, "max": cpu_max
                    },
                    "memory_used_mb": {
                        "avg": mem_used_avg, "min": mem_used_min, "max": mem_used_max
                    },
                    "memory_percent": {
                        "avg": mem_pct_avg, "min": mem_pct_min, "max": mem_pct_max
                    },
                    "network_rx_mb":  last.network_rx_mb,
                    "network_tx_mb":  last.network_tx_mb,
                    "block_read_mb":  last.block_read_mb,
                    "block_write_mb": last.block_write_mb,
                })
            })
            .collect();

        // Sort by container name for consistent ordering
        container_docs.sort_by(|a, b| {
            let name_a = a.as_document().and_then(|d| d.get_str("name").ok()).unwrap_or("");
            let name_b = b.as_document().and_then(|d| d.get_str("name").ok()).unwrap_or("");
            name_a.cmp(name_b)
        });

        let result = doc! {
            "node":         node_id,
            "timestamp":    Utc::now(),
            "sample_count": sample_count,
            "containers":   container_docs,
        };

        self.container_samples.clear();
        self.last_raw = None;
        Some(result)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn stats(iter: impl Iterator<Item = f64>) -> (f64, f64, f64) {
    let values: Vec<f64> = iter.collect();
    if values.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let avg = values.iter().sum::<f64>() / values.len() as f64;
    let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    (avg, min, max)
}

fn get_f64(doc: &Document, key: &str) -> f64 {
    match doc.get(key) {
        Some(Bson::Double(v))  => *v,
        Some(Bson::Int32(v))   => *v as f64,
        Some(Bson::Int64(v))   => *v as f64,
        _                      => 0.0,
    }
}

fn get_str(doc: &Document, key: &str) -> String {
    doc.get_str(key).unwrap_or("").to_string()
}
