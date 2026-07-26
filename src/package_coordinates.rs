pub const VERSION_CONSTRAINT: &str = "^0.1";
pub const LOCAL_VERSION: &str = "0.1.0";
pub const DESKTOP_VERSION_CONSTRAINT: &str = "^1.1";
pub const DESKTOP_LOCAL_VERSION: &str = "1.1.0";
pub const NATIVE_VERSION_CONSTRAINT: &str = "^0.2";
pub const NATIVE_LOCAL_VERSION: &str = "0.2.1";
pub const MOBILE_UI_VERSION_CONSTRAINT: &str = "^0.2";
pub const MOBILE_UI_LOCAL_VERSION: &str = "0.2.1";

pub const CORE_API: &str = "pushinbr/pam-core-api";
pub const API: &str = "pushinbr/pam-api";
pub const SOCKET: &str = "pushinbr/pam-socket";
pub const PSR_BRIDGE: &str = "pushinbr/pam-psr-bridge";
pub const TESTING: &str = "pushinbr/pam-testing";
pub const SKELETON: &str = "pushinbr/pam-skeleton";
pub const LARAVEL: &str = "pushinbr/pam-laravel";
pub const DESKTOP: &str = "pushinbr/pam-desktop";
pub const NATIVE: &str = "pushinbr/pam-native";
pub const MOBILE_UI: &str = "pushinbr/pam-mobile-ui";

pub const ALL: [&str; 8] = [
    CORE_API, API, SOCKET, PSR_BRIDGE, TESTING, SKELETON, LARAVEL, DESKTOP,
];

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    #[test]
    fn coordinates_match_the_publication_manifest() {
        let manifest: serde_json::Value =
            serde_json::from_str(include_str!("../packages/packages.json")).unwrap();
        let published = manifest["packages"]
            .as_array()
            .unwrap()
            .iter()
            .chain(manifest["runtimePackages"].as_array().unwrap())
            .map(|package| package["name"].as_str().unwrap())
            .collect::<BTreeSet<_>>();
        let runtime = super::ALL.into_iter().collect::<BTreeSet<_>>();

        assert_eq!(runtime, published);
        assert!(runtime.iter().all(|name| name.starts_with("pushinbr/pam-")));
        assert_eq!(
            manifest["runtimePackages"][0]["constraint"],
            super::DESKTOP_VERSION_CONSTRAINT
        );
    }
}
