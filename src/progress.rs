use std::borrow::Cow;

use anyhow::{anyhow, Result};
use indicatif::{ProgressBar, ProgressStyle};

#[derive(Debug, Clone)]
pub struct ProgressConfig {
    is_enabled: bool,
    template: String,
}

impl ProgressConfig {
    pub fn new(is_enabled: bool, template: String) -> Self {
        ProgressConfig {
            is_enabled,
            template,
        }
    }

    pub fn default() -> Self {
        ProgressConfig {
            is_enabled: true,
            template:
                "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})"
                    .to_string(),
        }
    }

    pub fn disabled() -> Self {
        ProgressConfig {
            is_enabled: false,
            template: "".to_string(),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.is_enabled
    }

    pub fn style(&self) -> Result<ProgressStyle> {
        Ok(ProgressStyle::default_bar().template(&self.template)?)
    }

    pub fn build<T: TryInto<u64>>(&self, length: T) -> Result<ProgressBar> {
        if !self.is_enabled() {
            return Ok(ProgressBar::hidden());
        }
        let pb = ProgressBar::new(
            length
                .try_into()
                .map_err(|_e| anyhow!("Failed to convert length into u64"))?,
        );
        pb.set_style(self.style()?);

        Ok(pb)
    }

    pub fn build_with_message<T: TryInto<u64>>(
        &self,
        length: T,
        message: impl Into<Cow<'static, str>>,
    ) -> Result<ProgressBar> {
        if !self.is_enabled() {
            return Ok(ProgressBar::hidden());
        }
        let pb = ProgressBar::new(
            length
                .try_into()
                .map_err(|_e| anyhow!("Failed to convert length into u64"))?,
        );
        pb.set_style(self.style()?);
        pb.set_message(message);

        Ok(pb)
    }
}
