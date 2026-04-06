use argon2::{
    Argon2, PasswordHasher, PasswordVerifier,
    password_hash::{Error, PasswordHashString, SaltString, rand_core::OsRng},
};
use std::str::FromStr;

pub fn make_password(raw_password: &str) -> Result<String, Error> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2.hash_password(raw_password.as_bytes(), &salt)?;
    Ok(hash.serialize().to_string())
}

pub fn check_password(raw_password: &str, hash: &str) -> Result<bool, Error> {
    let hash = PasswordHashString::from_str(hash)?;
    match Argon2::default().verify_password(raw_password.as_bytes(), &hash.password_hash()) {
        Ok(_) => Ok(true),
        Err(_) => Ok(false),
    }
}
