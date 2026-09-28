// SPDX-License-Identifier: AGPL-3.0-only
//! Remembered passwords, kept in the desktop's keyring (the Secret Service that GNOME
//! Keyring and KWallet provide) under the computer's identifier, never in the library file.
use std::time::Duration;

use zeroize::Zeroizing;

/// How long to wait on the keyring, which may first ask the user to unlock it.
const TIMEOUT: Duration = Duration::from_secs(60);

fn attributes(profile_id: &str) -> [(&'static str, &str); 2] {
    [("application", "io.winrdp.Next"), ("computer", profile_id)]
}

/// Runs one keyring request on a thread of its own: the launcher asks from inside the iced
/// runtime, where blocking on another tokio runtime is not allowed.
fn run<T: Send + 'static>(request: impl Future<Output = oo7::Result<T>> + Send + 'static) -> Result<T, String> {
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        runtime
            .block_on(async move { tokio::time::timeout(TIMEOUT, request).await })
            .map_err(|_| "The keyring did not answer.".to_owned())?
            .map_err(|e| format!("The keyring refused the request: {e}"))
    })
    .join()
    .map_err(|_| "The keyring request failed.".to_owned())?
}

/// The password remembered for a computer, if any.
pub fn get(profile_id: &str) -> Result<Option<Zeroizing<String>>, String> {
    let profile_id = profile_id.to_owned();
    run(async move {
        let keyring = oo7::Keyring::new().await?;
        let Some(item) = keyring.search_items(&attributes(&profile_id)).await?.into_iter().next() else {
            return Ok(None);
        };
        item.unlock().await?;
        let secret = item.secret().await?;
        Ok(Some(Zeroizing::new(
            String::from_utf8_lossy(secret.as_bytes()).into_owned(),
        )))
    })
}

/// Whether a password is remembered for a computer, without reading it.
pub fn has(profile_id: &str) -> Result<bool, String> {
    let profile_id = profile_id.to_owned();
    run(async move {
        let keyring = oo7::Keyring::new().await?;
        Ok(!keyring.search_items(&attributes(&profile_id)).await?.is_empty())
    })
}

/// Remembers a password for a computer, replacing any earlier one. `label` is what the
/// keyring's own tools show for it.
pub fn set(profile_id: &str, label: &str, password: &str) -> Result<(), String> {
    let (profile_id, label) = (profile_id.to_owned(), label.to_owned());
    let secret = oo7::Secret::text(password);
    run(async move {
        let keyring = oo7::Keyring::new().await?;
        keyring.unlock().await?;
        keyring
            .create_item(&label, &attributes(&profile_id), secret, true)
            .await
    })
}

/// Forgets a computer's password; nothing to forget is not an error.
pub fn delete(profile_id: &str) -> Result<(), String> {
    let profile_id = profile_id.to_owned();
    run(async move {
        let keyring = oo7::Keyring::new().await?;
        keyring.delete(&attributes(&profile_id)).await
    })
}

#[cfg(test)]
mod tests {
    /// Against a real Secret Service. Run it inside a throwaway session so it cannot touch
    /// your own keyring: `dbus-run-session -- sh -c 'echo test | gnome-keyring-daemon
    /// --unlock --components=secrets >/dev/null; cargo test -p winrdp-next -- --ignored keyring'`.
    #[test]
    #[ignore = "needs a Secret Service; see the comment for a throwaway one"]
    fn keyring_round_trip() {
        let id = uuid::Uuid::new_v4().to_string();
        assert!(!super::has(&id).unwrap());
        super::set(&id, "Win RDP test", "first").unwrap();
        super::set(&id, "Win RDP test", "second").unwrap();
        assert!(super::has(&id).unwrap());
        assert_eq!(super::get(&id).unwrap().as_deref().map(String::as_str), Some("second"));
        super::delete(&id).unwrap();
        assert_eq!(super::get(&id).unwrap(), None);
        super::delete(&id).unwrap();
    }
}
