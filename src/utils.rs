const BOT_USER_AGENT: &str = "manga.rs/1.0";

pub enum UserAgent {
    Bot,
}

impl UserAgent {
    pub fn value(&self) -> String {
        match self {
            UserAgent::Bot => BOT_USER_AGENT,
        }
        .to_string()
    }
}

/// Include generated proto files
macro_rules! include_proto {
    ($name:literal) => {
        include!(concat!(env!("OUT_DIR"), "/", $name, ".rs"));
    };
}
pub(crate) use include_proto;

pub(crate) type Bytes = Vec<u8>;
