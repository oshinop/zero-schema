use crate::{HarnessError, Report};

#[test]
fn malformed_reports_are_rejected() {
    let keys = [11, 12];
    assert!(Report::parse(7, &keys, &[1, 7, 2, 11, 1, 12, 2], 7).is_ok());
    assert_eq!(
        Report::parse(7, &keys, &[1, 7, 2, 11, 1, 11, 2], 7),
        Err(HarnessError::DuplicateReportKey { key: 11 })
    );
    assert_eq!(
        Report::parse(7, &keys, &[1, 7, 2, 12, 1, 11, 2], 7),
        Err(HarnessError::UnexpectedReportKey {
            index: 0,
            expected: 11,
            actual: 12
        })
    );
    assert_eq!(
        Report::parse(7, &keys, &[1, 7, 2, 11, 1, 13, 2], 7),
        Err(HarnessError::UnexpectedReportKey {
            index: 1,
            expected: 12,
            actual: 13
        })
    );
    assert_eq!(
        Report::parse(7, &keys, &[1, 7, 1, 11, 1], 5),
        Err(HarnessError::InvalidReportLength {
            expected: 7,
            actual: 5
        })
    );
    assert_eq!(
        Report::parse(7, &keys, &[1, 7, 2, 11, 1, 12, 2, 99], 8),
        Err(HarnessError::InvalidReportLength {
            expected: 7,
            actual: 8
        })
    );
    assert_eq!(
        Report::parse(7, &keys, &[1, 7, 3, 11, 1, 12, 2], 7),
        Err(HarnessError::UnexpectedReportPairCount {
            expected: 2,
            actual: 3
        })
    );
}
