//! The device token and where it is kept.
//!
//! The token is the only secret of a remote connection. It lives in the
//! Windows Credential Manager together with the address it belongs to, never
//! in `window.json`, and it never prints: `Debug` shows a placeholder.

use std::fmt;

use super::address::RemoteAddress;

/// What the daemon gives a paired device: 256 random bits as base64url, 43
/// characters. The check is looser on the length and strict on the
/// characters, because the token goes into an HTTP header.
const TOKEN_MAX_LEN: usize = 128;

#[derive(Clone, PartialEq, Eq)]
pub struct Token(String);

impl Token {
    pub fn parse(text: &str) -> Option<Self> {
        let valid = !text.is_empty()
            && text.len() <= TOKEN_MAX_LEN
            && text
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
        valid.then(|| Self(text.to_owned()))
    }

    /// The text for the `Authorization` header and the credential store.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token(<redacted>)")
    }
}

/// A token and the address it was issued for. The token is sent to that
/// address and to no other.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Credential {
    pub address: RemoteAddress,
    pub token: Token,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StoreError;

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the credential store failed")
    }
}

pub trait TokenStore {
    /// The saved credential, or `None` when there is none, or when what is
    /// stored cannot be read as one.
    fn load(&self) -> Result<Option<Credential>, StoreError>;
    fn save(&self, credential: &Credential) -> Result<(), StoreError>;
    fn delete(&self) -> Result<(), StoreError>;
}

#[cfg(test)]
pub mod memory {
    use std::cell::RefCell;

    use super::*;

    /// A store in memory, which can be told to fail.
    #[derive(Default)]
    pub struct MemoryStore {
        pub credential: RefCell<Option<Credential>>,
        pub fail_save: std::cell::Cell<bool>,
    }

    impl MemoryStore {
        pub fn with(credential: Credential) -> Self {
            Self {
                credential: RefCell::new(Some(credential)),
                fail_save: Default::default(),
            }
        }
    }

    impl TokenStore for MemoryStore {
        fn load(&self) -> Result<Option<Credential>, StoreError> {
            Ok(self.credential.borrow().clone())
        }

        fn save(&self, credential: &Credential) -> Result<(), StoreError> {
            if self.fail_save.get() {
                return Err(StoreError);
            }
            *self.credential.borrow_mut() = Some(credential.clone());
            Ok(())
        }

        fn delete(&self) -> Result<(), StoreError> {
            *self.credential.borrow_mut() = None;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_is_base64url_text_of_a_sane_length() {
        let real = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQ-_012345";
        assert_eq!(Token::parse(real).unwrap().expose(), real);
        for text in [
            "",
            " ",
            "a b",
            "a\r\nb",
            "abc\0",
            "abc=",
            "abc/def",
            "abc+def",
            "ключ",
            &"a".repeat(TOKEN_MAX_LEN + 1),
        ] {
            assert!(Token::parse(text).is_none(), "{text:?}");
        }
        assert!(Token::parse(&"a".repeat(TOKEN_MAX_LEN)).is_some());
    }

    #[test]
    fn a_token_never_prints() {
        let token = Token::parse("secret-token_value").unwrap();
        assert!(!format!("{token:?}").contains("secret"));
        let credential = Credential {
            address: RemoteAddress::parse("127.0.0.1:1").unwrap(),
            token,
        };
        assert!(!format!("{credential:?}").contains("secret"));
    }
}
