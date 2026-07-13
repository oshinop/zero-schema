use core::fmt;

pub type KeyValue = (u64, u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Status {
    Ok = 0,
    NullWritten = 1,
    UnknownId = 2,
    InvalidInputLength = 3,
    NullInput = 4,
    NullOutput = 5,
    InsufficientCapacity = 6,
    CountOverflowOrInternal = 7,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FfiError {
    Status(Status),
    UnknownStatus(u8),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HarnessError {
    Ffi(FfiError),
    ReportSlotOverflow {
        pair_count: usize,
    },
    WrittenExceedsCapacity {
        written: usize,
        capacity: usize,
    },
    InvalidReportLength {
        expected: usize,
        actual: usize,
    },
    InvalidReportVersion(u64),
    ReportCaseIdOutOfRange(u64),
    UnexpectedReportCaseId {
        expected: u32,
        actual: u32,
    },
    ReportPairCountOutOfRange(u64),
    UnexpectedReportPairCount {
        expected: usize,
        actual: usize,
    },
    ZeroReportKey {
        index: usize,
    },
    DuplicateReportKey {
        key: u64,
    },
    UnexpectedReportKey {
        index: usize,
        expected: u64,
        actual: u64,
    },
    ValueOutOfRange {
        what: &'static str,
        value: usize,
    },
    InvalidByteLength {
        expected: usize,
        actual: usize,
    },
    InvalidData(&'static str),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Report {
    pairs: Vec<KeyValue>,
}

impl Report {
    pub fn parse(
        expected_case_id: u32,
        expected_keys: &[u64],
        slots: &[u64],
        written: usize,
    ) -> Result<Self, HarnessError> {
        if written > slots.len() {
            return Err(HarnessError::WrittenExceedsCapacity {
                written,
                capacity: slots.len(),
            });
        }
        let expected_len = checked_report_slots(expected_keys.len())?;
        if written != expected_len {
            return Err(HarnessError::InvalidReportLength {
                expected: expected_len,
                actual: written,
            });
        }
        let frame = &slots[..written];
        if frame[0] != 1 {
            return Err(HarnessError::InvalidReportVersion(frame[0]));
        }
        let actual_case_id =
            u32::try_from(frame[1]).map_err(|_| HarnessError::ReportCaseIdOutOfRange(frame[1]))?;
        if actual_case_id != expected_case_id {
            return Err(HarnessError::UnexpectedReportCaseId {
                expected: expected_case_id,
                actual: actual_case_id,
            });
        }
        let actual_count = usize::try_from(frame[2])
            .map_err(|_| HarnessError::ReportPairCountOutOfRange(frame[2]))?;
        if actual_count != expected_keys.len() {
            return Err(HarnessError::UnexpectedReportPairCount {
                expected: expected_keys.len(),
                actual: actual_count,
            });
        }
        // Rechecking the count-derived frame size makes the parser independent of
        // the expected-key length check above and rejects trailing slots.
        let actual_len = checked_report_slots(actual_count)?;
        if actual_len != written {
            return Err(HarnessError::InvalidReportLength {
                expected: actual_len,
                actual: written,
            });
        }

        let mut pairs = Vec::with_capacity(actual_count);
        for (index, (&expected, pair)) in expected_keys
            .iter()
            .zip(frame[3..].chunks_exact(2))
            .enumerate()
        {
            let key = pair[0];
            if key == 0 {
                return Err(HarnessError::ZeroReportKey { index });
            }
            if pairs.iter().any(|&(seen, _)| seen == key) {
                return Err(HarnessError::DuplicateReportKey { key });
            }
            if key != expected {
                return Err(HarnessError::UnexpectedReportKey {
                    index,
                    expected,
                    actual: key,
                });
            }
            pairs.push((key, pair[1]));
        }
        Ok(Self { pairs })
    }

    pub fn pairs(&self) -> &[KeyValue] {
        &self.pairs
    }

    pub fn into_pairs(self) -> Vec<KeyValue> {
        self.pairs
    }
}

pub fn checked_report_slots(pair_count: usize) -> Result<usize, HarnessError> {
    pair_count
        .checked_mul(2)
        .and_then(|slots| slots.checked_add(3))
        .ok_or(HarnessError::ReportSlotOverflow { pair_count })
}

impl TryFrom<u8> for Status {
    type Error = FfiError;

    fn try_from(raw: u8) -> Result<Self, Self::Error> {
        match raw {
            0 => Ok(Self::Ok),
            1 => Ok(Self::NullWritten),
            2 => Ok(Self::UnknownId),
            3 => Ok(Self::InvalidInputLength),
            4 => Ok(Self::NullInput),
            5 => Ok(Self::NullOutput),
            6 => Ok(Self::InsufficientCapacity),
            7 => Ok(Self::CountOverflowOrInternal),
            other => Err(FfiError::UnknownStatus(other)),
        }
    }
}

impl From<Status> for FfiError {
    fn from(status: Status) -> Self {
        Self::Status(status)
    }
}

impl From<FfiError> for HarnessError {
    fn from(error: FfiError) -> Self {
        Self::Ffi(error)
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Ok => "success",
            Self::NullWritten => "null or misaligned written pointer",
            Self::UnknownId => "unknown case ID",
            Self::InvalidInputLength => "invalid input length",
            Self::NullInput => "null input pointer",
            Self::NullOutput => "null output pointer",
            Self::InsufficientCapacity => "insufficient output capacity",
            Self::CountOverflowOrInternal => "count overflow or internal error",
        })
    }
}

impl fmt::Display for FfiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Status(status) => write!(
                f,
                "conformance FFI returned status {} ({status})",
                *status as u8
            ),
            Self::UnknownStatus(raw) => write!(f, "conformance FFI returned unknown status {raw}"),
        }
    }
}

impl fmt::Display for HarnessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ffi(error) => error.fmt(f),
            Self::ReportSlotOverflow { pair_count } => {
                write!(f, "report slot count overflows for {pair_count} pairs")
            }
            Self::WrittenExceedsCapacity { written, capacity } => write!(
                f,
                "FFI reported {written} slots written into capacity {capacity}"
            ),
            Self::InvalidReportLength { expected, actual } => write!(
                f,
                "invalid report length: expected {expected} slots, got {actual}"
            ),
            Self::InvalidReportVersion(version) => {
                write!(f, "unsupported report version {version}")
            }
            Self::ReportCaseIdOutOfRange(value) => {
                write!(f, "report case ID {value} does not fit u32")
            }
            Self::UnexpectedReportCaseId { expected, actual } => write!(
                f,
                "unexpected report case ID: expected {expected}, got {actual}"
            ),
            Self::ReportPairCountOutOfRange(value) => {
                write!(f, "report pair count {value} does not fit usize")
            }
            Self::UnexpectedReportPairCount { expected, actual } => write!(
                f,
                "unexpected report pair count: expected {expected}, got {actual}"
            ),
            Self::ZeroReportKey { index } => write!(f, "report key at pair {index} is zero"),
            Self::DuplicateReportKey { key } => write!(f, "duplicate report key {key}"),
            Self::UnexpectedReportKey {
                index,
                expected,
                actual,
            } => write!(
                f,
                "unexpected report key at pair {index}: expected {expected}, got {actual}"
            ),
            Self::ValueOutOfRange { what, value } => write!(
                f,
                "{what} value {value} does not fit the report representation"
            ),
            Self::InvalidByteLength { expected, actual } => {
                write!(f, "invalid byte length: expected {expected}, got {actual}")
            }
            Self::InvalidData(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for FfiError {}
impl std::error::Error for HarnessError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Ffi(error) => Some(error),
            _ => None,
        }
    }
}
