//! Error classification module for camgrab-core.
//!
//! This module provides a comprehensive error classification system to categorize
//! and provide actionable suggestions for common camera-related errors.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Categorizes errors into common types for better error handling and user feedback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCategory {
    /// Authentication or authorization failures (401, 403, etc.)
    Auth,
    /// Connection refused errors
    NetworkRefused,
    /// Timeout errors
    NetworkTimeout,
    /// Resource not found errors (404, etc.)
    NotFound,
    /// Codec or format errors
    CodecError,
    /// I/O errors (permissions, disk space, etc.)
    IoError,
    /// Unknown or uncategorized errors
    Unknown,
}

impl ErrorCategory {
    /// Returns all possible error categories.
    pub fn all() -> &'static [ErrorCategory] {
        &[
            ErrorCategory::Auth,
            ErrorCategory::NetworkRefused,
            ErrorCategory::NetworkTimeout,
            ErrorCategory::NotFound,
            ErrorCategory::CodecError,
            ErrorCategory::IoError,
            ErrorCategory::Unknown,
        ]
    }
}

impl fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let description = match self {
            ErrorCategory::Auth => "authentication failure",
            ErrorCategory::NetworkRefused => "connection refused",
            ErrorCategory::NetworkTimeout => "network timeout",
            ErrorCategory::NotFound => "resource not found",
            ErrorCategory::CodecError => "codec error",
            ErrorCategory::IoError => "I/O error",
            ErrorCategory::Unknown => "unknown error",
        };
        write!(f, "{description}")
    }
}

/// Classifies an error message into a category based on keyword matching.
///
/// # Arguments
///
/// * `message` - The error message to classify
///
/// # Returns
///
/// The most appropriate `ErrorCategory` for the given error message.
///
/// # Examples
///
/// ```
/// use camgrab_core::error::{classify_error, ErrorCategory};
///
/// assert_eq!(classify_error("401 Unauthorized"), ErrorCategory::Auth);
/// assert_eq!(classify_error("connection refused"), ErrorCategory::NetworkRefused);
/// assert_eq!(classify_error("request timed out"), ErrorCategory::NetworkTimeout);
/// ```
pub fn classify_error(message: &str) -> ErrorCategory {
    let lower = message.to_lowercase();

    // Check for authentication errors
    if lower.contains("401")
        || lower.contains("unauthorized")
        || lower.contains("not authorized")
        || lower.contains("authentication")
        || lower.contains("forbidden")
        || lower.contains("403")
    {
        return ErrorCategory::Auth;
    }

    // Check for connection refused errors
    if lower.contains("connection refused") || lower.contains("refused") {
        return ErrorCategory::NetworkRefused;
    }

    // Check for timeout errors
    if lower.contains("timed out")
        || lower.contains("timeout")
        || lower.contains("deadline exceeded")
    {
        return ErrorCategory::NetworkTimeout;
    }

    // Check for not found errors
    if lower.contains("not found") || lower.contains("404") || lower.contains("no such") {
        return ErrorCategory::NotFound;
    }

    // Check for codec errors
    if lower.contains("codec") || lower.contains("unsupported format") || lower.contains("encoding")
    {
        return ErrorCategory::CodecError;
    }

    // Check for I/O errors
    if lower.contains("permission denied")
        || lower.contains("no space")
        || lower.contains("disk full")
    {
        return ErrorCategory::IoError;
    }

    // Default to unknown
    ErrorCategory::Unknown
}

/// Returns an actionable suggestion for a given error category.
///
/// # Arguments
///
/// * `category` - The error category to get a suggestion for
///
/// # Returns
///
/// A string containing an actionable suggestion for addressing the error.
///
/// # Examples
///
/// ```
/// use camgrab_core::error::{suggestion_for, ErrorCategory};
///
/// let suggestion = suggestion_for(ErrorCategory::Auth);
/// assert!(suggestion.contains("username and password"));
/// ```
pub fn suggestion_for(category: ErrorCategory) -> String {
    match category {
        ErrorCategory::Auth => {
            "Check your username and password. Verify that the camera's authentication \
             credentials are correct and that the account has sufficient permissions."
                .to_string()
        }
        ErrorCategory::NetworkRefused => {
            "Verify the camera is powered on and reachable on the network. Check that the \
             IP address and port are correct, and ensure no firewall is blocking the connection."
                .to_string()
        }
        ErrorCategory::NetworkTimeout => {
            "The camera is not responding in time. Check your network connection, verify the \
             camera is online, and consider increasing timeout values if the network is slow."
                .to_string()
        }
        ErrorCategory::NotFound => {
            "The requested resource does not exist. Verify the camera URL, stream path, or \
             endpoint configuration. Check that the camera supports the requested feature."
                .to_string()
        }
        ErrorCategory::CodecError => {
            "The media format is not supported. Check that the camera's video codec settings \
             are compatible (H.264 or H.265 recommended). Try adjusting the camera's encoding \
             settings or updating your software."
                .to_string()
        }
        ErrorCategory::IoError => {
            "A file system error occurred. Check disk space availability, verify write \
             permissions for the output directory, and ensure the file system is not read-only."
                .to_string()
        }
        ErrorCategory::Unknown => {
            "An unexpected error occurred. Check the error message details, review the logs \
             for more information, and consider reporting this issue if it persists."
                .to_string()
        }
    }
}

/// A classified error with category, message, and actionable suggestion.
///
/// This struct wraps an error message with its classification and provides
/// helpful suggestions for resolving the issue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassifiedError {
    /// The error category
    pub category: ErrorCategory,
    /// The original error message
    pub message: String,
    /// An actionable suggestion for resolving the error
    pub suggestion: String,
}

impl ClassifiedError {
    /// Creates a new classified error from an error message.
    ///
    /// # Arguments
    ///
    /// * `message` - The error message to classify
    ///
    /// # Examples
    ///
    /// ```
    /// use camgrab_core::error::ClassifiedError;
    ///
    /// let error = ClassifiedError::new("401 Unauthorized");
    /// assert_eq!(error.category, camgrab_core::error::ErrorCategory::Auth);
    /// assert!(error.suggestion.contains("username and password"));
    /// ```
    pub fn new(message: impl Into<String>) -> Self {
        let message = message.into();
        let category = classify_error(&message);
        let suggestion = suggestion_for(category);

        ClassifiedError {
            category,
            message,
            suggestion,
        }
    }

    /// Creates a new classified error with an explicit category.
    ///
    /// # Arguments
    ///
    /// * `category` - The error category
    /// * `message` - The error message
    ///
    /// # Examples
    ///
    /// ```
    /// use camgrab_core::error::{ClassifiedError, ErrorCategory};
    ///
    /// let error = ClassifiedError::with_category(
    ///     ErrorCategory::NetworkTimeout,
    ///     "Connection timed out after 30s"
    /// );
    /// assert_eq!(error.category, ErrorCategory::NetworkTimeout);
    /// ```
    pub fn with_category(category: ErrorCategory, message: impl Into<String>) -> Self {
        let message = message.into();
        let suggestion = suggestion_for(category);

        ClassifiedError {
            category,
            message,
            suggestion,
        }
    }

    /// Creates a new classified error with a custom suggestion.
    ///
    /// # Arguments
    ///
    /// * `category` - The error category
    /// * `message` - The error message
    /// * `suggestion` - A custom suggestion
    pub fn with_custom_suggestion(
        category: ErrorCategory,
        message: impl Into<String>,
        suggestion: impl Into<String>,
    ) -> Self {
        ClassifiedError {
            category,
            message: message.into(),
            suggestion: suggestion.into(),
        }
    }
}

impl fmt::Display for ClassifiedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} ({}): {}",
            self.message, self.category, self.suggestion
        )
    }
}

impl std::error::Error for ClassifiedError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_auth_errors() {
        assert_eq!(classify_error("401 Unauthorized"), ErrorCategory::Auth);
        assert_eq!(classify_error("Authentication failed"), ErrorCategory::Auth);
        assert_eq!(classify_error("403 Forbidden"), ErrorCategory::Auth);
        assert_eq!(
            classify_error("User is not authorized to access this resource"),
            ErrorCategory::Auth
        );
    }

    #[test]
    fn test_classify_network_refused_errors() {
        assert_eq!(
            classify_error("connection refused"),
            ErrorCategory::NetworkRefused
        );
        assert_eq!(
            classify_error("Connection refused by peer"),
            ErrorCategory::NetworkRefused
        );
        assert_eq!(
            classify_error("The server refused the connection"),
            ErrorCategory::NetworkRefused
        );
    }

    #[test]
    fn test_classify_timeout_errors() {
        assert_eq!(
            classify_error("request timed out"),
            ErrorCategory::NetworkTimeout
        );
        assert_eq!(
            classify_error("Connection timeout after 30s"),
            ErrorCategory::NetworkTimeout
        );
        assert_eq!(
            classify_error("deadline exceeded"),
            ErrorCategory::NetworkTimeout
        );
    }

    #[test]
    fn test_classify_not_found_errors() {
        assert_eq!(classify_error("404 Not Found"), ErrorCategory::NotFound);
        assert_eq!(
            classify_error("Resource not found"),
            ErrorCategory::NotFound
        );
        assert_eq!(
            classify_error("No such file or directory"),
            ErrorCategory::NotFound
        );
    }

    #[test]
    fn test_classify_codec_errors() {
        assert_eq!(
            classify_error("Unsupported codec"),
            ErrorCategory::CodecError
        );
        assert_eq!(
            classify_error("Unsupported format: MJPEG"),
            ErrorCategory::CodecError
        );
        assert_eq!(
            classify_error("Encoding error occurred"),
            ErrorCategory::CodecError
        );
    }

    #[test]
    fn test_classify_io_errors() {
        assert_eq!(classify_error("Permission denied"), ErrorCategory::IoError);
        assert_eq!(
            classify_error("No space left on device"),
            ErrorCategory::IoError
        );
        assert_eq!(classify_error("Disk full"), ErrorCategory::IoError);
    }

    #[test]
    fn test_classify_unknown_errors() {
        assert_eq!(
            classify_error("Something went wrong"),
            ErrorCategory::Unknown
        );
        assert_eq!(
            classify_error("Unexpected error occurred"),
            ErrorCategory::Unknown
        );
    }

    #[test]
    fn test_case_insensitive_classification() {
        assert_eq!(classify_error("401 UNAUTHORIZED"), ErrorCategory::Auth);
        assert_eq!(
            classify_error("CONNECTION REFUSED"),
            ErrorCategory::NetworkRefused
        );
        assert_eq!(classify_error("TIMED OUT"), ErrorCategory::NetworkTimeout);
    }

    #[test]
    fn test_error_category_display() {
        assert_eq!(ErrorCategory::Auth.to_string(), "authentication failure");
        assert_eq!(
            ErrorCategory::NetworkRefused.to_string(),
            "connection refused"
        );
        assert_eq!(ErrorCategory::NetworkTimeout.to_string(), "network timeout");
        assert_eq!(ErrorCategory::NotFound.to_string(), "resource not found");
        assert_eq!(ErrorCategory::CodecError.to_string(), "codec error");
        assert_eq!(ErrorCategory::IoError.to_string(), "I/O error");
        assert_eq!(ErrorCategory::Unknown.to_string(), "unknown error");
    }

    #[test]
    fn test_suggestion_for_auth() {
        let suggestion = suggestion_for(ErrorCategory::Auth);
        assert!(suggestion.contains("username and password"));
        assert!(suggestion.contains("credentials"));
    }

    #[test]
    fn test_suggestion_for_network_refused() {
        let suggestion = suggestion_for(ErrorCategory::NetworkRefused);
        assert!(suggestion.contains("powered on"));
        assert!(suggestion.contains("reachable"));
        assert!(suggestion.contains("firewall"));
    }

    #[test]
    fn test_suggestion_for_timeout() {
        let suggestion = suggestion_for(ErrorCategory::NetworkTimeout);
        assert!(suggestion.contains("not responding"));
        assert!(suggestion.contains("network connection"));
    }

    #[test]
    fn test_suggestion_for_not_found() {
        let suggestion = suggestion_for(ErrorCategory::NotFound);
        assert!(suggestion.contains("does not exist"));
        assert!(suggestion.contains("URL"));
    }

    #[test]
    fn test_suggestion_for_codec() {
        let suggestion = suggestion_for(ErrorCategory::CodecError);
        assert!(suggestion.contains("format"));
        assert!(suggestion.contains("codec"));
        assert!(suggestion.contains("H.264"));
    }

    #[test]
    fn test_suggestion_for_io() {
        let suggestion = suggestion_for(ErrorCategory::IoError);
        assert!(suggestion.contains("disk space"));
        assert!(suggestion.contains("permissions"));
    }

    #[test]
    fn test_suggestion_for_unknown() {
        let suggestion = suggestion_for(ErrorCategory::Unknown);
        assert!(suggestion.contains("unexpected"));
        assert!(suggestion.contains("logs"));
    }

    #[test]
    fn test_classified_error_new() {
        let error = ClassifiedError::new("401 Unauthorized");
        assert_eq!(error.category, ErrorCategory::Auth);
        assert_eq!(error.message, "401 Unauthorized");
        assert!(error.suggestion.contains("username and password"));
    }

    #[test]
    fn test_classified_error_with_category() {
        let error = ClassifiedError::with_category(
            ErrorCategory::NetworkTimeout,
            "Connection timed out after 30s",
        );
        assert_eq!(error.category, ErrorCategory::NetworkTimeout);
        assert_eq!(error.message, "Connection timed out after 30s");
        assert!(error.suggestion.contains("not responding"));
    }

    #[test]
    fn test_classified_error_with_custom_suggestion() {
        let error = ClassifiedError::with_custom_suggestion(
            ErrorCategory::Auth,
            "Login failed",
            "Try using admin credentials",
        );
        assert_eq!(error.category, ErrorCategory::Auth);
        assert_eq!(error.message, "Login failed");
        assert_eq!(error.suggestion, "Try using admin credentials");
    }

    #[test]
    fn test_classified_error_display() {
        let error = ClassifiedError::new("401 Unauthorized");
        let display = format!("{error}");
        assert!(display.contains("401 Unauthorized"));
        assert!(display.contains("authentication failure"));
        assert!(display.contains("username and password"));
    }

    #[test]
    fn test_classified_error_equality() {
        let error1 = ClassifiedError::new("401 Unauthorized");
        let error2 = ClassifiedError::new("401 Unauthorized");
        assert_eq!(error1, error2);
    }

    #[test]
    fn test_classified_error_serialization() {
        let error = ClassifiedError::new("401 Unauthorized");
        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("auth"));
        assert!(json.contains("401 Unauthorized"));

        let deserialized: ClassifiedError = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, error);
    }

    #[test]
    fn test_error_category_serialization() {
        let category = ErrorCategory::Auth;
        let json = serde_json::to_string(&category).unwrap();
        assert_eq!(json, "\"auth\"");

        let deserialized: ErrorCategory = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, category);
    }

    #[test]
    fn test_error_category_all() {
        let all = ErrorCategory::all();
        assert_eq!(all.len(), 7);
        assert!(all.contains(&ErrorCategory::Auth));
        assert!(all.contains(&ErrorCategory::NetworkRefused));
        assert!(all.contains(&ErrorCategory::NetworkTimeout));
        assert!(all.contains(&ErrorCategory::NotFound));
        assert!(all.contains(&ErrorCategory::CodecError));
        assert!(all.contains(&ErrorCategory::IoError));
        assert!(all.contains(&ErrorCategory::Unknown));
    }

    #[test]
    fn test_multiple_keywords_in_message() {
        // Should prioritize first match (Auth over NetworkRefused)
        assert_eq!(
            classify_error("401 Unauthorized: connection refused"),
            ErrorCategory::Auth
        );
    }

    #[test]
    fn test_partial_keyword_matching() {
        // "refused" should match even within "connection refused"
        assert_eq!(
            classify_error("TCP connection was refused by host"),
            ErrorCategory::NetworkRefused
        );
    }

    #[test]
    fn test_real_world_error_messages() {
        // Retina RTSP errors
        assert_eq!(
            classify_error("RTSP DESCRIBE request failed: 401"),
            ErrorCategory::Auth
        );
        assert_eq!(
            classify_error("RTSP connection timeout"),
            ErrorCategory::NetworkTimeout
        );

        // HTTP errors
        assert_eq!(
            classify_error("HTTP request failed with status 404"),
            ErrorCategory::NotFound
        );

        // FFmpeg errors
        assert_eq!(
            classify_error("Unsupported codec: mjpeg"),
            ErrorCategory::CodecError
        );

        // System errors
        assert_eq!(
            classify_error("Failed to write file: Permission denied"),
            ErrorCategory::IoError
        );
    }
}
