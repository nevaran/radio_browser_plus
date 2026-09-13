// Library exports
pub mod auth;
pub mod domain;
pub mod error;
pub mod features;
pub mod genres;
pub mod infra;

pub use auth::{AuthService, CreateUserRequest, LoginRequest, User, UserRole};
pub use error::{AppError, Result};
