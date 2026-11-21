use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinecraftVersion {
    pub raw: String,
    pub numbers: Vec<u32>,
    pub is_stable: bool,
    pub prerelease_num: u32,
}

impl MinecraftVersion {
    pub fn parse(input: &str) -> Self {
        let parts: Vec<&str> = input.splitn(2, '-').collect();
        let base = parts[0];
        let suffix = parts.get(1).copied().unwrap_or("");

        let number_regex = Regex::new(r"\d+").expect("valid regex");
        let numbers: Vec<u32> = number_regex
            .find_iter(base)
            .filter_map(|m| m.as_str().parse::<u32>().ok())
            .collect();

        let prerelease_regex =
            Regex::new(r"(?i)(rc|pre|alpha|beta|dev|snapshot)").expect("valid regex");
        let is_stable = suffix.is_empty() && !prerelease_regex.is_match(input);

        let prerelease_num = if !suffix.is_empty() {
            number_regex
                .find_iter(suffix)
                .filter_map(|m| m.as_str().parse::<u32>().ok())
                .next()
                .unwrap_or(0)
        } else {
            0
        };

        Self {
            raw: input.to_string(),
            numbers,
            is_stable,
            prerelease_num,
        }
    }

    pub fn compute_api_version(&self) -> String {
        if self.numbers.is_empty() {
            return self.raw.clone();
        }
        if self.numbers[0] == 1 && self.numbers.len() >= 2 {
            return format!("{}.{}", self.numbers[0], self.numbers[1]);
        }
        if self.numbers.len() >= 2 {
            return format!("{}.{}", self.numbers[0], self.numbers[1]);
        }
        self.raw.clone()
    }
}

impl PartialOrd for MinecraftVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MinecraftVersion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let max_len = self.numbers.len().max(other.numbers.len());
        for i in 0..max_len {
            let a = self.numbers.get(i).copied().unwrap_or(0);
            let b = other.numbers.get(i).copied().unwrap_or(0);
            match a.cmp(&b) {
                std::cmp::Ordering::Equal => continue,
                non_equal => return non_equal,
            }
        }

        match (self.is_stable, other.is_stable) {
            (true, false) => std::cmp::Ordering::Greater,
            (false, true) => std::cmp::Ordering::Less,
            (false, false) => self.prerelease_num.cmp(&other.prerelease_num),
            (true, true) => std::cmp::Ordering::Equal,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_ordering() {
        let v1 = MinecraftVersion::parse("1.21.3");
        let v2 = MinecraftVersion::parse("1.21.4");
        let v3 = MinecraftVersion::parse("26.3");
        let v4 = MinecraftVersion::parse("26.3-rc-1");
        let v5 = MinecraftVersion::parse("26.3-rc-2");

        assert!(v1 < v2);
        assert!(v2 < v3);
        assert!(v4 < v3);
        assert!(v4 < v5);
        assert!(v5 < v3);
    }

    #[test]
    fn test_api_version_computation() {
        assert_eq!(
            MinecraftVersion::parse("1.21.4").compute_api_version(),
            "1.21"
        );
        assert_eq!(
            MinecraftVersion::parse("26.3").compute_api_version(),
            "26.3"
        );
        assert_eq!(
            MinecraftVersion::parse("26.3.1").compute_api_version(),
            "26.3"
        );
    }
}
