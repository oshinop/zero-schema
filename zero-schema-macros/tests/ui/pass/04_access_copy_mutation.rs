#![deny(warnings)]

use zero_schema_macros::zero;

#[zero(crate = zs)]
pub struct AccessRecord<'a, const N: usize> {
    pub value: u8,
    pub samples: [u8; N],
    pub bytes: &'a [u8; 2],
}

fn main() {
    let mut producer = [1_u8, 2, 3, 4, 5];
    let _: Result<AccessRecordRef<'_, 2>, AccessRecordAccessError<2>> =
        AccessRecord::<'static, 2>::access(&producer);
    let read = AccessRecord::<'static, 2>::access(&producer).expect("reviewed producer bytes");
    let _: zs::ArrayRef<'_, u8, 2, _> = read.samples();
    let _: zs::ArrayRefIter<'_, u8, 2, _> = read.samples().iter();
    let logical = read.copy_into();
    assert_eq!(logical.value, 1);
    assert_eq!(logical.samples, [2, 3]);
    assert_eq!(logical.bytes, &[4, 5]);

    {
        let mut record: AccessRecordMut<'_, 2> = AccessRecord::<'static, 2>::access_mut(&mut producer)
            .expect("reviewed producer bytes");
        record.value_mut().set(6).expect("scalar mutation");
        record.samples_mut().set(1, 7).expect("array mutation");
        record.bytes_mut().set(&[8, 9]).expect("byte mutation");
        let patch: AccessRecordPatch<'static, 2> = AccessRecordPatch {
            value: Some(10),
            samples: Some([11, 12]),
            bytes: Some(&[13, 14]),
        };
        let _: Result<(), AccessRecordMutationError<2>> = record.copy_from(&patch);
    }

    let refreshed = AccessRecord::<'static, 2>::access(&producer).expect("valid mutation");
    assert_eq!(refreshed.copy_into().samples, [11, 12]);

    let storage = zs::schema_buffer!(AccessRecord<'static, 2>);
    assert_eq!(storage.as_bytes().len(), AccessRecord::<'static, 2>::SCHEMA_SIZE);
}
