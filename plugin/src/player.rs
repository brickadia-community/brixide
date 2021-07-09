use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Debug)]
pub struct Player {
    pub name: String,
    pub uuid: Uuid,
}
