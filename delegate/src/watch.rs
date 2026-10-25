//! Placeholder owned by the watch task (t011).
use crate::task::Ctx;
use anyhow::{Result, bail};

pub fn run(_ctx: &Ctx, _id: Option<&str>, _interval: f64) -> Result<()> {
    bail!("not built yet")
}
