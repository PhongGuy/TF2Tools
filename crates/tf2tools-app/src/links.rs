//! The web pages the app sends a player to for help.

/// GitHub's new-issue chooser, where a player reports a problem.
pub const REPORT_PROBLEM_URL: &str = "https://github.com/PhongGuy/TF2Tools/issues/new/choose";

/// The README, the app's help page.
pub const HELP_URL: &str = "https://github.com/PhongGuy/TF2Tools#readme";

/// GameBanana's own page for mod `id`: `profile_url` when it is on gamebanana.com over HTTPS,
/// else the page built from the id, so a listing can never send the player elsewhere.
pub fn gamebanana_page(profile_url: &str, id: tf2tools_core::GbId) -> String {
    if profile_url.starts_with("https://gamebanana.com/") {
        profile_url.to_owned()
    } else {
        format!("https://gamebanana.com/mods/{}", id.get())
    }
}

#[cfg(test)]
mod tests {
    use super::gamebanana_page;
    use tf2tools_core::GbId;

    #[test]
    fn only_a_gamebanana_profile_url_is_opened_as_given() {
        let id = GbId::new(8);
        let own = "https://gamebanana.com/mods/8";
        assert_eq!(gamebanana_page(own, id), own);
        for other in [
            "",
            "http://gamebanana.com/mods/8",
            "https://gamebanana.com.evil.example/mods/8",
            "https://evil.example/?https://gamebanana.com/",
            "javascript:alert(1)",
        ] {
            assert_eq!(gamebanana_page(other, id), own, "{other}");
        }
    }
}
