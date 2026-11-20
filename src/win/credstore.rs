//! The device token in the Windows Credential Manager.
//!
//! One generic credential, named [`TARGET`], per user. Its user name field
//! holds the address the token belongs to, and its data is the token. The
//! settings file names the address too; the two are compared on every read.

use windows::Win32::Foundation::ERROR_NOT_FOUND;
use windows::Win32::Security::Credentials::{
    CRED_FLAGS, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree,
    CredReadW, CredWriteW,
};
use windows::core::{PCWSTR, PWSTR};

use super::util::{pcwstr, wide};
use crate::remote::address::RemoteAddress;
use crate::remote::store::{Credential, StoreError, Token, TokenStore};

pub const TARGET: &str = "Ullage Window remote token";

pub struct CredStore;

impl TokenStore for CredStore {
    fn load(&self) -> Result<Option<Credential>, StoreError> {
        load_at(TARGET)
    }

    fn save(&self, credential: &Credential) -> Result<(), StoreError> {
        save_at(TARGET, credential)
    }

    fn delete(&self) -> Result<(), StoreError> {
        delete_at(TARGET)
    }
}

// The functions take the name of the credential, so that the test can use a
// name of its own and never touch the real token.

fn load_at(name: &str) -> Result<Option<Credential>, StoreError> {
    let target = wide(name);
    let mut found: *mut CREDENTIALW = std::ptr::null_mut();
    unsafe {
        if let Err(error) = CredReadW(pcwstr(&target), CRED_TYPE_GENERIC, None, &mut found) {
            return if error.code() == ERROR_NOT_FOUND.to_hresult() {
                Ok(None)
            } else {
                Err(StoreError)
            };
        }
        let credential = read(&*found);
        CredFree(found.cast());
        Ok(credential)
    }
}

fn save_at(name: &str, credential: &Credential) -> Result<(), StoreError> {
    let mut target = wide(name);
    let mut user = wide(&credential.address.to_string());
    let mut blob = credential.token.expose().as_bytes().to_vec();
    let item = CREDENTIALW {
        Flags: CRED_FLAGS(0),
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(target.as_mut_ptr()),
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        UserName: PWSTR(user.as_mut_ptr()),
        ..Default::default()
    };
    let written = unsafe { CredWriteW(&item, 0) };
    // The token must not stay in memory we own.
    blob.fill(0);
    written.map_err(|_| StoreError)
}

fn delete_at(name: &str) -> Result<(), StoreError> {
    let target = wide(name);
    unsafe {
        match CredDeleteW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, None) {
            Ok(()) => Ok(()),
            Err(error) if error.code() == ERROR_NOT_FOUND.to_hresult() => Ok(()),
            Err(_) => Err(StoreError),
        }
    }
}

/// The credential, when what is stored reads as one.
unsafe fn read(item: &CREDENTIALW) -> Option<Credential> {
    if item.UserName.is_null() || item.CredentialBlob.is_null() {
        return None;
    }
    let user = unsafe { item.UserName.to_string() }.ok()?;
    let blob = unsafe {
        std::slice::from_raw_parts(item.CredentialBlob, item.CredentialBlobSize as usize)
    };
    let token = Token::parse(std::str::from_utf8(blob).ok()?)?;
    Some(Credential {
        address: RemoteAddress::parse(&user).ok()?,
        token,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_TARGET: &str = "Ullage Window remote token (test)";

    fn credential(address: &str, token: &str) -> Credential {
        Credential {
            address: RemoteAddress::parse(address).unwrap(),
            token: Token::parse(token).unwrap(),
        }
    }

    #[test]
    fn a_credential_is_written_read_replaced_and_deleted() {
        delete_at(TEST_TARGET).unwrap();
        assert_eq!(load_at(TEST_TARGET), Ok(None));
        // Deleting what is not there is not a failure.
        assert_eq!(delete_at(TEST_TARGET), Ok(()));

        let first = credential("100.71.140.68:7878", "first-token_123");
        save_at(TEST_TARGET, &first).unwrap();
        assert_eq!(load_at(TEST_TARGET), Ok(Some(first)));

        let second = credential("192.168.50.11:7878", "second-token_456");
        save_at(TEST_TARGET, &second).unwrap();
        assert_eq!(load_at(TEST_TARGET), Ok(Some(second)));

        delete_at(TEST_TARGET).unwrap();
        assert_eq!(load_at(TEST_TARGET), Ok(None));
    }
}
