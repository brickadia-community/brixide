use std::collections::HashMap;

use async_trait::async_trait;
use lazy_static::lazy_static;
use log::info;
use plugin::{payloads::*, player::Player, rpc};
use regex::{Captures, Match, Regex};
use uuid::Uuid;

lazy_static! {
    static ref JOIN_REGEX_GROUP: Vec<Regex> = vec![
        Regex::new("^LogServerList: Auth payload valid\\. Result:$").unwrap(),
        Regex::new("^LogServerList: UserName: (?P<user>.+)$").unwrap(),
        Regex::new("^LogServerList: UserId: (?P<id>.+)$").unwrap(),
        Regex::new("^LogServerList: HandleId: (?P<handle>.+)$").unwrap()
    ];
    
    static ref CHAT_REGEX: Regex = Regex::new("LogChat: (?P<user>[^:]+): (?P<message>.*)$").unwrap();
}

/// A matcher for a regex.
#[async_trait]
pub trait RegexMatcher {
    fn regex(&self) -> &'static Regex;
    async fn convert(&self, captures: Captures<'_>) -> rpc::Message;
}

/// A matcher for a group of regexes. Once the first is hit, each successive regex
/// is expected in sequence, using the index to chain matches.
#[async_trait]
pub trait GroupedRegexMatcher: Sized {
    fn regexes(&self) -> &'static Vec<Regex>;
    async fn convert(&self, instance: &GroupedRegexMatches<'_, Self>) -> rpc::Message;
}

/// An instance of an in-progress grouped regex match.
/// On completion, it is sent to its `GroupedRegexMatcher` for `convert`ing.
pub struct GroupedRegexMatches<'a, T: 'a + GroupedRegexMatcher> {
    pub index: i32,
    pub matcher: &'a T,
    pub captures: Vec<HashMap<String, String>>
}

impl<'a, T: 'a + GroupedRegexMatcher> GroupedRegexMatches<'a, T> {
    pub fn group_at(&self, capture_ind: usize, key: &str) -> &str {
        self.captures[capture_ind][key.into()].as_str()
    }
}

/// Player join regex.
pub struct JoinRegexGroupMatcher;

#[async_trait]
impl GroupedRegexMatcher for JoinRegexGroupMatcher {
    fn regexes(&self) -> &'static Vec<Regex> {
        &JOIN_REGEX_GROUP
    }

    async fn convert(&self, instance: &GroupedRegexMatches<'_, Self>) -> rpc::Message {
        let name = instance.group_at(1, "user");
        let uuid: Uuid = instance.group_at(2, "id").parse().unwrap();
        let player = Player { name: name.into(), uuid };
        rpc::Message::notification("join", Some(serde_json::to_value(player).unwrap()))
    }
}

/// Chat matcher regex.
pub struct ChatRegexMatcher;

#[async_trait]
impl RegexMatcher for ChatRegexMatcher {
    fn regex(&self) -> &'static Regex {
        &CHAT_REGEX
    }

    async fn convert(&self, captures: Captures<'_>) -> rpc::Message {
        let (user, message) = (&captures["user"], &captures["message"]);
        info!("{}: {}", user, message);
        ChatPayload { user: user.into(), message: message.into() }.into()
    }
}
