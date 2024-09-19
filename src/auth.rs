use base64::{engine::general_purpose::STANDARD, Engine as _};

/// Auth trait
pub trait Auth {
    /// create header value
    fn create_header(&self) -> String;

    /// get header value
    fn get_header_value(&self) -> String;
}

/// Basic auth
#[derive(Debug, Clone)]
pub struct BasicAuth {
    username: String,
    password: String,
}

impl BasicAuth {
    /// create new basic auth
    pub fn new(username: &str, password: &str) -> Self {
        Self {
            username: username.to_string(),
            password: password.to_string(),
        }
    }
}

impl Auth for BasicAuth {
    fn create_header(&self) -> String {
        format!(
            "Basic {}",
            STANDARD.encode(&format!("{}:{}", self.username, self.password))
        )
    }

    fn get_header_value(&self) -> String {
        format!("{}:{}", self.username, self.password)
    }
}

/// Bearer auth
#[derive(Debug, Clone)]
pub struct BearerAuth {
    token: String,
}

impl BearerAuth {
    /// create new bearer auth
    pub fn new(token: &str) -> Self {
        Self {
            token: token.to_string(),
        }
    }
}

impl Auth for BearerAuth {
    fn create_header(&self) -> String {
        format!("Bearer {}", self.token)
    }

    fn get_header_value(&self) -> String {
        self.token.clone()
    }
}

/// Empty auth. do nothing
#[derive(Debug, Clone)]
pub struct EmptyAuth {}

impl Auth for EmptyAuth {
    fn create_header(&self) -> String {
        "".to_string()
    }

    fn get_header_value(&self) -> String {
        "".to_string()
    }
}
