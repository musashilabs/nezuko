#![allow(dead_code)]

use crate::Result;

#[derive(Debug)]
pub struct Runtime {
    _priv: (),
}

impl Runtime {
    pub fn new() -> Result<Self> {
        Ok(Self { _priv: () })
    }

    pub fn block_on<F: core::future::Future>(&self, _fut: F) -> F::Output {
        todo!("wire up the executor")
    }
}
