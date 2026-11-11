use serde::{Deserialize, Deserializer, de};
use std::fmt;

pub const GRID_SIZE: usize = 4;
pub const TILE_COUNT: usize = GRID_SIZE * GRID_SIZE;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scramble([usize; TILE_COUNT]);

impl Scramble {
    pub fn tile_sources(&self) -> &[usize; TILE_COUNT] {
        &self.0
    }
}

impl TryFrom<[usize; TILE_COUNT]> for Scramble {
    type Error = &'static str;

    fn try_from(sources: [usize; TILE_COUNT]) -> Result<Self, Self::Error> {
        let mut seen = [false; TILE_COUNT];
        for source in sources {
            if source >= TILE_COUNT || seen[source] {
                return Err("Comici scramble must be a permutation of 0 through 15");
            }
            seen[source] = true;
        }
        Ok(Self(sources))
    }
}

impl<'de> Deserialize<'de> for Scramble {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ScrambleVisitor;

        impl<'de> de::Visitor<'de> for ScrambleVisitor {
            type Value = Scramble;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a JSON-encoded permutation of the 16 Comici tiles")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                let sources: Vec<usize> = serde_json::from_str(value).map_err(E::custom)?;
                let sources: [usize; TILE_COUNT] =
                    sources.try_into().map_err(|sources: Vec<_>| {
                        E::custom(format!(
                            "Comici scramble must contain {TILE_COUNT} entries, got {}",
                            sources.len()
                        ))
                    })?;

                Scramble::try_from(sources).map_err(E::custom)
            }
        }

        deserializer.deserialize_str(ScrambleVisitor)
    }
}
