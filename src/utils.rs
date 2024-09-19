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
