#![allow(dead_code)]

use crate::error;


#[derive(Debug)]
pub struct Runtime {
    _priv: (),
}

impl Runtime {
    pub fn new() -> error::Result<Self> {
        Ok(Self { _priv: () })
    }

    pub fn block_on<F: Future>(&self, _fut: F) -> F::Output {
        todo!("wire up the executor")
    }
}
