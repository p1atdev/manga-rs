use crate::utils::Bytes;
use anyhow::Result;

pub trait ImageSolver {
    fn solve(&self, bytes: Bytes) -> Result<Bytes>;
}
