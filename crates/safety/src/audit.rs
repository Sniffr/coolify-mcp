use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::{self, Write};

#[derive(Clone, Debug, Serialize)]
pub struct AuditEvent {
    pub timestamp: String,
    pub client_id: Option<String>,
    pub tool: String,
    pub resource_id: String,
    pub outcome: String,
    pub status: u16,
    pub duration_ms: u64,
}

impl AuditEvent {
    pub fn new(
        timestamp: impl Into<String>,
        client_id: Option<&str>,
        tool: impl Into<String>,
        resource_id: impl Into<String>,
        outcome: impl Into<String>,
        status: u16,
        duration_ms: u64,
    ) -> Self {
        Self {
            timestamp: timestamp.into(),
            client_id: client_id.map(str::to_owned),
            tool: tool.into(),
            resource_id: resource_id.into(),
            outcome: outcome.into(),
            status,
            duration_ms,
        }
    }
}

#[derive(Serialize)]
struct AuditRecord<'a> {
    #[serde(flatten)]
    event: &'a AuditEvent,
    previous_hash: &'a str,
    hash: String,
}

pub struct AuditLogger<W: Write> {
    writer: W,
    previous_hash: String,
}

impl<W: Write> AuditLogger<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            previous_hash: String::new(),
        }
    }

    pub fn record(&mut self, event: AuditEvent) -> io::Result<String> {
        let canonical = serde_json::to_vec(&event).map_err(io::Error::other)?;
        let mut hasher = Sha256::new();
        hasher.update(self.previous_hash.as_bytes());
        hasher.update(&canonical);
        let hash = format!("{:x}", hasher.finalize());
        let record = AuditRecord {
            event: &event,
            previous_hash: &self.previous_hash,
            hash: hash.clone(),
        };
        serde_json::to_writer(&mut self.writer, &record).map_err(io::Error::other)?;
        self.writer.write_all(b"\n")?;
        self.writer.flush()?;
        self.previous_hash = hash.clone();
        Ok(hash)
    }
}
