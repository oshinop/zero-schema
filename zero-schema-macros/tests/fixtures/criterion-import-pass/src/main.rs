#![deny(warnings)]

use criterion::Criterion;
use zero_schema::zero;

#[zero]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Priority {
    Low = 1,
    Normal = 2,
    High = 3,
}

fn accepts_criterion(_: &mut Criterion) {}

fn main() {
    let _ = accepts_criterion as fn(&mut Criterion);
    let _ = Priority::SCHEMA_SIZE;
    let _ = Priority::SCHEMA_ALIGN;
    let _ = Priority::SCHEMA_STRIDE;
}
