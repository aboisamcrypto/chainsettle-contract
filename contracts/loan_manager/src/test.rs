#![cfg(test)]

extern crate std;

#[path = "fuzz_tests.rs"]
mod fuzz_tests;
#[path = "test_common.rs"]
pub mod test_common;
#[path = "test_lending.rs"]
mod test_lending;
#[path = "test_liquidation.rs"]
mod test_liquidation;
#[path = "test_oracle.rs"]
mod test_oracle;
