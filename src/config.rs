use serde_derive::{Deserialize, Serialize};

const DEFAULT_CACHE_SIZE: &str = "10%";

fn default_cache_size() -> String {
    DEFAULT_CACHE_SIZE.to_owned()
}

pub fn parse_cache_size(specification: &str, total_host_bytes: u64) -> Result<usize, String> {
    let specification = specification.trim().to_ascii_uppercase();
    if specification.is_empty() {
        return Err("cache_size must not be empty".to_owned());
    }

    let bytes = if let Some(percent) = specification.strip_suffix('%') {
        let percent = percent
            .parse::<u64>()
            .map_err(|_| format!("invalid cache_size percentage: {specification}"))?;
        if !(1..=100).contains(&percent) {
            return Err("cache_size percentage must be between 1% and 100%".to_owned());
        }
        if total_host_bytes == 0 {
            return Err("total host memory is unavailable".to_owned());
        }
        (u128::from(total_host_bytes) * u128::from(percent)) / 100
    } else {
        let (amount, multiplier) = if let Some(amount) = specification.strip_suffix("MB") {
            (amount, 1_000_000_u128)
        } else if let Some(amount) = specification.strip_suffix("GB") {
            (amount, 1_000_000_000_u128)
        } else if let Some(amount) = specification.strip_suffix('M') {
            (amount, 1_000_000_u128)
        } else if let Some(amount) = specification.strip_suffix('G') {
            (amount, 1_000_000_000_u128)
        } else {
            return Err("cache_size must end in M, MB, G, GB, or %".to_owned());
        };
        let amount = amount
            .parse::<u128>()
            .map_err(|_| format!("invalid cache_size amount: {specification}"))?;
        if amount == 0 {
            return Err("cache_size must be greater than zero".to_owned());
        }
        amount
            .checked_mul(multiplier)
            .ok_or_else(|| "cache_size overflows the platform size".to_owned())?
    };

    if bytes == 0 {
        return Err("cache_size resolves to zero bytes".to_owned());
    }

    usize::try_from(bytes).map_err(|_| "cache_size overflows the platform size".to_owned())
}

pub const CONFIG_VERSION: i32 = 3;

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
pub struct Meili {
    pub url: String,
    pub key: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Configuration {
    pub version: i32,
    pub osu_username: String,
    pub osu_password: String,
    pub osu_access_token: String,
    pub osu_refresh_token: String,
    pub osu_token_expires_at: i64,
    pub cursor: String,
    pub meilisearch: Meili,
    pub beatmaps_folder: String,
    #[serde(default = "default_cache_size")]
    pub cache_size: String,
}

impl ::std::default::Default for Configuration {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            osu_username: String::new(),
            osu_password: String::new(),
            osu_access_token: String::new(),
            osu_refresh_token: String::new(),
            osu_token_expires_at: 0,
            cursor: String::new(),
            meilisearch: Default::default(),
            beatmaps_folder: String::new(),
            cache_size: default_cache_size(),
        }
    }
}

impl Configuration {
    pub fn has_authorization(&self) -> bool {
        return !self.osu_access_token.is_empty() && !self.osu_refresh_token.is_empty();
    }
}

#[derive(clap::Parser, Clone)]
pub struct Config {
    #[clap(long, env)]
    pub app_component: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_size_accepts_decimal_units_and_injected_host_percentages() {
        assert_eq!(parse_cache_size("2048MB", 1).unwrap(), 2_048_000_000);
        assert_eq!(parse_cache_size(" 4gb ", 1).unwrap(), 4_000_000_000);
        assert_eq!(parse_cache_size("1G", 1).unwrap(), 1_000_000_000);
        assert_eq!(parse_cache_size("512M", 1).unwrap(), 512_000_000);
        assert_eq!(parse_cache_size("10%", 8_000_000_000).unwrap(), 800_000_000);
        assert_eq!(parse_cache_size("100%", 1234).unwrap(), 1234);
    }

    #[test]
    fn cache_size_rejects_zero_malformed_percentages_and_overflow() {
        for invalid in ["", "0MB", "0%", "101%", "-1GB", "4GiB", "ten%", "MB"] {
            assert!(
                parse_cache_size(invalid, 8_000_000_000).is_err(),
                "{invalid}"
            );
        }
        assert!(parse_cache_size("10%", 0).is_err());
        assert!(parse_cache_size("1%", 1).is_err());
        assert!(parse_cache_size(&format!("{}GB", u128::MAX), 1).is_err());
    }

    #[test]
    fn missing_cache_size_deserializes_to_the_backward_compatible_default() {
        let mut value = serde_json::to_value(Configuration::default()).unwrap();
        value.as_object_mut().unwrap().remove("cache_size");
        let configuration: Configuration = serde_json::from_value(value).unwrap();
        assert_eq!(configuration.cache_size, DEFAULT_CACHE_SIZE);
    }
}
