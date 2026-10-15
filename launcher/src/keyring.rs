// SPDX-License-Identifier: AGPL-3.0-only
//! Remembered passwords, kept in the desktop's keyring (the Secret Service that GNOME
//! Keyring and KWallet provide) under the computer's identifier, never in the library file.
use std::sync::{OnceLock, mpsc};
use std::time::Duration;

use zeroize::Zeroizing;

use crate::backend::Credentials;

/// How long to wait on the keyring, which may first ask the user to unlock it.
const TIMEOUT: Duration = Duration::from_secs(60);
const MAX_PENDING_REQUESTS: usize = 32;
type Request = Box<dyn FnOnce(&Result<tokio::runtime::Runtime, String>) + Send>;
static REQUESTS: OnceLock<Result<mpsc::SyncSender<Request>, String>> = OnceLock::new();

/// Submission order is fixed before any returned future is polled. A late save cannot
/// overtake a subsequent forget, including when a dialog cancels its pending reply.
fn worker() -> Result<&'static mpsc::SyncSender<Request>, String> {
    REQUESTS
        .get_or_init(|| {
            let (sender, receiver) = mpsc::sync_channel::<Request>(MAX_PENDING_REQUESTS);
            std::thread::Builder::new()
                .name("winrdp-keyring".into())
                .spawn(move || {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|error| error.to_string());
                    for request in receiver {
                        request(&runtime);
                    }
                })
                .map(|_| sender)
                .map_err(|error| format!("Could not start the keyring worker: {error}"))
        })
        .as_ref()
        .map_err(Clone::clone)
}

fn attributes(credentials: &Credentials) -> [(&'static str, &str); 4] {
    [
        ("application", "io.winrdp.Next"),
        ("computer", &credentials.profile_id),
        ("address", &credentials.address),
        ("username", &credentials.username),
    ]
}

/// Old releases did not bind items to an endpoint. Keep them discoverable for Forget,
/// but never read their secret or remove another endpoint's newer bound item.
async fn legacy_items(keyring: &oo7::Keyring, credentials: &Credentials) -> oo7::Result<Vec<oo7::Item>> {
    let mut legacy = Vec::new();
    for item in keyring
        .search_items(&[
            ("application", "io.winrdp.Next"),
            ("computer", credentials.profile_id.as_str()),
        ])
        .await?
    {
        let attributes = item.attributes().await?;
        if !attributes.contains_key("address") && !attributes.contains_key("username") {
            legacy.push(item);
        }
    }
    Ok(legacy)
}

async fn clear_legacy(keyring: &oo7::Keyring, credentials: &Credentials) -> oo7::Result<()> {
    for item in legacy_items(keyring, credentials).await? {
        item.delete().await?;
    }
    Ok(())
}

/// Enqueue without waiting; the iced runtime awaits only this request's reply.
fn run<T: Send + 'static>(
    request: impl Future<Output = oo7::Result<T>> + Send + 'static,
) -> impl Future<Output = Result<T, String>> + Send {
    let (reply, received) = tokio::sync::oneshot::channel();
    let submitted = worker().and_then(|sender| {
        sender
            .try_send(Box::new(move |runtime| {
                let result = match runtime {
                    Ok(runtime) => runtime.block_on(async move {
                        tokio::time::timeout(TIMEOUT, request)
                            .await
                            .map_err(|_| "The keyring did not answer.".to_owned())?
                            .map_err(|error| format!("The keyring refused the request: {error}"))
                    }),
                    Err(error) => Err(error.clone()),
                };
                let _ = reply.send(result);
            }))
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => "The keyring is busy. Try again shortly.".to_owned(),
                mpsc::TrySendError::Disconnected(_) => "The keyring worker stopped.".to_owned(),
            })
    });
    async move {
        submitted?;
        received.await.map_err(|_| "The keyring request failed.".to_owned())?
    }
}

/// The password remembered for a computer, if any.
/// Legacy items without an endpoint and account deliberately require a fresh sign-in.
pub fn get(credentials: Credentials) -> impl Future<Output = Result<Option<Zeroizing<String>>, String>> + Send {
    run(async move {
        let keyring = oo7::Keyring::new().await?;
        let Some(item) = keyring
            .search_items(&attributes(&credentials))
            .await?
            .into_iter()
            .next()
        else {
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
pub fn has(credentials: Credentials) -> impl Future<Output = Result<bool, String>> + Send {
    run(async move {
        let keyring = oo7::Keyring::new().await?;
        Ok(!keyring.search_items(&attributes(&credentials)).await?.is_empty()
            || !legacy_items(&keyring, &credentials).await?.is_empty())
    })
}

/// Remembers a password for a computer, replacing any earlier one. `label` is what the
/// keyring's own tools show for it.
pub fn set(
    credentials: Credentials,
    label: String,
    password: Zeroizing<String>,
) -> impl Future<Output = Result<(), String>> + Send {
    let secret = oo7::Secret::text(password.as_str());
    run(async move {
        let keyring = oo7::Keyring::new().await?;
        keyring.unlock().await?;
        clear_legacy(&keyring, &credentials).await?;
        keyring
            .create_item(&label, &attributes(&credentials), secret, true)
            .await
    })
}

/// Forgets a computer's password; nothing to forget is not an error.
pub fn delete(credentials: Credentials) -> impl Future<Output = Result<(), String>> + Send {
    run(async move {
        let keyring = oo7::Keyring::new().await?;
        keyring.delete(&attributes(&credentials)).await?;
        clear_legacy(&keyring, &credentials).await
    })
}

#[cfg(test)]
mod tests {
    static WORKER_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn queued_mutations_keep_their_order_when_a_reply_is_cancelled() {
        let _serial = WORKER_TEST.lock().unwrap();
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let (release, wait) = tokio::sync::oneshot::channel();
        let saving = std::sync::Arc::clone(&events);
        let save = super::run(async move {
            wait.await.unwrap();
            saving.lock().unwrap().push("save");
            Ok(())
        });
        let forgetting = std::sync::Arc::clone(&events);
        let forget = super::run(async move {
            forgetting.lock().unwrap().push("forget");
            Ok(())
        });
        // Neither reply was polled, and the first is dropped altogether. Enqueueing
        // the second still returned while the first worker request was stalled.
        drop(save);
        release.send(()).unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(5), forget)
                .await
                .unwrap()
                .unwrap();
        });
        assert_eq!(*events.lock().unwrap(), ["save", "forget"]);
    }

    #[test]
    fn a_full_keyring_queue_rejects_new_requests_without_waiting() {
        let _serial = WORKER_TEST.lock().unwrap();
        let (started, running) = std::sync::mpsc::channel();
        let (release, wait) = tokio::sync::oneshot::channel();
        let first = super::run(async move {
            started.send(()).unwrap();
            wait.await.unwrap();
            Ok(())
        });
        running.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        let waiting: Vec<_> = (0..super::MAX_PENDING_REQUESTS)
            .map(|_| super::run(async { Ok(()) }))
            .collect();
        let rejected = iced::futures::executor::block_on(super::run(async { Ok(()) }));
        // Release the worker before asserting, so a failure cannot stall another test.
        release.send(()).unwrap();
        iced::futures::executor::block_on(first).unwrap();
        for request in waiting {
            iced::futures::executor::block_on(request).unwrap();
        }
        assert!(rejected.unwrap_err().contains("busy"));
    }

    /// Against a real Secret Service. Use a throwaway D-Bus session AND private temporary
    /// XDG_DATA_HOME, XDG_CONFIG_HOME and XDG_RUNTIME_DIR directories before starting
    /// gnome-keyring-daemon. A separate bus alone does not isolate the user's keyring files.
    /// Then run `cargo test -p winrdp-next -- --ignored keyring_round_trip`.
    #[test]
    #[ignore = "needs a Secret Service; see the comment for a throwaway one"]
    fn keyring_round_trip() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let key = super::Credentials {
                profile_id: uuid::Uuid::new_v4().to_string(),
                address: "192.0.2.10".into(),
                username: "test".into(),
            };
            assert!(!super::has(key.clone()).await.unwrap());
            let keyring = oo7::Keyring::new().await.unwrap();
            let legacy_attributes = [("application", "io.winrdp.Next"), ("computer", key.profile_id.as_str())];
            keyring
                .create_item("Legacy test", &legacy_attributes, oo7::Secret::text("old"), true)
                .await
                .unwrap();
            assert!(
                super::has(key.clone()).await.unwrap(),
                "legacy entries can still be forgotten"
            );
            assert!(
                super::get(key.clone()).await.unwrap().is_none(),
                "legacy entries never auto-fill"
            );
            for password in ["first", "second"] {
                super::set(key.clone(), "Win RDP test".into(), password.to_owned().into())
                    .await
                    .unwrap();
            }
            assert!(super::legacy_items(&keyring, &key).await.unwrap().is_empty());
            assert!(super::has(key.clone()).await.unwrap());
            assert_eq!(
                super::get(key.clone()).await.unwrap().as_deref().map(String::as_str),
                Some("second")
            );
            let mut moved = key.clone();
            moved.address = "192.0.2.99".into();
            assert!(super::get(moved.clone()).await.unwrap().is_none());
            super::set(moved.clone(), "Other endpoint".into(), "other".to_owned().into())
                .await
                .unwrap();
            let mut account = key.clone();
            account.username = "someone-else".into();
            assert!(super::get(account).await.unwrap().is_none());
            keyring
                .create_item("Legacy test", &legacy_attributes, oo7::Secret::text("old"), true)
                .await
                .unwrap();
            super::delete(key.clone()).await.unwrap();
            assert_eq!(super::get(key.clone()).await.unwrap(), None);
            assert!(super::legacy_items(&keyring, &key).await.unwrap().is_empty());
            assert!(
                super::get(moved.clone()).await.unwrap().is_some(),
                "forget preserves another endpoint's secret"
            );
            super::delete(moved).await.unwrap();
            super::delete(key).await.unwrap();
        });
    }
}
