extern crate bcrypt;

use bcrypt::{DEFAULT_COST, hash, verify};

pub fn hash_password(password: &str) -> String {
    hash(password, DEFAULT_COST).expect("Failed to hash password")
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    verify(password, hash).expect("Failed to verify password")
}
