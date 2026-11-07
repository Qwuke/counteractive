//! Conversations: which session a client's turns use (docs 04 section 3).
//!
//! An OpenAI request names no conversation, so walletd accepts an optional
//! `X-Conversation-Id`. Turns with the same id share one session and one
//! dial scope; a request without it is its own conversation, closed after
//! its turn, which is the unlinkable default.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use crate::error::ApiError;
use crate::session::{LiveSession, Reached};
use crate::state::{Context, Walletd};

/// Sessions idle this long are closed by walletd (SPEC section 7). Tests
/// pass [`close_idle`] a shorter time.
pub const IDLE_CLOSE: Duration = Duration::from_secs(5 * 60);

/// How often the idle check runs at most; a shorter idle time is checked
/// four times per period.
const IDLE_CHECK: Duration = Duration::from_secs(15);

/// The longest conversation id accepted.
const MAX_ID_LEN: usize = 128;

/// One conversation's session, if it has one.
pub struct Conversation {
    /// The live session its turns use.
    pub session: Option<LiveSession>,
    /// When its last turn ended.
    pub last_used: Instant,
}

impl Conversation {
    /// A conversation with no session yet.
    pub fn new() -> Self {
        Self { session: None, last_used: Instant::now() }
    }
}

/// A conversation shared between requests. Its lock is held for a whole
/// turn, so a conversation runs one turn at a time.
pub type SharedConversation = Arc<tokio::sync::Mutex<Conversation>>;

/// The named conversations.
#[derive(Default)]
pub struct Conversations {
    map: Mutex<HashMap<String, SharedConversation>>,
}

impl Conversations {
    /// The conversation with this id, created if new.
    pub fn get_or_create(&self, id: &str) -> SharedConversation {
        Arc::clone(
            self.lock().entry(id.to_owned()).or_insert_with(|| Arc::new(tokio::sync::Mutex::new(Conversation::new()))),
        )
    }

    /// The conversation with this id, if it exists.
    pub fn get(&self, id: &str) -> Option<SharedConversation> {
        self.lock().get(id).cloned()
    }

    /// Every conversation, with its id.
    pub fn all(&self) -> Vec<(String, SharedConversation)> {
        self.lock().iter().map(|(id, conversation)| (id.clone(), Arc::clone(conversation))).collect()
    }

    /// Forgets a conversation.
    pub fn remove(&self, id: &str) {
        self.lock().remove(id);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, SharedConversation>> {
        self.map.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Whether a client's conversation id is acceptable: short printable ASCII.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= MAX_ID_LEN && id.bytes().all(|byte| byte.is_ascii_graphic())
}

impl Walletd {
    /// Closes a conversation's session, if it has one.
    pub async fn close_conversation(
        &self,
        context: &Context,
        conversation: &mut Conversation,
    ) -> Result<Option<Reached>, ApiError> {
        match conversation.session.take() {
            Some(session) => self.close_session(context, session).await.map(Some),
            None => Ok(None),
        }
    }
}

/// Closes sessions idle for `idle_close` (walletd runs it with
/// [`IDLE_CLOSE`]) and forgets their conversations. A conversation in the
/// middle of a turn is busy, not idle, and skipped.
pub async fn close_idle(walletd: Arc<Walletd>, idle_close: Duration) {
    let mut tick = tokio::time::interval(IDLE_CHECK.min(idle_close / 4).max(Duration::from_millis(1)));
    loop {
        tick.tick().await;
        for (id, conversation) in walletd.conversations.all() {
            let Ok(mut conversation) = conversation.try_lock() else { continue };
            if conversation.last_used.elapsed() < idle_close {
                continue;
            }
            if conversation.session.is_some() {
                let closed = match walletd.context() {
                    Ok(context) => walletd.close_conversation(&context, &mut conversation).await.map(|_| ()),
                    Err(error) => Err(error.into()),
                };
                if let Err(error) = closed {
                    // The chain stays in its journalled state for reconcile.
                    tracing::warn!(code = error.code(), "closing an idle session failed");
                }
            }
            walletd.conversations.remove(&id);
        }
    }
}
