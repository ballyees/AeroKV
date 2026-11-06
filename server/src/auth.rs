use subtle::ConstantTimeEq;

#[derive(Clone, Debug)]
pub struct Authenticator {
    valid_tokens: Vec<Vec<u8>>,
    enabled: bool,
}

impl Authenticator {
    /// Creates an authenticator supporting single or comma-separated tokens
    /// e.g. "mysecret123,mysecret1234,mysecret12345"
    pub fn new(tokens_str: &str) -> Self {
        let tokens: Vec<Vec<u8>> = tokens_str
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.as_bytes().to_vec())
            .collect();

        Self {
            enabled: !tokens.is_empty(),
            valid_tokens: tokens,
        }
    }

    #[inline]
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    #[inline]
    pub fn verify(&self, supplied_token: &[u8]) -> bool {
        if !self.enabled {
            return true;
        }

        let mut matched = 0u8;
        for expected in &self.valid_tokens {
            if supplied_token.len() == expected.len() {
                matched |= supplied_token.ct_eq(expected).unwrap_u8();
            }
        }

        matched == 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_verification() {
        let auth = Authenticator::new("secret123");
        assert!(auth.verify(b"secret123"));
        assert!(!auth.verify(b"secret124"));
        assert!(!auth.verify(b"short"));
        assert!(!auth.verify(b"muchlongersecret1234567"));

        let disabled_auth = Authenticator::new("");
        assert!(disabled_auth.verify(b"any_token"));
        assert!(disabled_auth.verify(b""));
    }

    #[test]
    fn test_multiple_comma_separated_tokens() {
        let auth = Authenticator::new("tok1, tok2,tok3");
        assert!(auth.is_enabled());
        assert!(auth.verify(b"tok1"));
        assert!(auth.verify(b"tok2"));
        assert!(auth.verify(b"tok3"));
        assert!(!auth.verify(b"tok4"));
        assert!(!auth.verify(b"tok"));
    }
}

