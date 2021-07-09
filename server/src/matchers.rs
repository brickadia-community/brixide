use std::{collections::HashMap, fmt, sync::Arc, time::Duration};

use async_trait::async_trait;
use lazy_static::lazy_static;
use log::info;
use plugin::{payloads::*, player::Player, rpc};
use regex::{Captures, Match, Regex};
use serde::{Deserialize, Serialize};
use tokio::{sync::{mpsc, oneshot}, time::Instant};
use uuid::Uuid;

lazy_static! {
    static ref CHAT_REGEX: Vec<Regex> = vec![Regex::new("LogChat: (?P<user>[^:]+): (?P<message>.*)$").unwrap()];

    static ref JOIN_REGEX: Vec<Regex> = vec![
        Regex::new("^LogServerList: Auth payload valid\\. Result:$").unwrap(),
        Regex::new("^LogServerList: UserName: (?P<user>.+)$").unwrap(),
        Regex::new("^LogServerList: UserId: (?P<id>.+)$").unwrap(),
        Regex::new("^LogServerList: HandleId: (?P<handle>.+)$").unwrap()
    ];
}

/// A wrapper around the captures of a regex.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct RegexCaptures {
    vec: Vec<HashMap<String, String>>
}

impl RegexCaptures {
    pub fn new(vec: Vec<HashMap<String, String>>) -> Self {
        RegexCaptures { vec }
    }

    pub fn at(&self, ind: usize, key: &str) -> Option<&str> {
        match self.vec.get(ind) {
            Some(map) => map.get(key.into()).map(String::as_str),
            None => None
        }
    }

    pub fn push(&mut self, map: HashMap<String, String>) {
        self.vec.push(map);
    }

    pub fn len(&self) -> usize {
        self.vec.len()
    }
}

/// A matcher for a group of regexes. Once the first is hit, each successive regex
/// is expected in sequence, using the index to chain matches.
#[async_trait]
pub trait GroupedRegexMatcher: Sync {
    fn regexes(&self) -> &Vec<Regex>;
    async fn complete(&self, instance: &GroupedRegexMatches<'_>);
}

/// An instance of an in-progress grouped regex match.
/// On completion, it is sent to its `GroupedRegexMatcher` for `convert`ing.
pub struct GroupedRegexMatches<'a> {
    pub index: Option<i32>,
    pub matcher: Arc<dyn 'a + GroupedRegexMatcher + Send>,
    pub captures: RegexCaptures,
    pub last: Instant,
    pub timeout: Duration
}

impl fmt::Debug for GroupedRegexMatches<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GroupedRegexMatches")
            .field("index", &self.index)
            .field("captures", &self.captures)
            .field("last", &self.last)
            .field("timeout", &self.timeout)
            .finish()
    }
}

/// Runtime plugin regex.
pub struct PluginRegexMatcher {
    pub regexes: Vec<Regex>,
    pub capture_sender: mpsc::Sender<RegexCaptures>
}

#[async_trait]
impl GroupedRegexMatcher for PluginRegexMatcher {
    fn regexes(&self) -> &Vec<Regex> {
        &self.regexes
    }

    async fn complete(&self, instance: &GroupedRegexMatches<'_>) {
        let captures = instance.captures.clone();
        self.capture_sender.send(captures).await.unwrap();
    }
}

/// Player join regex.
pub struct ConnectRegexMatcher(pub mpsc::UnboundedSender<rpc::Message>);

#[async_trait]
impl GroupedRegexMatcher for ConnectRegexMatcher {
    fn regexes(&self) -> &'static Vec<Regex> {
        &JOIN_REGEX
    }

    async fn complete(&self, instance: &GroupedRegexMatches<'_>) {
        let name = instance.captures.at(1, "user").unwrap();
        let uuid: Uuid = instance.captures.at(2, "id").unwrap().parse().unwrap();
        let player = Player { name: name.into(), uuid };
        let message = rpc::Message::notification("connect", Some(serde_json::to_value(player).unwrap()));
        self.0.send(message).unwrap();
    }
}

/// Chat matcher regex.
pub struct ChatRegexMatcher(pub mpsc::UnboundedSender<rpc::Message>);

#[async_trait]
impl GroupedRegexMatcher for ChatRegexMatcher {
    fn regexes(&self) -> &'static Vec<Regex> {
        &CHAT_REGEX
    }

    async fn complete(&self, instance: &GroupedRegexMatches<'_>) {
        let (user, message) = (instance.captures.at(0, "user").unwrap(), instance.captures.at(0, "message").unwrap());
        info!("{}: {}", user, message);
        let message = ChatPayload { user: user.into(), message: message.into() }.into();
        self.0.send(message).unwrap();
    }
}
