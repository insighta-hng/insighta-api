use mongodb::{Collection, Database};

use crate::models::users::User;

#[derive(Clone, Debug)]
pub struct UserRepo {
    collection: Collection<User>,
}

impl UserRepo {
    pub fn new(db: &Database) -> Self {
        Self {
            collection: db.collection("users"),
        }
    }
}
