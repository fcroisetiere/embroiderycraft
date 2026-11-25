//! People and programs using the server: users with passwords (the web page keeps a session
//! cookie) and API tokens (`Authorization: Bearer …`) for other programs.

use crate::db;
use crate::library::{hex, sha256};
use argon2::Argon2;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

pub const COOKIE: &str = "ec_session";
/// Sessions last 30 days.
const SESSION_SECS: i64 = 30 * 24 * 3600;

#[derive(Clone, Debug, Serialize)]
pub struct User {
    pub id: i64,
    pub name: String,
    pub admin: bool,
}

/// A new random secret (session or token), 64 hex digits.
pub fn secret() -> String {
    let mut b = Vec::with_capacity(32);
    b.extend_from_slice(&argon2::password_hash::generate_salt());
    b.extend_from_slice(&argon2::password_hash::generate_salt());
    hex(&b)
}

pub fn hash_password(pass: &str) -> Result<String, String> {
    let h: PasswordHash = Argon2::default()
        .hash_password(pass.as_bytes())
        .map_err(|e| e.to_string())?;
    Ok(h.to_string())
}

fn check_password(pass: &str, stored: &str) -> bool {
    PasswordHash::new(stored)
        .map(|h| {
            Argon2::default()
                .verify_password(pass.as_bytes(), &h)
                .is_ok()
        })
        .unwrap_or(false)
}

pub fn user_count(db: &Connection) -> rusqlite::Result<i64> {
    db.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))
}

pub fn check_new_password(pass: &str) -> Result<(), String> {
    if pass.chars().count() < 8 {
        return Err("The password needs at least 8 characters.".into());
    }
    Ok(())
}

pub fn add_user(db: &Connection, name: &str, pass: &str, admin: bool) -> Result<i64, String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 60 {
        return Err("Give a user name (at most 60 characters).".into());
    }
    check_new_password(pass)?;
    let hash = hash_password(pass)?;
    db.execute(
        "INSERT INTO users (name, pass, admin, created) VALUES (?, ?, ?, ?)",
        params![name, hash, admin, db::now()],
    )
    .map_err(|e| match e {
        rusqlite::Error::SqliteFailure(f, _)
            if f.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            format!("There is already a user named \"{name}\".")
        }
        e => e.to_string(),
    })?;
    Ok(db.last_insert_rowid())
}

pub fn set_password(db: &Connection, name: &str, pass: &str) -> Result<(), String> {
    check_new_password(pass)?;
    let hash = hash_password(pass)?;
    let n = db
        .execute(
            "UPDATE users SET pass = ? WHERE name = ?",
            params![hash, name],
        )
        .map_err(|e| e.to_string())?;
    if n == 0 {
        return Err(format!("No user named \"{name}\"."));
    }
    // signed out everywhere
    db.execute(
        "DELETE FROM sessions WHERE user_id = (SELECT id FROM users WHERE name = ?)",
        [name],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Check a name and password; on success a new session secret (for the cookie).
pub fn login(db: &Connection, name: &str, pass: &str) -> rusqlite::Result<Option<String>> {
    let row: Option<(i64, String)> = db
        .query_row(
            "SELECT id, pass FROM users WHERE name = ?",
            [name.trim()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((id, stored)) = row else {
        // same work as a real check, so a wrong name takes as long as a wrong password
        let _ = hash_password(pass);
        return Ok(None);
    };
    if !check_password(pass, &stored) {
        return Ok(None);
    }
    let s = secret();
    let now = db::now();
    db.execute("DELETE FROM sessions WHERE expires < ?", [now])?;
    db.execute(
        "INSERT INTO sessions (hash, user_id, expires) VALUES (?, ?, ?)",
        params![sha256(s.as_bytes()), id, now + SESSION_SECS],
    )?;
    Ok(Some(s))
}

pub fn logout(db: &Connection, session: &str) -> rusqlite::Result<()> {
    db.execute(
        "DELETE FROM sessions WHERE hash = ?",
        [sha256(session.as_bytes())],
    )?;
    Ok(())
}

fn user_row(r: &rusqlite::Row) -> rusqlite::Result<User> {
    Ok(User {
        id: r.get(0)?,
        name: r.get(1)?,
        admin: r.get(2)?,
    })
}

pub fn by_session(db: &Connection, session: &str) -> rusqlite::Result<Option<User>> {
    db.query_row(
        "SELECT u.id, u.name, u.admin FROM sessions s JOIN users u ON u.id = s.user_id \
         WHERE s.hash = ? AND s.expires > ?",
        params![sha256(session.as_bytes()), db::now()],
        user_row,
    )
    .optional()
}

pub fn by_token(db: &Connection, token: &str) -> rusqlite::Result<Option<User>> {
    let hash = sha256(token.as_bytes());
    let u = db
        .query_row(
            "SELECT u.id, u.name, u.admin FROM tokens t JOIN users u ON u.id = t.user_id WHERE t.hash = ?",
            [&hash],
            user_row,
        )
        .optional()?;
    if u.is_some() {
        db.execute(
            "UPDATE tokens SET last_used = ? WHERE hash = ?",
            params![db::now(), hash],
        )?;
    }
    Ok(u)
}

#[derive(Debug, Serialize)]
pub struct TokenInfo {
    pub id: i64,
    pub name: String,
    pub user: String,
    pub created: i64,
    pub last_used: Option<i64>,
}

pub fn tokens(db: &Connection) -> rusqlite::Result<Vec<TokenInfo>> {
    let mut q = db.prepare(
        "SELECT t.id, t.name, u.name, t.created, t.last_used FROM tokens t JOIN users u ON u.id = t.user_id ORDER BY t.id",
    )?;
    q.query_map([], |r| {
        Ok(TokenInfo {
            id: r.get(0)?,
            name: r.get(1)?,
            user: r.get(2)?,
            created: r.get(3)?,
            last_used: r.get(4)?,
        })
    })?
    .collect()
}

/// A new API token acting as `user`; the secret is only ever shown here.
pub fn add_token(db: &Connection, user: i64, name: &str) -> rusqlite::Result<(i64, String)> {
    let s = format!("ect_{}", secret());
    db.execute(
        "INSERT INTO tokens (name, hash, user_id, created) VALUES (?, ?, ?, ?)",
        params![name.trim(), sha256(s.as_bytes()), user, db::now()],
    )?;
    Ok((db.last_insert_rowid(), s))
}

pub fn users(db: &Connection) -> rusqlite::Result<Vec<User>> {
    let mut q = db.prepare("SELECT id, name, admin FROM users ORDER BY name COLLATE NOCASE")?;
    q.query_map([], user_row)?.collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passwords_sessions_and_tokens() {
        let db = db::open(std::path::Path::new(":memory:")).unwrap_or_else(|e| panic!("{e}"));
        assert!(add_user(&db, "Ann", "short", true).is_err());
        let id = add_user(&db, "Ann", "a long password", true).unwrap_or_else(|e| panic!("{e}"));
        assert!(add_user(&db, "ann", "another password", false).is_err());
        assert_eq!(login(&db, "Ann", "wrong password").ok().flatten(), None);
        let s = login(&db, "ann", "a long password")
            .ok()
            .flatten()
            .unwrap_or_default();
        assert_eq!(by_session(&db, &s).ok().flatten().map(|u| u.id), Some(id));
        logout(&db, &s).unwrap_or_else(|e| panic!("{e}"));
        assert!(by_session(&db, &s).ok().flatten().is_none());
        let (_, t) = add_token(&db, id, "odoo").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            by_token(&db, &t).ok().flatten().map(|u| u.name),
            Some("Ann".into())
        );
        assert!(by_token(&db, "ect_nope").ok().flatten().is_none());
    }
}
