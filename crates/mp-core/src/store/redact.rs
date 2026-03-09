use regex::Regex;
use std::sync::LazyLock;

const REDACTED: &str = "[REDACTED]";

struct Pattern {
    regex: Regex,
    _label: &'static str,
}

macro_rules! pat {
    ($label:expr, $re:expr) => {
        Pattern {
            regex: Regex::new($re).unwrap(),
            _label: $label,
        }
    };
}

static PATTERNS: LazyLock<Vec<Pattern>> = LazyLock::new(|| {
    vec![
        // 1. OpenAI API keys
        pat!("openai_key", r"sk-[A-Za-z0-9_-]{20,}"),
        // 2. AWS access key IDs
        pat!("aws_access_key", r"AKIA[0-9A-Z]{16}"),
        // 3. AWS secret access keys (in assignments)
        pat!(
            "aws_secret_key",
            r"(?i)aws_secret_access_key\s*[=:]\s*[A-Za-z0-9/+=]{30,}"
        ),
        // 4. GCP API keys
        pat!("gcp_api_key", r"AIza[0-9A-Za-z_-]{35}"),
        // 5. GCP service account JSON private key
        pat!(
            "gcp_private_key",
            r"-----BEGIN (RSA |EC )?PRIVATE KEY-----[\s\S]*?-----END (RSA |EC )?PRIVATE KEY-----"
        ),
        // 6. Azure connection strings
        pat!(
            "azure_conn_str",
            r"(?i)DefaultEndpointsProtocol=https?;AccountName=[^;]+;AccountKey=[^;]+"
        ),
        // 7. Stripe API keys
        pat!("stripe_key", r"(?:sk|pk|rk)_(test|live)_[0-9a-zA-Z]{10,}"),
        // 8. Snowflake tokens
        pat!("snowflake_token", r"(?i)snowflake[_\s]*token\s*[=:]\s*\S+"),
        // 9. Snowflake connection strings
        pat!("snowflake_conn", r"(?i)snowflake://[^\s]+"),
        // 10. JWTs (three base64url dot-separated segments)
        pat!(
            "jwt",
            r"eyJ[A-Za-z0-9_-]+\.eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+"
        ),
        // 11. Bearer tokens
        pat!("bearer_token", r"(?i)bearer\s+[A-Za-z0-9_\-.~+/]+=*"),
        // 12. PEM certificates / keys (catch-all for BEGIN...END blocks)
        pat!(
            "pem_block",
            r"-----BEGIN [A-Z ]+-----[\s\S]*?-----END [A-Z ]+-----"
        ),
        // 13. Generic password assignments
        pat!(
            "password_assign",
            r#"(?i)password\s*[=:]\s*["']?[^\s"']{8,}["']?"#
        ),
        // 14. Generic secret assignments
        pat!(
            "secret_assign",
            r#"(?i)secret\s*[=:]\s*["']?[^\s"']{8,}["']?"#
        ),
        // 15. Generic token assignments
        pat!(
            "token_assign",
            r#"(?i)token\s*[=:]\s*["']?[^\s"']{8,}["']?"#
        ),
        // 16. Database connection URIs (postgres, mysql, mongodb, redis)
        pat!(
            "db_uri",
            r"(?i)(postgres|postgresql|mysql|mongodb(\+srv)?|redis|rediss)://[^\s]+"
        ),
        // 17. GitHub personal access tokens
        pat!("github_pat", r"ghp_[A-Za-z0-9]{36}"),
        // 18. Anthropic API keys
        pat!("anthropic_key", r"sk-ant-[A-Za-z0-9_-]{20,}"),
    ]
});

/// Redact secrets from text. Returns the redacted text.
/// This is always-on, non-configurable, and runs before any data is written.
pub fn redact(text: &str) -> String {
    let mut result = text.to_string();
    for pattern in PATTERNS.iter() {
        result = pattern.regex.replace_all(&result, REDACTED).to_string();
    }
    result
}

/// Check if text contains any secrets.
pub fn contains_secrets(text: &str) -> bool {
    PATTERNS.iter().any(|p| p.regex.is_match(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_openai_key() {
        let input = "My key is sk-abc123XYZ789longkeyvalue and it works";
        let out = redact(input);
        assert!(!out.contains("sk-abc123"));
        assert!(out.contains(REDACTED));
    }

    #[test]
    fn redacts_aws_access_key() {
        let input = "AWS key: AKIAIOSFODNN7EXAMPLE";
        let out = redact(input);
        assert!(!out.contains("AKIAIOSFODNN7EXAMPLE"));
        assert!(out.contains(REDACTED));
    }

    #[test]
    fn redacts_aws_secret_key() {
        let input = "aws_secret_access_key = wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";
        let out = redact(input);
        assert!(!out.contains("wJalrXUtnFEMI"));
        assert!(out.contains(REDACTED));
    }

    #[test]
    fn redacts_gcp_api_key() {
        let input = "key=AIzaSyD-abc123_some-Long-Key-Value-Here";
        let out = redact(input);
        assert!(!out.contains("AIzaSyD"));
        assert!(out.contains(REDACTED));
    }

    #[test]
    fn redacts_pem_private_key() {
        let input =
            "-----BEGIN RSA PRIVATE KEY-----\nMIIBogIBAAJBALRi...\n-----END RSA PRIVATE KEY-----";
        let out = redact(input);
        assert!(!out.contains("MIIBogIBAAJBALRi"));
        assert!(out.contains(REDACTED));
    }

    #[test]
    fn redacts_azure_connection_string() {
        let input = "DefaultEndpointsProtocol=https;AccountName=myaccount;AccountKey=abc123==";
        let out = redact(input);
        assert!(!out.contains("AccountKey=abc123"));
        assert!(out.contains(REDACTED));
    }

    #[test]
    fn redacts_stripe_key() {
        let input = "stripe key: sk_test_4eC39HqLyjWDarjtT1zdp7dc";
        let out = redact(input);
        assert!(!out.contains("sk_test_4eC39"));
        assert!(out.contains(REDACTED));
    }

    #[test]
    fn redacts_snowflake_token() {
        let input = "snowflake_token = abc123secretvalue";
        let out = redact(input);
        assert!(!out.contains("abc123secretvalue"));
        assert!(out.contains(REDACTED));
    }

    #[test]
    fn redacts_snowflake_connection_string() {
        let input = "Use snowflake://user:pass@account.snowflakecomputing.com/db";
        let out = redact(input);
        assert!(!out.contains("user:pass@account"));
        assert!(out.contains(REDACTED));
    }

    #[test]
    fn redacts_jwt() {
        let input = "token: eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U";
        let out = redact(input);
        assert!(!out.contains("eyJhbGciOiJI"));
        assert!(out.contains(REDACTED));
    }

    #[test]
    fn redacts_bearer_token() {
        let input = "Authorization: Bearer eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.abc.def";
        let out = redact(input);
        assert!(!out.contains("eyJhbGciOiJSUzI1NiI"));
        assert!(out.contains(REDACTED));
    }

    #[test]
    fn redacts_password_assignment() {
        let input = r#"password = "SuperSecret123!""#;
        let out = redact(input);
        assert!(!out.contains("SuperSecret123!"));
        assert!(out.contains(REDACTED));
    }

    #[test]
    fn redacts_secret_assignment() {
        let input = "secret: my-very-secret-value-here";
        let out = redact(input);
        assert!(!out.contains("my-very-secret-value-here"));
        assert!(out.contains(REDACTED));
    }

    #[test]
    fn redacts_token_assignment() {
        let input = "token = 'abcdef1234567890'";
        let out = redact(input);
        assert!(!out.contains("abcdef1234567890"));
        assert!(out.contains(REDACTED));
    }

    #[test]
    fn redacts_postgres_uri() {
        let input = "DATABASE_URL=postgres://user:password@host:5432/dbname";
        let out = redact(input);
        assert!(!out.contains("user:password@host"));
        assert!(out.contains(REDACTED));
    }

    #[test]
    fn redacts_mongodb_uri() {
        let input = "mongodb+srv://admin:pass@cluster0.example.net/mydb";
        let out = redact(input);
        assert!(!out.contains("admin:pass"));
        assert!(out.contains(REDACTED));
    }

    #[test]
    fn redacts_github_pat() {
        let input = "Use ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij for auth";
        let out = redact(input);
        assert!(!out.contains("ghp_ABCDEFGH"));
        assert!(out.contains(REDACTED));
    }

    #[test]
    fn redacts_anthropic_key() {
        let input = "ANTHROPIC_KEY=sk-ant-abcdef1234567890abcdefgh";
        let out = redact(input);
        assert!(!out.contains("sk-ant-abcdef"));
        assert!(out.contains(REDACTED));
    }

    #[test]
    fn preserves_safe_text() {
        let input = "The ORDERS table uses soft deletes via a deleted_at timestamp column.";
        let out = redact(input);
        assert_eq!(out, input);
    }

    #[test]
    fn contains_secrets_detects() {
        assert!(contains_secrets("key is sk-abc123XYZ789longkeyvalue"));
        assert!(!contains_secrets("just a normal sentence"));
    }

    #[test]
    fn multiple_secrets_in_one_string() {
        let input = "key sk-abc123longkeyvalue456 and postgres://user:pass@host:5432/db";
        let out = redact(input);
        assert!(
            !out.contains("sk-abc123"),
            "OpenAI key should be redacted: {out}"
        );
        assert!(
            !out.contains("user:pass@host"),
            "DB URI should be redacted: {out}"
        );
    }
}
