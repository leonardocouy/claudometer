use std::sync::Arc;
use tokio::sync::Mutex;

const KEYRING_SERVICE: &str = "com.softaworks.claudometer";
pub const KEYRING_USER_CLAUDE_SESSION_KEY: &str = "claude_session_key";

#[derive(Clone)]
pub struct SecretManager {
    user: &'static str,
    in_memory: Arc<Mutex<Option<String>>>,
}

impl SecretManager {
    pub fn new(user: &'static str) -> Self {
        Self {
            user,
            in_memory: Arc::new(Mutex::new(None)),
        }
    }

    fn entry(&self) -> Result<keyring::Entry, keyring::Error> {
        keyring::Entry::new(KEYRING_SERVICE, self.user)
    }

    pub fn is_available(&self) -> bool {
        let Ok(entry) = self.entry() else {
            return false;
        };

        match entry.get_password() {
            Ok(_) => true,
            Err(keyring::Error::NoEntry) => true,
            Err(keyring::Error::BadEncoding(_)) => true,
            Err(keyring::Error::Ambiguous(_)) => true,
            Err(keyring::Error::NoStorageAccess(_)) => false,
            Err(keyring::Error::PlatformFailure(_)) => false,
            Err(_) => false,
        }
    }

    pub async fn set_in_memory(&self, value: Option<String>) {
        let mut guard = self.in_memory.lock().await;
        *guard = value.and_then(|v| {
            let trimmed = v.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        });
    }

    pub async fn get_current(&self, remember: bool) -> Result<Option<String>, ()> {
        if let Some(value) = self.in_memory.lock().await.clone() {
            return Ok(Some(value));
        }

        if !remember {
            return Ok(None);
        }

        let entry = self.entry().map_err(|_| ())?;

        match entry.get_password() {
            Ok(pwd) => {
                let trimmed = pwd.trim().to_string();
                if trimmed.is_empty() {
                    Ok(None)
                } else {
                    self.set_in_memory(Some(trimmed.clone())).await;
                    Ok(Some(trimmed))
                }
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(keyring::Error::NoStorageAccess(_)) => Err(()),
            Err(keyring::Error::PlatformFailure(_)) => Err(()),
            Err(_) => Ok(None),
        }
    }

    pub async fn remember(&self, value: &str) -> Result<(), ()> {
        let entry = self.entry().map_err(|_| ())?;
        entry.set_password(value).map_err(|_| ())?;
        Ok(())
    }

    pub async fn delete_persisted(&self) -> Result<(), ()> {
        if let Ok(entry) = self.entry() {
            let _ = entry.delete_credential();
        };
        Ok(())
    }

    pub async fn forget_all(&self) -> Result<(), ()> {
        let _ = self.delete_persisted().await;
        self.set_in_memory(None).await;
        Ok(())
    }
}
