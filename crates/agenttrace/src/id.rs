//! Trace and span identifiers.
//!
//! Identifiers follow OpenTelemetry conventions so AgentTrace data maps cleanly
//! onto the wider tracing ecosystem:
//! * `TraceId` is 16 random bytes, rendered as 32 lowercase hex characters.
//! * `SpanId` is 8 random bytes, rendered as 16 lowercase hex characters.
//!
//! IDs are generated from a CSPRNG. They are not sequential or guessable, which
//! keeps them collision-safe across concurrent runs and processes.

use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::fmt;

/// A 128-bit trace identifier (32 hex chars), shared by every span in one run.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct TraceId([u8; 16]);

/// A 64-bit span identifier (16 hex chars), unique to a single span.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct SpanId([u8; 8]);

impl TraceId {
    /// Generate a new random, non-zero trace id.
    pub fn generate() -> Self {
        let mut bytes = [0u8; 16];
        loop {
            rand::thread_rng().fill_bytes(&mut bytes);
            if bytes != [0u8; 16] {
                break;
            }
        }
        TraceId(bytes)
    }

    /// The all-zero trace id, treated as "invalid / none" per OTel.
    pub fn invalid() -> Self {
        TraceId([0u8; 16])
    }

    pub fn is_valid(&self) -> bool {
        self.0 != [0u8; 16]
    }

    pub fn to_hex(&self) -> String {
        hex(&self.0)
    }

    pub fn from_hex(s: &str) -> Result<Self, IdError> {
        let bytes: [u8; 16] = from_hex(s)?;
        Ok(TraceId(bytes))
    }
}

impl SpanId {
    /// Generate a new random, non-zero span id.
    pub fn generate() -> Self {
        let mut bytes = [0u8; 8];
        loop {
            rand::thread_rng().fill_bytes(&mut bytes);
            if bytes != [0u8; 8] {
                break;
            }
        }
        SpanId(bytes)
    }

    pub fn invalid() -> Self {
        SpanId([0u8; 8])
    }

    pub fn is_valid(&self) -> bool {
        self.0 != [0u8; 8]
    }

    pub fn to_hex(&self) -> String {
        hex(&self.0)
    }

    pub fn from_hex(s: &str) -> Result<Self, IdError> {
        let bytes: [u8; 8] = from_hex(s)?;
        Ok(SpanId(bytes))
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum IdError {
    #[error("identifier has wrong length: expected {expected} hex chars, got {got}")]
    WrongLength { expected: usize, got: usize },
    #[error("identifier contains non-hex characters")]
    NotHex,
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(char::from_digit((b >> 4) as u32, 16).unwrap());
        s.push(char::from_digit((b & 0xf) as u32, 16).unwrap());
    }
    s
}

fn from_hex<const N: usize>(s: &str) -> Result<[u8; N], IdError> {
    if s.len() != N * 2 {
        return Err(IdError::WrongLength {
            expected: N * 2,
            got: s.len(),
        });
    }
    let mut out = [0u8; N];
    let bytes = s.as_bytes();
    for i in 0..N {
        let hi = (bytes[i * 2] as char).to_digit(16).ok_or(IdError::NotHex)?;
        let lo = (bytes[i * 2 + 1] as char)
            .to_digit(16)
            .ok_or(IdError::NotHex)?;
        out[i] = ((hi << 4) | lo) as u8;
    }
    Ok(out)
}

// Hex string is the canonical serialized form for both ids.
impl fmt::Display for TraceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}
impl fmt::Debug for TraceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TraceId({})", self.to_hex())
    }
}
impl fmt::Display for SpanId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}
impl fmt::Debug for SpanId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SpanId({})", self.to_hex())
    }
}

impl Serialize for TraceId {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_hex())
    }
}
impl<'de> Deserialize<'de> for TraceId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        TraceId::from_hex(&s).map_err(serde::de::Error::custom)
    }
}
impl Serialize for SpanId {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_hex())
    }
}
impl<'de> Deserialize<'de> for SpanId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        SpanId::from_hex(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_id_roundtrips_through_hex() {
        let id = TraceId::generate();
        let hex = id.to_hex();
        assert_eq!(hex.len(), 32);
        assert_eq!(TraceId::from_hex(&hex).unwrap(), id);
    }

    #[test]
    fn span_id_roundtrips_through_hex() {
        let id = SpanId::generate();
        let hex = id.to_hex();
        assert_eq!(hex.len(), 16);
        assert_eq!(SpanId::from_hex(&hex).unwrap(), id);
    }

    #[test]
    fn generated_ids_are_valid_and_unique() {
        let a = TraceId::generate();
        let b = TraceId::generate();
        assert!(a.is_valid() && b.is_valid());
        assert_ne!(a, b);
        assert!(!TraceId::invalid().is_valid());
        assert!(!SpanId::invalid().is_valid());
    }

    #[test]
    fn rejects_malformed_hex() {
        assert_eq!(
            TraceId::from_hex("abc"),
            Err(IdError::WrongLength {
                expected: 32,
                got: 3
            })
        );
        assert_eq!(SpanId::from_hex("zzzzzzzzzzzzzzzz"), Err(IdError::NotHex));
    }

    #[test]
    fn serializes_as_hex_string() {
        let id = SpanId::generate();
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, format!("\"{}\"", id.to_hex()));
    }
}
